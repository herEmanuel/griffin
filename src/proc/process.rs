use crate::fs::vfs;
use crate::mm::vmm;
use alloc::vec::Vec;

pub struct Process {
    pid: usize,
    pagemap: vmm::VirtualMemManager,
    file_desc_list: Vec<vfs::FileDescription>,
}

pub struct Thread {}
