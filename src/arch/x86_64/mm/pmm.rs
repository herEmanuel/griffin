use crate::serial;
use crate::utils::{bitmap, math::div_ceil};
use core::ops::{Deref, DerefMut};
use core::ptr::null_mut;
use stivale_boot::v2::{StivaleMemoryMapEntry, StivaleMemoryMapEntryType};

//TODO: eventually switch to a buddy allocator?

pub const PAGE_SIZE: u64 = 4096;
pub const PHYS_BASE: u64 = 0xffff800000000000;

pub static mut PAGE_ALLOCATOR: Option<Pmm> = None;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    pub const fn new(addr: u64) -> Self {
        PhysAddr(addr)
    }

    pub fn higher_half(self) -> Self {
        PhysAddr(self.0 | PHYS_BASE)
    }

    pub fn lower_half(self) -> Self {
        PhysAddr(self.0 & !PHYS_BASE)
    }

    pub fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    pub fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    // remove the page table bits that give information about the mapping
    pub fn remove_flags(self) -> Self {
        PhysAddr(self.0 & 0x000ffffffffff000)
    }
}

pub struct PmmBox<T> {
    data: *mut T,
    page_cnt: usize,
}

impl<T> PmmBox<T> {
    pub fn new(size: usize) -> Self {
        let alloc_size = div_ceil(size, PAGE_SIZE as usize);
        let mem: *mut T = get()
            .calloc(alloc_size)
            .expect("PmmBox: could not allocate the pages needed")
            .higher_half()
            .as_mut_ptr();

        PmmBox {
            data: mem,
            page_cnt: alloc_size,
        }
    }

    pub fn as_ptr(&self) -> *const T {
        self.data
    }

    pub fn as_mut_ptr(&self) -> *mut T {
        self.data
    }
}

impl<T> Deref for PmmBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.data }
    }
}

impl<T> DerefMut for PmmBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.data }
    }
}

impl<T> Drop for PmmBox<T> {
    fn drop(&mut self) {
        get().free(self.data as *mut u8, self.page_cnt);
    }
}

pub struct Pmm(spin::Mutex<bitmap::Bitmap>);

impl Pmm {
    fn new(bitmap: bitmap::Bitmap) -> Self {
        Pmm(spin::Mutex::new(bitmap))
    }

    pub fn alloc(&mut self, pages: usize) -> Option<PhysAddr> {
        let mut bitmap = self.0.lock();
        let mut count = 0;

        for i in 0..bitmap.size() * 8 {
            if bitmap.is_set(i) {
                count += 1;

                if count == pages {
                    let page = i - pages + 1;

                    for p in page..page + pages {
                        bitmap.clear(p);
                    }
                    serial::print!("address: {:#x}\n", page as u64 * PAGE_SIZE);
                    return Some(PhysAddr::new(page as u64 * PAGE_SIZE));
                }

                continue;
            }

            count = 0;
        }

        None
    }

    pub fn calloc(&mut self, pages: usize) -> Option<PhysAddr> {
        if let Some(mem) = self.alloc(pages) {
            unsafe {
                mem.as_mut_ptr::<u8>()
                    .write_bytes(0, pages * PAGE_SIZE as usize);
            }
            Some(mem)
        } else {
            None
        }
    }

    pub fn free(&mut self, ptr: *mut u8, pages_amnt: usize) {
        let page = (ptr as u64 & !PHYS_BASE) / PAGE_SIZE;
        let mut bitmap = self.0.lock();

        for i in page..(page + pages_amnt as u64) {
            bitmap.set(i as usize);
        }
    }
}

pub unsafe fn init(entries: *const StivaleMemoryMapEntry, entries_num: u64) {
    let mut biggest = 0;
    let mut bitmap_ptr = null_mut();
    let mut bitmap;

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

    let bitmap_size = div_ceil((biggest / PAGE_SIZE) as usize, 8) as u64;

    for i in 0..entries_num {
        let entry = &mut *(entries.offset(i as isize) as *mut StivaleMemoryMapEntry);

        match entry.entry_type {
            StivaleMemoryMapEntryType::BootloaderReclaimable
            | StivaleMemoryMapEntryType::Usable => {}
            _ => {
                continue;
            }
        }

        if entry.length < bitmap_size {
            continue;
        }

        bitmap_ptr = (entry.base + PHYS_BASE) as *mut u8;
        bitmap_ptr.write_bytes(0, bitmap_size as usize);

        entry.base += bitmap_size;
        entry.length -= bitmap_size;
        break;
    }

    if bitmap_ptr.is_null() {
        panic!("[PMM] Could not allocate the memory needed for the bitmap");
    }

    bitmap = bitmap::Bitmap::from_raw_ptr(bitmap_ptr, bitmap_size as usize);

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

        for p in page..page + length {
            bitmap.set(p as usize);
        }
    }

    PAGE_ALLOCATOR = Some(Pmm::new(bitmap));
}

pub fn get() -> &'static mut Pmm {
    unsafe {
        PAGE_ALLOCATOR
            .as_mut()
            .expect("The Pmm hasn't been initialized")
    }
}
