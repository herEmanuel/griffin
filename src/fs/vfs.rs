bitflags::bitflags! {
    pub struct OpenFlags: u32 {
        const O_RDONLY = 0;
        const O_WRONLY = 1;
        const O_RDWR   = 2;
        const O_CREAT  = 100;
        const O_TRUNC  = 1000;
        const O_APPEND = 2000;
    }
}
