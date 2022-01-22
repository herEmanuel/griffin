/*
    A *very* simple slab allocator

    still needs some testing
*/

use crate::arch::mm::pmm;
use crate::serial;
use crate::spinlock::Spinlock;
use crate::utils::{bitmap, math};
use core::alloc::GlobalAlloc;
use core::mem::size_of;
use core::ptr::null_mut;

const OBJS_PER_SLAB: usize = 256;

#[global_allocator]
pub static mut SLAB_ALLOCATOR: SlabAllocator = SlabAllocator { caches: null_mut() };

struct Cache<'a> {
    name: &'a str,
    object_size: usize,
    pages_per_slab: usize,
    slab_count: usize,
    slabs: *mut Slab,
    next: *mut Cache<'a>,
}

impl<'a> Cache<'a> {
    unsafe fn new(name: &str, obj_size: usize) -> *mut Cache {
        let mut chache_ptr: *mut Cache = pmm::get()
            .calloc(1)
            .expect("Could not allocate pages for the cache")
            .higher_half()
            .as_mut_ptr();

        let cache = &mut *chache_ptr;
        cache.name = name;
        cache.object_size = obj_size;
        cache.pages_per_slab = math::div_ceil(
            OBJS_PER_SLAB * obj_size + size_of::<Slab>(),
            pmm::PAGE_SIZE as usize,
        );
        cache.slab_count = 0;
        cache.slabs = null_mut(); // last slab guaranted to be null
        cache.slabs = Slab::new(cache);
        cache.next = null_mut();

        chache_ptr
    }

    unsafe fn alloc_obj(&mut self) -> *mut u8 {
        let mut curr_slab = &mut *self.slabs;

        while curr_slab.free_objs == 0 {
            curr_slab = &mut *curr_slab.next;
        }

        //TODO: limit the number of new slabs?
        //TODO: lock this?
        if curr_slab.free_objs == 0 {
            let new_slab = Slab::new(self);
            (*new_slab).next = self.slabs;
            self.slabs = new_slab;
            curr_slab = &mut *new_slab;
        }

        curr_slab.alloc()
    }

    unsafe fn free_obj(&mut self, ptr: *mut u8) {
        // we may want to free the slabs that are not being used... but not now
        let mut curr_slab = &mut *self.slabs;

        let mut found = false;
        for _ in 0..self.slab_count {
            if ptr as usize >= curr_slab.data as usize
                && (ptr as usize)
                    < (curr_slab.data as usize) + self.pages_per_slab * pmm::PAGE_SIZE as usize
            {
                found = true;
                break;
            }

            curr_slab = &mut *curr_slab.next;
        }

        if !found {
            panic!("Tried do deallocate memory not allocated by the heap");
        }

        curr_slab.dealloc(ptr);
    }
}

struct Slab {
    free_objs: usize,
    object_size: usize,
    data: *mut u8,
    bitmap: Spinlock<bitmap::Bitmap<{ OBJS_PER_SLAB / 8 }>>,
    next: *mut Slab,
    previous: *mut Slab,
}

impl Slab {
    unsafe fn new(parent: &mut Cache) -> *mut Slab {
        let mut slab_ptr: *mut Slab = pmm::get()
            .calloc(parent.pages_per_slab)
            .expect("Could not allocate pages for the new slab")
            .higher_half()
            .as_mut_ptr();

        let slab = &mut *slab_ptr;

        slab.free_objs = OBJS_PER_SLAB;
        slab.object_size = parent.object_size;
        slab.next = parent.slabs;
        slab.previous = null_mut();
        parent.slabs = slab;
        parent.slab_count += 1;
        // this should be ok... right?
        slab.data = slab_ptr.offset(1) as *mut u8;

        slab_ptr
    }

    unsafe fn alloc(&mut self) -> *mut u8 {
        if self.free_objs == 0 {
            return null_mut();
        }

        let bitmap = self.bitmap.lock();

        for i in 0..OBJS_PER_SLAB {
            if bitmap.bit_at(i) == 0 {
                bitmap.set_bit(i);
                self.free_objs -= 1;

                self.bitmap.unlock();
                return self.data.offset((i * self.object_size) as isize);
            }
        }

        self.bitmap.unlock();
        null_mut() // should never get here
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        let bit = (ptr as usize - self.data as usize) / self.object_size;
        let bitmap = self.bitmap.lock();

        self.free_objs += 1;
        bitmap.clear_bit(bit);
        self.bitmap.unlock();
    }
}

pub struct SlabAllocator<'a> {
    caches: *mut Cache<'a>,
}

impl<'a> SlabAllocator<'a> {
    unsafe fn add_cache(&mut self, name: &'a str, obj_size: usize) {
        if self.caches.is_null() {
            self.caches = Cache::new(name, obj_size);
            return;
        }

        let new_cache = Cache::new(name, obj_size);
        (*new_cache).next = self.caches;
        self.caches = new_cache;
    }

    unsafe fn cache_for(&self, size: usize) -> Option<*mut Cache<'a>> {
        let mut curr_cache = self.caches;

        while !curr_cache.is_null() && (*curr_cache).object_size < size {
            curr_cache = (*curr_cache).next;
        }

        if curr_cache.is_null() || (*curr_cache).object_size < size {
            return None;
        }

        Some(curr_cache)
    }

    pub unsafe fn dump(&self) {
        let mut curr_cache = self.caches;

        while !curr_cache.is_null() {
            serial::print!(
                "[SLAB DUMP] Found a cache, object size of {}, slab count of {}\n",
                (*curr_cache).object_size,
                (*curr_cache).slab_count
            );
            curr_cache = (*curr_cache).next;
        }
    }
}

pub unsafe fn init() {
    SLAB_ALLOCATOR.add_cache("4096 bytes", 4096);
    SLAB_ALLOCATOR.add_cache("2048 bytes", 2048);
    SLAB_ALLOCATOR.add_cache("1024 bytes", 1024);
    SLAB_ALLOCATOR.add_cache("512 bytes", 512);
    SLAB_ALLOCATOR.add_cache("256 bytes", 256);
    SLAB_ALLOCATOR.add_cache("128 bytes", 128);
    SLAB_ALLOCATOR.add_cache("64 bytes", 64);
    SLAB_ALLOCATOR.add_cache("32 bytes", 32);
    SLAB_ALLOCATOR.add_cache("16 bytes", 16);
    SLAB_ALLOCATOR.add_cache("8 bytes", 8);
}

unsafe impl<'a> GlobalAlloc for SlabAllocator<'a> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        if let Some(cache) = SLAB_ALLOCATOR.cache_for(layout.size()) {
            (*cache).alloc_obj()
        } else {
            panic!("Could not find a cache large enough to suffice the heap allocation");
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        if let Some(cache) = SLAB_ALLOCATOR.cache_for(layout.size()) {
            (*cache).free_obj(ptr)
        } else {
            panic!("Tried do deallocate memory not allocated by the heap");
        }
    }
}
