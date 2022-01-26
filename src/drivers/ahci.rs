use core::intrinsics::size_of;

use crate::arch::mm::pmm::{self, PhysAddr, PmmBox};
use crate::arch::{idt, io::Mmio, pci};
use crate::mm::vmm::{self, PageFlags, VirtAddr};
use crate::serial;
use crate::utils::math::div_ceil;
use alloc::vec::Vec;

const SATA_ATA: u32 = 0x101;
const FIS_TYPE_REG_H2D: u8 = 0x27;

const ATA_READ_DMA: u8 = 0x25;
const ATA_WRITE_DMA: u8 = 0x35;
const ATA_IDENTIFY: u8 = 0xec;

static mut AHCI_DEVICES: Vec<AhciDevice> = alloc::vec![];

#[repr(C, packed)]
struct FisRegH2D {
    fis_type: Mmio<u8>,
    mul_cmd: Mmio<u8>, // port multiplier and command/control bit
    command: Mmio<u8>,
    featurel: Mmio<u8>,
    lba0: Mmio<u8>,
    lba1: Mmio<u8>,
    lba2: Mmio<u8>,
    device: Mmio<u8>,
    lba3: Mmio<u8>,
    lba4: Mmio<u8>,
    lba5: Mmio<u8>,
    featureh: Mmio<u8>,
    countl: Mmio<u8>,
    counth: Mmio<u8>,
    icc: Mmio<u8>,
    control: Mmio<u8>,
    reserved: Mmio<u32>,
}

impl FisRegH2D {
    fn set_lba(&self, lba: u64) {
        self.lba0.set(lba as u8);
        self.lba1.set((lba >> 8) as u8);
        self.lba2.set((lba >> 16) as u8);
        self.lba3.set((lba >> 24) as u8);
        self.lba4.set((lba >> 32) as u8);
        self.lba5.set((lba >> 40) as u8);

        self.device.set(1 << 6); // use LBA addressing
    }

    fn set_count(&self, count: u16) {
        self.countl.set(count as u8);
        self.counth.set((count >> 8) as u8);
    }
}

#[repr(C, packed)]
struct CommandHeader {
    cfl_awp: Mmio<u8>,
    rbc_rsv_pmp: Mmio<u8>,
    prdtl: Mmio<u16>,
    prdbc: Mmio<u32>,
    ctaddr_lower: Mmio<u32>,
    ctaddr_upper: Mmio<u32>,
    reserved: [Mmio<u32>; 4],
}

impl CommandHeader {
    fn get_command_table(&self) -> &mut CommandTable {
        let cmd_table_addr = (self.ctaddr_lower.get() as u64
            | (self.ctaddr_upper.get() as u64) << 32)
            + pmm::PHYS_BASE;

        let cmd_table = cmd_table_addr as *mut CommandTable;

        unsafe { &mut *cmd_table }
    }
}

#[repr(C, packed)]
struct CommandTable {
    cmd_fis: [u8; 64],
    atapi_cmd: [u8; 16],
    reserved: [u8; 48],
    prdt_entries: [Prdt; 1], // max is 65536
}

#[repr(C, packed)]
struct Prdt {
    data_lower: Mmio<u32>,
    data_upper: Mmio<u32>,
    reserved: Mmio<u32>,
    bc_i: Mmio<u32>,
}

impl Prdt {
    fn set_buffer(&self, address: u64, sector_cnt: u16) {
        self.data_lower.set(address as u32);
        self.data_upper.set((address >> 32) as u32);
        self.reserved.set(0);
        self.bc_i.set((sector_cnt as u32 * 512) - 1 | 1 << 31); // sector size might not always be 512
    }
}

#[repr(C, packed)]
struct ControllerRegisters {
    capabilities: Mmio<u32>,
    ghc: Mmio<u32>,
    interrupt_status: Mmio<u32>,
    port_implemented: Mmio<u32>,
    version: Mmio<u32>,
    ccc_ctl: Mmio<u32>,
    ccc_ports: Mmio<u32>,
    em_loc: Mmio<u32>,
    em_ctl: Mmio<u32>,
    capabilities2: Mmio<u32>,
    bohc: Mmio<u32>,
    reserved: [Mmio<u32>; 29],
    vendor_specific: [Mmio<u32>; 24],
    ports: [PortRegisters; 32],
}

