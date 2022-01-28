use core::ops::RangeBounds;

use crate::arch::mm::pmm::{self, PhysAddr};
use crate::arch::{cpu, interrupts};
use crate::proc::scheduler;
use crate::utils::math::div_ceil;
use crate::{serial, vfs};
use alloc::vec::Vec;

static mut VIRTUAL_MEMORY_MANAGER: Option<VirtualMemManager> = None;
pub const KERNEL_BASE: u64 = 0xffffffff80000000;

bitflags::bitflags! {
    pub struct PageFlags: u64 {
        const PRESENT     = 1 << 0;
        const WRITABLE    = 1 << 1;
        const USERMODE    = 1 << 2;
        const WT          = 1 << 3;
        const UNCACHEABLE = 1 << 4;

        // bits that are ignored by the cpu but used by griffin's vmm
        const MMAPED = 1 << 9;
        // ==========================

        const NX          = 1 << 63;
    }

    pub struct MapProt: u64 {
        const NONE  = 0x0;
        const READ  = 0x1;
        const WRITE = 0x2;
        const EXEC  = 0x4;
    }

    pub struct MapFlags: u64 {
        const SHARED    = 0x0001;
        const PRIVATE   = 0x0002;
        const FIXED     = 0x0010;
        const ANONYMOUS = 0x1000;
    }
}

impl From<MapProt> for PageFlags {
    fn from(prot: MapProt) -> Self {
        let mut page_flags = Self::NX;

        if prot.contains(MapProt::NONE) {
            return page_flags;
        }

        if prot.contains(MapProt::WRITE) {
            page_flags |= Self::WRITABLE;
        }

        if prot.contains(MapProt::READ) {
            page_flags |= Self::USERMODE;
        }

        if prot.contains(MapProt::EXEC) {
            page_flags.remove(Self::NX);
        }

        page_flags
    }
}

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

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PageMapping(u64);

impl PageMapping {
    pub fn new(addr: u64) -> Self {
        PageMapping(addr)
    }

    pub fn phys_addr(&self) -> PhysAddr {
        PhysAddr::new(self.0).remove_flags()
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn is_present(&self) -> bool {
        self.0 & PageFlags::PRESENT.bits() != 0
    }

    pub fn is_writable(&self) -> bool {
        self.0 & PageFlags::WRITABLE.bits() != 0
    }

    pub fn is_usermode(&self) -> bool {
        self.0 & PageFlags::USERMODE.bits() != 0
    }

    pub fn is_uncacheable(&self) -> bool {
        self.0 & PageFlags::UNCACHEABLE.bits() != 0
    }

    pub fn is_mmaped(&self) -> bool {
        self.0 & PageFlags::MMAPED.bits() != 0
    }

    pub fn is_non_exec(&self) -> bool {
        self.0 & PageFlags::NX.bits() != 0
    }
}

pub struct VirtMemoryRange {
    base: VirtAddr,
    length: usize,
    prot: MapProt,
    flags: MapFlags,
    offset: usize,
    fd: Option<vfs::FileDescription>,
}

impl VirtMemoryRange {
    pub fn new(
        base: VirtAddr,
        length: usize,
        prot: MapProt,
        flags: MapFlags,
        offset: usize,
        fd: Option<vfs::FileDescription>,
    ) -> Self {
        VirtMemoryRange {
            base,
            length,
            prot,
            flags,
            offset,
            fd,
        }
    }

    pub fn start(&self) -> u64 {
        self.base.as_u64()
    }

    pub fn end(&self) -> u64 {
        self.base.as_u64() + self.length as u64
    }

    pub fn is_anon_map(&self) -> bool {
        self.flags.contains(MapFlags::ANONYMOUS)
    }

    pub fn is_private_map(&self) -> bool {
        self.flags.contains(MapFlags::PRIVATE)
    }

    pub fn is_shared_map(&self) -> bool {
        self.flags.contains(MapFlags::SHARED)
    }
}

pub struct VirtualMemManager {
    pagemap: PhysAddr,
    ranges: Vec<VirtMemoryRange>,
}

impl VirtualMemManager {
    pub fn new(usermode: bool) -> Self {
        if !usermode {
            return VirtualMemManager {
                pagemap: PhysAddr::new(0),
                ranges: alloc::vec![],
            };
        }

        let pml4 = pmm::get().calloc(1).expect("Could not allocate a new pml4");
        let pml4_ptr: *mut u64 = pml4.higher_half().as_mut_ptr();

        unsafe {
            let kernel_vmm_ptr = get().pagemap.as_mut_ptr::<u64>();
            *pml4_ptr.offset(256) = *kernel_vmm_ptr.offset(256);
            *pml4_ptr.offset(511) = *kernel_vmm_ptr.offset(511);
        }

        VirtualMemManager {
            pagemap: pml4,
            ranges: alloc::vec![],
        }
    }

