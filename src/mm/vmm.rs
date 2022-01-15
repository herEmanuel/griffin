use crate::arch::x86_64::mm::pmm::{self, PhysAddr};
use alloc::vec::Vec;

bitflags::bitflags! {
    pub struct PageFlags: u64 {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const USERMODE = 1 << 2;
        const WT = 1 << 3;
        const UNCACHEABLE = 1 << 4;
        const NX = 1 << 63;
    }
}

static mut VIRTUAL_MEMORY_MANAGER: VirtualMemManager = VirtualMemManager::new();
pub const KERNEL_BASE: u64 = 0xffffffff80000000;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    pub fn new(addr: u64) -> Self {
        VirtAddr(addr)
    }

    pub fn pml4(self) -> u16 {
        ((self.0 >> 39) & 0x1ff) as u16
    }

    pub fn pdp(self) -> u16 {
        ((self.0 >> 30) & 0x1ff) as u16
    }

    pub fn pd(self) -> u16 {
        ((self.0 >> 21) & 0x1ff) as u16
    }

    pub fn pt(self) -> u16 {
        ((self.0 >> 12) & 0x1ff) as u16
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

pub struct MemRange {
    base: u64,
    length: usize,
    flags: usize,
}

pub struct VirtualMemManager {
    pagemap: PhysAddr,
    ranges: Vec<MemRange>,
}

impl VirtualMemManager {
    const fn new() -> Self {
        VirtualMemManager {
            pagemap: PhysAddr::new(0),
            ranges: alloc::vec![],
        }
    }

    pub fn switch_pagemap(&self) {
        unsafe {
            asm!("mov cr3, {}", in(reg) self.pagemap.as_u64());
        }
    }

    pub fn invlpg(&self, virtual_addr: VirtAddr) {
        unsafe {
            asm!("invlpg [{}]", in(reg) virtual_addr.as_u64());
        }
    }

    fn get_next_level(&self, curr: PhysAddr, index: isize) -> PhysAddr {
        let level: *mut u64 = curr.higher_half().as_mut_ptr();

        unsafe {
            if *level.offset(index) & 1 == 0 {
                let entry = pmm::get()
                    .calloc(1)
                    .expect("Could not allocate a page needed for get_next_level")
                    .as_u64();

                let flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USERMODE;
                *level.offset(index) = entry | flags.bits();

                return PhysAddr::new(entry);
            }

            PhysAddr::new(*level.offset(index) & 0xfffffffffffff000)
        }
    }

    pub fn map_page(
        &self,
        virtual_addr: VirtAddr,
        phys_addr: PhysAddr,
        flags: PageFlags,
        flush_prev: bool,
    ) {
        if flush_prev {
            self.invlpg(virtual_addr);
        }

        let pml4e = virtual_addr.pml4();
        let pdpe = virtual_addr.pdp();
        let pde = virtual_addr.pd();
        let pte = virtual_addr.pt();

        let pdp = self.get_next_level(self.pagemap, pml4e as isize);
        let pd = self.get_next_level(pdp, pdpe as isize);
        let page_table: *mut u64 = self.get_next_level(pd, pde as isize).as_mut_ptr();

        unsafe {
            *page_table.offset(pte as isize) = phys_addr.as_u64() | flags.bits();
        }
    }

    pub fn get_phys_addr(&self, virtual_addr: VirtAddr) -> PhysAddr {
        let pml4e = virtual_addr.pml4();
        let pdpe = virtual_addr.pdp();
        let pde = virtual_addr.pd();
        let pte = virtual_addr.pt();

        let pdp = self.get_next_level(self.pagemap, pml4e as isize);
        let pd = self.get_next_level(pdp, pdpe as isize);
        let page_table: *mut u64 = self.get_next_level(pd, pde as isize).as_mut_ptr();

        // TODO: remove the flags from the address
        unsafe { PhysAddr::new(*page_table.offset(pte as isize)) }
    }
}

pub fn init() {
    let pml4: u64;

    unsafe {
        asm!("mov {}, cr3", out(reg) pml4);
        VIRTUAL_MEMORY_MANAGER.pagemap = PhysAddr::new(pml4);
    }
}

pub fn get() -> &'static VirtualMemManager {
    unsafe { &VIRTUAL_MEMORY_MANAGER }
}