#[repr(C, packed)]
struct PortRegisters {
    clb_lower: Mmio<u32>,
    clb_higher: Mmio<u32>,
    fb_lower: Mmio<u32>,
    fb_higher: Mmio<u32>,
    interrupt_status: Mmio<u32>,
    interrupt_enable: Mmio<u32>,
    cmd: Mmio<u32>,
    reserved: Mmio<u32>,
    tfd: Mmio<u32>,
    signature: Mmio<u32>,
    ssts: Mmio<u32>,
    sctl: Mmio<u32>,
    serr: Mmio<u32>,
    sact: Mmio<u32>,
    ci: Mmio<u32>,
    sntf: Mmio<u32>,
    fbs: Mmio<u32>,
    dev_sleep: Mmio<u32>,
    reserved2: [Mmio<u32>; 11],
    vendor_specific: [Mmio<u32>; 4],
}

impl PortRegisters {
    fn get_command_header(&self, slot: u8) -> &mut CommandHeader {
        let cmd_header_addr =
            (self.clb_lower.get() as u64 | (self.clb_higher.get() as u64) << 32) + pmm::PHYS_BASE;

        let cmd_header = cmd_header_addr as *mut CommandHeader;

        unsafe { &mut *cmd_header.offset(slot as isize) }
    }

    fn get_slot(&self) -> Option<u8> {
        for i in 0..32 {
            if ((self.sact.get() | self.ci.get()) & (1 << i)) == 0 {
                return Some(i);
            }
        }

        None
    }

    // TODO: zero structs
    // if it succeeds, it will return the number of bytes read/written
    // max number of bytes that can be read/written with one command is 4MB (only 1 prdt is used)
    pub fn send_command(
        &self,
        lba: u64,
        sectors: u16,
        buffer: *mut u8,
        write: bool,
    ) -> Result<usize, ()> {
        let slot = self
            .get_slot()
            .expect("Could not get a slot fot the AHCI command");

        let cmd_header = self.get_command_header(slot);
        cmd_header.cfl_awp.set((size_of::<FisRegH2D>() / 4) as u8);
        if write {
            cmd_header.cfl_awp.set(cmd_header.cfl_awp.get() | 1 << 6);
        }
        cmd_header.prdtl.set(1);

        let cmd_table = cmd_header.get_command_table();

        let buffer_addr = buffer as u64 & !pmm::PHYS_BASE;
        cmd_table.prdt_entries[0].set_buffer(buffer_addr, sectors);

        let fis = unsafe { &mut *(cmd_table.cmd_fis.as_mut_ptr() as *mut FisRegH2D) };
        fis.fis_type.set(FIS_TYPE_REG_H2D);
        fis.mul_cmd.set(1 << 7); // specifies that it is a command
        fis.command
            .set(if write { ATA_WRITE_DMA } else { ATA_READ_DMA });

        fis.set_lba(lba); // this will also set the lba addressing
        fis.set_count(sectors as u16);

        self.ci.set(1 << slot);

        while self.ci.get() & (1 << slot) != 0 {
            if self.interrupt_status.get() & (1 << 30) != 0 {
                serial::print!("[AHCI] error while executing a command\n");
                serial::print!("1\n");
                serial::print!("LBA: {}, sectors: {}, buffer: {:?}\n", lba, sectors, buffer);
                return Err(());
            }
        }

        if self.interrupt_status.get() & (1 << 30) != 0 {
            serial::print!("[AHCI] error while executing a command\n");
            serial::print!("2\n");
            serial::print!("LBA: {}, sectors: {}, buffer: {:?}\n", lba, sectors, buffer);
            return Err(());
        }

        serial::print!("bytes read: {}\n", cmd_header.prdbc.get());
        serial::print!("AHCI access completed\n");
        Ok(cmd_header.prdbc.get() as usize)
    }
}

struct AhciDevice {
    pub regs: &'static mut PortRegisters,
}