    pub fn mmap(
        &mut self,
        address: Option<VirtAddr>,
        length: u64,
        prot: MapProt,
        flags: MapFlags,
        fd: Option<vfs::FileDescription>,
        offset: usize,
    ) {
        if address.is_none() && flags.contains(MapFlags::FIXED) {
            return; // TODO: hard error
        }

        let mut range_address: VirtAddr;

        if let Some(address_value) = address {
            let new_range_start = address_value.as_u64();
            let new_range_end = address_value.as_u64() + length;

            range_address = address_value;

            if !flags.contains(MapFlags::FIXED) {
                for entry in self.ranges.iter() {
                    if (new_range_start > entry.start() && new_range_start < entry.end())
                        || (new_range_end > entry.start() && new_range_end < entry.end())
                    {
                        range_address = self.get_free_range(length as usize);
                    }
                }
            }
        } else {
            range_address = self.get_free_range(length as usize);
        }

        let new_range_start = range_address.as_u64();
        let new_range_end = range_address.as_u64() + length;

        for page in (new_range_start..new_range_end).step_by(pmm::PAGE_SIZE as usize) {
            // TODO: do i really need to add all the prot flags here? the answer is prob no
            self.map_page(
                VirtAddr::new(page),
                PhysAddr::new(0),
                PageFlags::from(prot) | PageFlags::MMAPED,
                true,
            );
        }

        let new_entry =
            VirtMemoryRange::new(range_address, length as usize, prot, flags, offset, fd);
        self.ranges.push(new_entry);
    }

    pub fn get_range(&self, address: VirtAddr) -> Option<&VirtMemoryRange> {
        for entry in self.ranges.iter() {
            if address.as_u64() > entry.start() && address.as_u64() < entry.end() {
                return Some(entry);
            }
        }

        None
    }

    pub fn get_free_range(&self, length: usize) -> VirtAddr {
        todo!()
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

            PhysAddr::new(*level.offset(index)).remove_flags()
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

    pub fn get_mapping(&self, virtual_addr: VirtAddr) -> PageMapping {
        let pml4e = virtual_addr.pml4();
        let pdpe = virtual_addr.pdp();
        let pde = virtual_addr.pd();
        let pte = virtual_addr.pt();

        let pdp = self.get_next_level(self.pagemap, pml4e as isize);
        let pd = self.get_next_level(pdp, pdpe as isize);
        let page_table: *mut u64 = self.get_next_level(pd, pde as isize).as_mut_ptr();

        unsafe { PageMapping::new(*page_table.offset(pte as isize)) }
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
}

pub fn init() {
    let pml4: u64;

    unsafe {
        asm!("mov {}, cr3", out(reg) pml4);
        let mut kernel_vmm = VirtualMemManager::new(false);
        kernel_vmm.pagemap = PhysAddr::new(pml4);

        VIRTUAL_MEMORY_MANAGER = Some(kernel_vmm);
        interrupts::register_isr(0xe, page_fault as u64, 0, 0x8e);
    }
}

pub fn get() -> &'static mut VirtualMemManager {
    unsafe {
        VIRTUAL_MEMORY_MANAGER
            .as_mut()
            .expect("The VMM hasn't been initialized")
    }
}

// NOTE: SMAP is enabled, so all of this wont work rn
// TODO: handle MAP_SHARED
interrupts::isr_err!(page_fault, |_stack, error_code| {
    let mut cr2: u64;
    asm!("mov {}, cr2", out(reg) cr2);

    let virt_cr2 = VirtAddr::new(cr2);

    let curr_thread = scheduler::get()
        .running_thread
        .as_ref()
        .expect("Page fault: no running thread")
        .borrow();

    let curr_process = curr_thread.parent.borrow();

    let vmm = &curr_process.pagemap;
    let mapping = vmm.get_mapping(virt_cr2);

    if mapping.is_mmaped() {
        // demand paging
        interrupts::enable();

        let range = vmm
            .get_range(virt_cr2)
            .expect("Page is marked as mmaped but doesn't belong to any range");

        if range.is_anon_map() {
            let page = pmm::get()
                .calloc(1)
                .expect("Could not allocate new page for anonymous map");

            vmm.map_page(
                virt_cr2,
                page,
                PageFlags::from(range.prot) | PageFlags::PRESENT,
                true,
            );
            return;
        }

        // TODO: test this
        if range.is_private_map() {
            let page = pmm::get()
                .calloc(1)
                .expect("Could not allocate new page for private map")
                .higher_half();

            let this_page_number = cr2 / pmm::PAGE_SIZE - range.start() / pmm::PAGE_SIZE;
            // TODO: add range offset to the calculation
            let offset = this_page_number * pmm::PAGE_SIZE;
            let cnt = if (this_page_number + 1) * pmm::PAGE_SIZE <= range.length as u64 {
                pmm::PAGE_SIZE
            } else {
                range.length as u64 % pmm::PAGE_SIZE
            };

            let fd = range
                .fd
                .as_ref()
                .expect("Private mapping not backed by a file");

            vfs::read(
                fd.fs,
                fd.file_index,
                page.as_mut_ptr::<u8>(),
                cnt as usize,
                offset as usize + range.offset,
            );

            vmm.map_page(
                virt_cr2,
                page.lower_half(),
                PageFlags::from(range.prot),
                true,
            );
            return;
        }

        serial::print!("Page fault says: crap\n");
        return;
    }

    serial::print!("Page fault\n");
    serial::print!("Error code: {}\n", error_code);
    serial::print!("CR2: {:#x}\n", cr2);

    cpu::halt();
});
