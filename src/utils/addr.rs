use crate::arch::x86_64::mm::pmm;

pub fn higher_half<T>(ptr: *mut T) -> *mut T {
    (ptr as u64 | pmm::PHYS_BASE) as *mut T
}