impl AhciDevice {
    // we use the clb and fb provided by the firmware
    unsafe fn new(regs: &'static mut PortRegisters) -> Self {
        /*
            get an interrupt once we receive a device to host FIS,
            which should indicate that a transfer has been completed
        */
        regs.interrupt_enable.set(regs.interrupt_enable.get() | 1);

        for i in 0..32 {
            let cmd_header = regs.get_command_header(i);

            let cmd_table_pages = div_ceil(size_of::<CommandTable>(), pmm::PAGE_SIZE as usize);
            let cmd_table = pmm::get()
                .calloc(cmd_table_pages)
                .expect("Could not allocate the pages needed for the command list (AHCI)")
                .as_u64();

            for i in (0..cmd_table_pages * pmm::PAGE_SIZE as usize).step_by(pmm::PAGE_SIZE as usize)
            {
                vmm::get().map_page(
                    VirtAddr::new(cmd_table + pmm::PHYS_BASE + i as u64),
                    PhysAddr::new(cmd_table + i as u64),
                    PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::UNCACHEABLE,
                    true,
                );
            }

            cmd_header.ctaddr_lower.set(cmd_table as u32);
            cmd_header.ctaddr_upper.set((cmd_table >> 32) as u32);
        }

        let device = AhciDevice { regs };
        device
    }
}

pub fn init(hba: &pci::PciDevice) {
    let bar5 = hba.get_bar(5);

    hba.bus_master();
    hba.enable_mmio();

    let hba_mem = unsafe { &mut *bar5.higher_half().as_mut_ptr::<ControllerRegisters>() };

    vmm::get().map_page(
        VirtAddr::new(bar5.higher_half().as_u64()),
        bar5,
        PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::UNCACHEABLE,
        true,
    );

    if hba_mem.capabilities.get() & (1 << 31) == 0 {
        serial::print!("The AHCI controller does not support 64 bits addressing\n");
        return;
    }

    hba_mem.ghc.set(hba_mem.ghc.get() | 2); // enable interrupts

    let vector = idt::alloc_vector().expect("[AHCI] Could not allocate an interrupt vector");
    unsafe {
        idt::register_isr(vector, ahci_isr as u64, 0, 0x8e);
    }
    hba.set_msi(vector);

    for (i, port) in hba_mem.ports.iter_mut().enumerate() {
        if hba_mem.port_implemented.get() & (1 << i) != 0 {
            if port.signature.get() == SATA_ATA {
                unsafe {
                    let device = AhciDevice::new(port);
                    serial::print!("Initialized ahci driver\n");
                    AHCI_DEVICES.push(device);
                }
            }
        }
    }
}

pub fn read(device_index: usize, offset: u64, bytes: usize, buffer: *mut u8) -> Result<usize, ()> {
    let device = unsafe { &AHCI_DEVICES[device_index] };
    let tmp_buffer = PmmBox::<u8>::new(bytes);
    let tmp_buffer_ptr = tmp_buffer.as_mut_ptr();

    /*
        bytes + (offset % 512) will make sure than unaligned reads that span more than one sector
        will work

        E.g. a read from offset 510 and with byte count of 4 needs to get the contents of 2 sectors
        in order to retrieve those 4 bytes
    */
    let sectors = div_ceil(bytes + (offset % 512) as usize, 512) as u16;

    let access_result = device
        .regs
        .send_command(offset / 512, sectors, tmp_buffer_ptr, false);

    if let Ok(bc) = access_result {
        unsafe {
            buffer.copy_from(tmp_buffer_ptr.offset((offset % 512) as isize), bytes);
        }

        Ok(bc)
    } else {
        access_result
    }
}

pub fn write(
    device_index: usize,
    offset: u64,
    bytes: usize,
    buffer: *const u8,
) -> Result<usize, ()> {
    let device = unsafe { &AHCI_DEVICES[device_index] };
    let tmp_buffer = PmmBox::<u8>::new(bytes);
    let tmp_buffer_ptr = tmp_buffer.as_mut_ptr();

    let sectors = div_ceil(bytes + (offset % 512) as usize, 512) as u16;

    let mut access_result = device
        .regs
        .send_command(offset / 512, sectors, tmp_buffer_ptr, false);

    if let Ok(_) = access_result {
        unsafe {
            tmp_buffer_ptr
                .offset((offset % 512) as isize)
                .copy_from(buffer, bytes);
        }

        access_result = device
            .regs
            .send_command(offset / 512, sectors, tmp_buffer_ptr, true);

        access_result
    } else {
        access_result
    }
}

idt::isr!(ahci_isr, |_stack| {
    serial::print!("=== Disk transfer completed ===\n");
});
