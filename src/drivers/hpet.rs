use crate::arch::{acpi, mm::pmm};
use crate::mm::vmm::{self, PageFlags};

const MS_IN_FEMTOSECONDS: u64 = 1000000000000;

static mut HPET: Option<&HpetMem> = None;

#[repr(C, packed)]
struct HpetTable {
    header: acpi::Sdt,
    revision_id: u8,
    details: u8,
    pci_id: u16,
    addr_space_id: u8,
    register_width: u8,
    register_offset: u8,
    reserved: u8,
    address: u64,
    hpet_num: u8,
    min_ticks: u16,
    page_prot: u8,
}

#[repr(C, packed)]
struct HpetMem {
    general_capabilities: u64,
    unused0: u64,
    general_config: u64,
    unused1: u64,
    interrupt_status: u64,
    unused2: [u64; 25],
    main_counter_value: u64,
}

pub fn init() {
    let hpet_table = unsafe {
        &mut *(acpi::find_table(*b"HPET").expect("Could not find the HPET table")
            as *const acpi::Sdt as *mut HpetTable)
    };

    vmm::get().map_page(
        vmm::VirtAddr::new(hpet_table.address + pmm::PHYS_BASE),
        pmm::PhysAddr::new(hpet_table.address),
        PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::UNCACHEABLE,
        true,
    );

    let hpet = unsafe { &mut *(hpet_table.address as *mut HpetMem) };
    hpet.general_config = 1;

    unsafe { HPET = Some(hpet) }
}

pub fn sleep(ms: u64) {
    let hpet = unsafe { HPET.expect("The HPET hasn't been initialized") };
    let clock = (hpet.general_capabilities >> 32) as u32;

    let target = { hpet.main_counter_value } + (ms * MS_IN_FEMTOSECONDS) / clock as u64;
    while hpet.main_counter_value < target {
        core::hint::spin_loop();
    }
}
