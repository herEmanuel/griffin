use crate::serial;
use crate::spinlock::Spinlock;
use core::ptr::null_mut;
use stivale_boot::v2::{StivaleMemoryMapEntry, StivaleMemoryMapEntryType};

//TODO: eventually switch to a buddy allocator?

pub const PAGE_SIZE: u64 = 4096;
pub const PHYS_BASE: u64 = 0xffff800000000000;

pub static mut PAGE_ALLOCATOR: Pmm = Pmm::new();

pub struct Pmm {
    bitmap: Spinlock<*mut u8>,
    bitmap_size: u64,
}

impl Pmm {
    const fn new() -> Self {
        Pmm {
            bitmap: Spinlock::new(null_mut()),
            bitmap_size: 0,
        }
    }

    pub unsafe fn alloc(&mut self, pages: usize) -> Option<*mut u8> {
        let bitmap = self.bitmap.lock();
        let mut count = 0;

        for i in 0..self.bitmap_size * 8 {
            if *bitmap.offset((i / 8) as isize) & (1 << (7 - i % 8)) != 0 {
                count += 1;

                if count == pages {
                    let page = i - pages as u64 + 1;

                    for p in page..page + pages as u64 {
                        *bitmap.offset((p / 8) as isize) &= !(1 << (7 - p % 8));
                    }

                    self.bitmap.unlock();
                    return Some((page * PAGE_SIZE) as *mut u8);
                }

                continue;
            }

            count = 0;
        }

        self.bitmap.unlock();
        None
    }

    pub unsafe fn calloc(&mut self, pages: usize) -> Option<*mut u8> {
        if let Some(mem) = self.alloc(pages) {
            mem.write_bytes(0, pages * PAGE_SIZE as usize);
            Some(mem)
        } else {
            None
        }
    }
}

pub unsafe fn init(entries: *const StivaleMemoryMapEntry, entries_num: u64) {
    serial::print!("init\n");
    let mut biggest = 0;

    for i in 0..entries_num {
        let entry = &*(entries.offset(i as isize));

        match entry.entry_type {
            StivaleMemoryMapEntryType::BootloaderReclaimable
            | StivaleMemoryMapEntryType::Usable
            | StivaleMemoryMapEntryType::Kernel => {}
            _ => {
                continue;
            }
        }

        let peak = entry.base + entry.length;
        if peak > biggest {
            biggest = peak;
        }
    }
    serial::print!("got biggest\n");
    PAGE_ALLOCATOR.bitmap_size = (biggest / PAGE_SIZE) / 8; // wasting some pages here

    for i in 0..entries_num {
        serial::print!("loop\n");
        let entry = &mut *(entries.offset(i as isize) as *mut StivaleMemoryMapEntry);

        match entry.entry_type {
            StivaleMemoryMapEntryType::BootloaderReclaimable
            | StivaleMemoryMapEntryType::Usable => {}
            _ => {
                continue;
            }
        }

        if entry.length < PAGE_ALLOCATOR.bitmap_size {
            continue;
        }

        let bitmap = PAGE_ALLOCATOR.bitmap.lock();

        *bitmap = (entry.base + PHYS_BASE) as *mut u8;
        bitmap.write_bytes(0, PAGE_ALLOCATOR.bitmap_size as usize);

        PAGE_ALLOCATOR.bitmap.unlock();

        entry.base += PAGE_ALLOCATOR.bitmap_size;
        entry.length -= PAGE_ALLOCATOR.bitmap_size;
        break;
    }
    serial::print!("allocated bitmap memory\n");
    for i in 0..entries_num {
        let entry = &*(entries.offset(i as isize));

        match entry.entry_type {
            StivaleMemoryMapEntryType::BootloaderReclaimable
            | StivaleMemoryMapEntryType::Usable => {}
            _ => {
                continue;
            }
        }

        let page = entry.base / PAGE_SIZE;
        let length = entry.length / PAGE_SIZE;
        let bitmap = PAGE_ALLOCATOR.bitmap.lock();

        for p in page..page + length {
            *bitmap.offset((p / 8) as isize) |= 1 << (7 - p % 8);
        }

        PAGE_ALLOCATOR.bitmap.unlock();
    }
    serial::print!("initialized the bitmap\n");
}
