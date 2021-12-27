use core::hint::spin_loop;
use core::sync::atomic::{self, Ordering};

pub struct Spinlock<T>(atomic::AtomicBool, T);

impl<T> Spinlock<T> {
    pub const fn new(value: T) -> Self {
        Spinlock(atomic::AtomicBool::new(false), value)
    }

    pub fn lock(&mut self) -> &mut T {
        while let Err(_) =
            self.0
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            spin_loop();
        }

        &mut self.1
    }

    pub fn unlock(&self) {
        self.0.store(false, Ordering::Release);
    }
}

unsafe impl<T> Sync for Spinlock<T> {}
