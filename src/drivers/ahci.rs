use core::intrinsics::size_of;

use crate::arch::x86_64::{mm::pmm, pci};
use crate::serial;
use crate::utils::{addr, math::div_ceil};
use alloc::vec::Vec;

#[repr(C, packed)]
struct ControllerRegisters {
    capabilities: u32,
    ghc: u32,
    interrupt_status: u32,
    port_implemented: u32,
    version: u32,
    ccc_ctl: u32,
    ccc_ports: u32,
    em_loc: u32,
    em_ctl: u32,
    capabilities2: u32,
    bohc: u32,
    reserved: [u32; 29],
    vendor_specific: [u32; 24],
    ports: [PortRegisters; 32],
}

#[repr(C, packed)]
struct PortRegisters {
    clb_lower: u32,
    clb_higher: u32,
    fb_lower: u32,
    fb_higher: u32,
    interrupt_status: u32,
    interrupt_enable: u32,
    cmd: u32,
    reserved: u32,
    tfd: u32,
    signature: u32,
    ssts: u32,
    sctl: u32,
    serr: u32,
    sact: u32,
    ci: u32,
    sntf: u32,
    fbs: u32,
    dev_sleep: u32,
    reserved2: [u32; 11],
    vendor_specific: [u32; 4],
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
struct FisRegH2D {
    fis_type: u8,
    mul_cmd: u8, // port multiplier and command/control bit
    command: u8,
    featurel: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    featureh: u8,
    countl: u8,
    counth: u8,
    icc: u8,
    control: u8,
    reserved: u32,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
struct CommandHeader {
    cfl_awp: u8,
    rbc_rsv_pmp: u8,
    prdtl: u16,
    prdbc: u32,
    ctaddr_lower: u32,
    ctaddr_upper: u32,
    reserved: [u32; 4],
}

#[repr(C, packed)]
#[derive(Debug)]
struct CommandTable {
    cmd_fis: [u8; 64],
    atapi_cmd: [u8; 16],
    reserved: [u8; 48],
    prdt_entries: [Prdt; 65536],
}

#[derive(Debug, Copy, Clone)]
struct Prdt {
    data_lower: u32,
    data_upper: u32,
    reserved: u32,
    bc_i: u32,
}

const SATA_ATA: u32 = 0x101;
const FIS_TYPE_REG_H2D: u8 = 0x27;

const ATA_READ_DMA: u8 = 0x25;
const ATA_WRITE_DMA: u8 = 0x35;
const ATA_IDENTIFY: u8 = 0xec;

static mut AHCI_DEVICES: Vec<AhciDevice> = alloc::vec![];

pub fn init(hba: &pci::PciDevice) {
    let bar5 = hba.get_bar(5);
    serial::print!("ahci bar: {:x}\n", bar5);

    hba.bus_master();
    hba.enable_mmio();

    let hba_mem;
    unsafe {
        hba_mem = &mut *(bar5 as *mut ControllerRegisters);
    }

    if hba_mem.capabilities & (1 << 31) == 0 {
        serial::print!("The ahci controller does not support 64 bits addressing\n");
        return;
    }

    // // This setup is not guaranted to work on real hardware
    // hba_mem.ghc |= 1 << 31; // AHCI enable
    // hba_mem.ghc |= 1; // reset the HBA

    // while hba_mem.ghc & 1 != 0 {}
    // hba_mem.ghc |= 1 << 31; // AHCI enable

    for i in 0..32 {
        if hba_mem.port_implemented & (1 << i) != 0 {
            serial::print!("found one\n");
            let port_mem = &mut hba_mem.ports[i];
            serial::print!("signature: {}\n", port_mem.signature);
            if port_mem.signature == SATA_ATA {
                serial::print!("yep right signature\n");
                unsafe {
                    let device = AhciDevice::new(port_mem as *mut PortRegisters);
                    serial::print!("Initialized ahci driver\n");
                    let mut mem = pmm::PAGE_ALLOCATOR.calloc(1).unwrap();
                    mem = addr::higher_half(mem);
                    let mut mem2 = pmm::PAGE_ALLOCATOR.calloc(1).unwrap();
                    mem2 = addr::higher_half(mem2);
                    mem.write_bytes(0xff, 4096);
                    device.access(1, 1, mem, true);
                    device.access(1, 1, mem2, false);
                    serial::print!("ahci result: {}\n", *(mem2 as *mut u64));
                }
            }
        }
    }

    serial::print!("down here\n");
}

struct AhciDevice {
    port_mem: *mut PortRegisters,
}

impl AhciDevice {
    unsafe fn new(port_mem: *mut PortRegisters) -> Self {
        let port_ref = &mut *port_mem;
        let cmd_list = pmm::PAGE_ALLOCATOR
            .calloc(div_ceil(
                size_of::<CommandHeader>() * 32,
                pmm::PAGE_SIZE as usize,
            ))
            .expect("Could not allocate the pages needed for the command list (AHCI)");

        port_ref.clb_lower = cmd_list as u32;
        port_ref.clb_higher = (cmd_list as u64 >> 32) as u32;

        let cmd_headers = cmd_list as *mut CommandHeader;
        for i in 0..32 {
            let cmd_table = pmm::PAGE_ALLOCATOR
                .calloc(div_ceil(size_of::<CommandTable>(), pmm::PAGE_SIZE as usize))
                .expect("Could not allocate the pages needed for the command list (AHCI)")
                as u64;

            (*cmd_headers.offset(i)).ctaddr_lower = cmd_table as u32;
            (*cmd_headers.offset(i)).ctaddr_upper = (cmd_table >> 32) as u32;
        }

        // received fis???
        serial::print!("initialized that shit successfully\n");
        let device = AhciDevice { port_mem };
        serial::print!("Ahci devices address: {:p}\n", &AHCI_DEVICES);
        // AHCI_DEVICES.push(device);
        device
    }

