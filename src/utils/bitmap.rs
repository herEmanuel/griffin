use super::math::div_ceil;
use crate::arch::mm::pmm;

pub struct Bitmap(&'static mut [u8], usize);

impl Bitmap {
    pub fn new(size: usize) -> Self {
        let data: *mut u8 = pmm::get()
            .calloc(div_ceil(size, pmm::PAGE_SIZE as usize))
            .expect("Could not allocate the pages for the bitmap")
            .higher_half()
            .as_mut_ptr();

        let slice = unsafe { core::slice::from_raw_parts_mut(data, size) };

        Bitmap(slice, size)
    }

    pub fn from_raw_ptr(ptr: *mut u8, len: usize) -> Self {
        let slice = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        Bitmap(slice, len)
    }

    pub fn size(&self) -> usize {
        self.1
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }

    pub fn set(&mut self, bit: usize) {
        self.0[bit / 8] |= 1 << (bit % 8);
    }

    pub fn toggle(&mut self, bit: usize) {
        self.0[bit / 8] ^= 1 << (bit % 8);
    }

    pub fn clear(&mut self, bit: usize) {
        self.0[bit / 8] &= !(1 << (bit % 8));
    }

    pub fn is_set(&self, bit: usize) -> bool {
        self.0[bit / 8] & (1 << (bit % 8)) != 0
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        pmm::get().free(
            self.0.as_mut_ptr(),
            div_ceil(self.1, pmm::PAGE_SIZE as usize),
        );
    }
}
