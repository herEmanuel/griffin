use crate::arch::x86_64::mm::pmm;
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

pub struct MemRange {
    base: u64,
    length: usize,
    flags: usize,
}

pub struct VirtualMemManager {
    pagemap: u64,
    ranges: Vec<MemRange>,
}

impl VirtualMemManager {
    const fn new() -> Self {
        VirtualMemManager {
            pagemap: 0,
            ranges: alloc::vec![],
        }
    }

    pub fn switch_pagemap(&self) {
        unsafe {
            asm!("mov cr3, {}", in(reg) self.pagemap);
        }
    }

    pub fn invlpg(&self, virtual_addr: u64) {
        unsafe {
            asm!("invlpg [{}]", in(reg) virtual_addr);
        }
    }

    fn get_next_level(&self, curr: u64, index: isize) -> u64 {
        let level = (curr + pmm::PHYS_BASE) as *mut u64;

        unsafe {
            if *level.offset(index) & 1 == 0 {
                let entry = pmm::PAGE_ALLOCATOR
                    .calloc(1)
                    .expect("Could not allocate a page needed for get_next_level")
                    as u64;

                let flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USERMODE;
                *level.offset(index) = entry | flags.bits();

                return entry;
            }

            *level.offset(index) & 0xfffffffffffff000
        }
    }

    pub fn map_page(&self, virtual_addr: u64, phys_addr: u64, flags: PageFlags, flush_prev: bool) {
        if flush_prev {
            self.invlpg(virtual_addr);
        }

        let pml4e = (virtual_addr >> 39) & 0x1ff;
        let pdpe = (virtual_addr >> 30) & 0x1ff;
        let pde = (virtual_addr >> 21) & 0x1ff;
        let pte = (virtual_addr >> 12) & 0x1ff;

        let pdp = self.get_next_level(self.pagemap, pml4e as isize);
        let pd = self.get_next_level(pdp, pdpe as isize);
        let page_table = self.get_next_level(pd, pde as isize) as *mut u64;

        unsafe {
            *page_table.offset(pte as isize) = phys_addr | flags.bits();
        }
    }

    pub fn get_phys_addr(&self, virtual_addr: u64) -> u64 {
        let pml4e = (virtual_addr >> 39) & 0x1ff;
        let pdpe = (virtual_addr >> 30) & 0x1ff;
        let pde = (virtual_addr >> 21) & 0x1ff;
        let pte = (virtual_addr >> 12) & 0x1ff;

        let pdp = self.get_next_level(self.pagemap, pml4e as isize);
        let pd = self.get_next_level(pdp, pdpe as isize);
        let page_table = self.get_next_level(pd, pde as isize) as *mut u64;

        unsafe { *page_table.offset(pte as isize) }
    }
}

pub fn init() {
    let pml4: u64;

    unsafe {
        asm!("mov {}, cr3", out(reg) pml4);
        VIRTUAL_MEMORY_MANAGER.pagemap = pml4;
    }
}

pub fn get() -> &'static VirtualMemManager {
    unsafe { &VIRTUAL_MEMORY_MANAGER }
}