    unsafe fn get_slot(&self) -> Option<u8> {
        for i in 0..32 {
            if (((*self.port_mem).sact | (*self.port_mem).ci) & (1 << i)) == 0 {
                return Some(i);
            }
        }

        None
    }

    pub unsafe fn access(&self, lba: u64, sectors: usize, buffer: *mut u8, write: bool) {
        serial::print!("at device access\n");
        let port_ref = &mut *self.port_mem;

        let slot = self.get_slot().unwrap() as isize;
        serial::print!("slot: {}\n", slot);
        let cmd_header = ((port_ref.clb_lower as u64 | (port_ref.clb_higher as u64) << 32)
            + pmm::PHYS_BASE) as *mut CommandHeader;
        let cmd_header = &mut *cmd_header.offset(slot);

        cmd_header.cfl_awp = (size_of::<FisRegH2D>() / 4) as u8;
        cmd_header.cfl_awp |= if write { 1 << 6 } else { 0 };
        cmd_header.prdtl = 1;
        let cmd_table = ((cmd_header.ctaddr_lower as u64 | (cmd_header.ctaddr_upper as u64) << 32)
            + pmm::PHYS_BASE) as *mut CommandTable;
        let cmd_table = &mut *cmd_table;

        let buffer_addr = buffer as u64 - pmm::PHYS_BASE;
        cmd_table.prdt_entries[0].data_lower = buffer_addr as u32;
        cmd_table.prdt_entries[0].data_upper = (buffer_addr >> 32) as u32;
        // interrupt on completion?
        cmd_table.prdt_entries[0].bc_i = (sectors * 512 - 1) as u32 | (1 << 31); // sector size might not be 512

        let fis = &mut *(cmd_table.cmd_fis.as_mut_ptr() as *mut FisRegH2D);
        fis.fis_type = FIS_TYPE_REG_H2D;
        fis.mul_cmd = 1 << 7; // specifies that it is a command
        fis.command = if write { ATA_WRITE_DMA } else { ATA_READ_DMA };
        fis.device = 0xA0 | 1 << 6; // use LBA addressing

        fis.lba0 = lba as u8;
        fis.lba1 = (lba >> 8) as u8;
        fis.lba2 = (lba >> 16) as u8;
        fis.lba3 = (lba >> 24) as u8;
        fis.lba4 = (lba >> 32) as u8;
        fis.lba5 = (lba >> 40) as u8;

        fis.countl = sectors as u8;
        fis.counth = (sectors >> 8) as u8;
        serial::print!("fis : {:?}\n", fis);
        serial::print!("command header: {:?}\n", cmd_header);
        serial::print!("prdt: {:?}\n", cmd_table.prdt_entries[0]);

        port_ref.ci = 1 << slot;

        let mut i = 0;
        while port_ref.ci & (1 << slot) != 0 {
            i += 1;
        }
        serial::print!("i: {}\n", i);
        if port_ref.interrupt_status & (1 << 30) != 0 {
            serial::print!("error fuck\n");
        }
        serial::print!("bytes read: {}\n", cmd_header.prdbc);
        serial::print!("AHCI access completed\n");
    }
}
