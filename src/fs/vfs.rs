use crate::serial;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::null_mut;

static mut MOUNT_POINTS: Vec<MountPoint> = alloc::vec![];

bitflags::bitflags! {
    pub struct Flags: u32 {
        const O_RDONLY = 0;
        const O_WRONLY = 1;
        const O_RDWR   = 2;
        const O_CREAT  = 100;
        const O_TRUNC  = 1000;
        const O_APPEND = 2000;
    }

    pub struct Mode: u32 {
    }
}

pub struct FileDescription {
    mode: Flags,
    offset: usize,
    file: *mut u8,
}

pub struct MountPoint {
    name: String,
    fs: Option<Box<dyn Filesystem>>,
}

impl MountPoint {
    pub fn new() -> Self {
        MountPoint {
            name: String::new(),
            fs: None,
        }
    }
}

pub trait Filesystem {
    fn open(&self, path: &str, flags: Flags, mode: Mode) -> FileDescription;
    fn mkdir(&self, path: &str, mode: Mode) -> FileDescription;
    fn read(&self, node: FileDescription, buffer: *mut u8, cnt: usize) -> usize;
    fn write(&self, node: FileDescription, buffer: *const u8, cnt: usize) -> usize;
}

pub fn mount(fs: Box<dyn Filesystem>, target: &str) -> bool {
    if target.chars().nth(0) != Some('/') {
        return false;
    }

    for mount_point in unsafe { MOUNT_POINTS.iter() } {
        if mount_point.name == target {
            return false;
        }
    }

    unsafe {
        let mut new_mp = MountPoint::new();
        new_mp.fs = Some(fs);
        new_mp.name = String::from(target);
        MOUNT_POINTS.push(new_mp);
    }

    true
}

pub fn get_mount_point(path: &str) -> Option<(&str, &MountPoint)> {
    None
}
