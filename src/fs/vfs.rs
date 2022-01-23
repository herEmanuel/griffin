use alloc::{string::String, vec::Vec};

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

    pub struct FileType: u16 {
        const FIFO = 1 << 12;
        const CHAR_DEVICE = 1 << 13;
        const DIRECTORY = 1 << 14;
        const BLOCK_DEVICE = 1 << 14 | 1 << 13;
        const NORMAL = 1 << 15;
        const SYMLINK = 1 << 15 | 1 << 13;
        const SOCKET = 1 << 15 | 1 << 14;
    }

    pub struct FilePermissions: u16 {
        const USER_READ = 1 << 8;
        const USER_WRITE = 1 << 7;
        const USER_EXEC = 1 << 6;
    }
}

pub struct FileDescription {
    flags: Flags,
    offset: usize,
    fs: &'static dyn Filesystem,
    pub file_index: usize, // an index for the filesystem-specific table of open files
}

impl FileDescription {
    pub fn new(index: usize, flags: Flags, fs: &'static dyn Filesystem) -> Self {
        FileDescription {
            flags,
            offset: 0,
            fs,
            file_index: index,
        }
    }
}

pub struct MountPoint {
    name: String,
    fs: Option<&'static dyn Filesystem>,
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
    fn open(&self, path: &str, flags: Flags, mode: Mode) -> Option<FileDescription>;
    fn mkdir(&self, path: &str, mode: Mode) -> Option<FileDescription>;
    fn read(&self, index: usize, buffer: *mut u8, cnt: usize, offset: usize) -> usize;
    fn write(&self, index: usize, buffer: *const u8, cnt: usize, offset: usize) -> usize;
}

pub fn mount(fs: &'static dyn Filesystem, target: &str) -> bool {
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

pub fn get_mount_point(path: &str) -> Option<&MountPoint> {
    let mut curr_mp: Option<&MountPoint> = None;
    for mount_point in unsafe { MOUNT_POINTS.iter() } {
        if path.contains(mount_point.name.as_str()) {
            if let Some(mp) = curr_mp {
                if mount_point.name.len() > mp.name.len() {
                    curr_mp = Some(mount_point);
                }
            } else {
                curr_mp = Some(mount_point);
            }
        }
    }

    curr_mp
}

pub fn open(path: &str, flags: Flags, mode: Mode) -> Option<FileDescription> {
    if path.chars().nth(0) != Some('/') {
        // relative path, not supported atm
        return None;
    }

    if let Some(mount_point) = get_mount_point(path) {
        mount_point
            .fs
            .as_ref()
            .unwrap()
            .open(&path[mount_point.name.len()..], flags, mode)
    } else {
        // TODO: report the error
        None
    }
}

pub fn mkdir(path: &str, mode: Mode) -> Option<FileDescription> {
    if let Some(mount_point) = get_mount_point(path) {
        mount_point
            .fs
            .as_ref()
            .unwrap()
            .mkdir(&path[mount_point.name.len()..], mode)
    } else {
        // TODO: report the error
        None
    }
}

pub fn read(fd: FileDescription, buffer: *mut u8, cnt: usize) -> usize {
    fd.fs.read(fd.file_index, buffer, cnt, fd.offset)
}

pub fn write(fd: FileDescription, buffer: *const u8, cnt: usize) -> usize {
    fd.fs.write(fd.file_index, buffer, cnt, fd.offset)
}
