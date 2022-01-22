use super::cpu;
use super::mm::pmm;
use crate::drivers::hpet;
use crate::mm::vmm::{self, PageFlags};
use crate::serial;

static mut LAPIC: Option<Xapic> = None;

#[repr(u16)]
#[derive(Clone, Copy)]
pub enum LapicRegisters {
    Eoi = 0xb0,
    Sivr = 0xf0,
    Dcr = 0x3e0,
    LvtTimer = 0x320,
    InitialCount = 0x380,
    CurrCount = 0x390,
}

#[derive(Clone, Copy)]
pub struct Xapic {
    address: u64,
}

impl Xapic {
    pub fn new() -> Self {
        Xapic {
            address: cpu::rdmsr(cpu::MsrList::ApicBase) + pmm::PHYS_BASE,
        }
    }

    pub fn enable(&self) {
        self.write(
            LapicRegisters::Sivr,
            self.read(LapicRegisters::Sivr) | 0x1ff,
        );
    }

    pub fn read(&self, reg: LapicRegisters) -> u32 {
        serial::print!("addr: {:#x}\n", self.address + reg as u64);
        unsafe { *((self.address + reg as u64) as *const u32) }
    }

    pub fn write(&self, reg: LapicRegisters, value: u32) {
        unsafe { *((self.address + reg as u64) as *mut u32) = value }
    }

    pub fn calibrate_timer(&self, ms: u64) {
        self.write(LapicRegisters::Dcr, 0); // divide by two
        self.write(LapicRegisters::InitialCount, u32::MAX);

        hpet::sleep(ms);

        let count = u32::MAX - self.read(LapicRegisters::CurrCount);
        self.write(LapicRegisters::LvtTimer, 0x20 | 1 << 17); // periodic mode
        self.write(LapicRegisters::InitialCount, count);
    }

    pub fn eoi(&self) {
        self.write(LapicRegisters::Eoi, 0);
    }
}

pub fn init() {
    let xapic = Xapic::new();

    vmm::get().map_page(
        vmm::VirtAddr::new(xapic.address),
        pmm::PhysAddr::new(xapic.address - pmm::PHYS_BASE),
        PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::UNCACHEABLE,
        true,
    );

    unsafe {
        LAPIC = Some(xapic);
        LAPIC.unwrap().enable();
        let sivr = LAPIC.unwrap().read(LapicRegisters::Sivr);
        serial::print!("address: {:#x}\n", LAPIC.unwrap().address);
        serial::print!("sivr: {:#x}\n", sivr);
    }
}

pub fn get() -> Xapic {
    unsafe { LAPIC.expect("The Lapic hasn't been initialized") }
}
