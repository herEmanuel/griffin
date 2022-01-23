use super::vfs;
use crate::arch::mm::pmm::PmmBox;
use crate::utils::math::{div_ceil, round_up};
use crate::{drivers::ahci, serial, utils::bitmap};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::intrinsics::size_of;
use core::ops::Deref;

const EXT2_SIGNATURE: u16 = 0xef53;
const ROOT_DIR_INODE: u32 = 0x2;
const MAX_OPEN_FILE_CNT: usize = 1024;
const INODE_TABLE_INIT: Option<Box<Inode>> = None;

static mut EXT2_FS: Option<Arc<Ext2Filesystem>> = None;
static mut INODE_TABLE: [Option<Box<Inode>>; MAX_OPEN_FILE_CNT] =
    [INODE_TABLE_INIT; MAX_OPEN_FILE_CNT];

#[repr(C, packed)]
pub struct Superblock {
    inode_cnt: u32,
    block_cnt: u32,
    reserved_blocks_cnt: u32,
    unallocated_blocks: u32,
    unallocated_inodes: u32,
    superblock_block: u32,
    block_size: u32,
    fragment_size: u32,
    blocks_per_group: u32,
    fragments_per_group: u32,
    inodes_per_group: u32,
    last_mt: u32,
    last_wt: u32,
    mount_cnt: u16,
    mounts_bfc: u16,
    signature: u16,
    fs_state: u16,
    handle_error: u16,
    min_version: u16,
    last_cc: u32,
    cc_interval: u32,
    os_id: u32,
    maj_version: u32,
    user_id: u16,
    group_id: u16,
}

impl Superblock {
    pub fn flush(&self) {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let starting_lba = fs.starting_lba;

        ahci::write(
            0,
            (starting_lba as u64 + 2) * 512,
            size_of::<Superblock>(),
            self as *const Superblock as *const u8,
        )
        .unwrap();
    }
}

#[repr(C, packed)]
#[derive(Debug)]
struct BlockGroupDescriptor {
    block_bitmap: u32,
    inode_bitmap: u32,
    inode_table: u32,
    unallocated_blocks: u16,
    unallocated_inodes: u16,
    directories_cnt: u16,
    unused: [u8; 14],
}

#[repr(C)]
struct BlockGroup {
    raw: BlockGroupDescriptor,
    index: usize,
}

impl BlockGroup {
    pub fn get(block_group_index: usize) -> Box<BlockGroup> {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let starting_lba = fs.starting_lba;
        let block_size = fs.block_size;

        let bgdt_block = if block_size > 1024 { 1 } else { 2 };
        let block_group = unsafe {
            alloc::alloc::alloc(alloc::alloc::Layout::new::<BlockGroup>()) as *mut BlockGroup
        };

        ahci::read(
            0,
            (starting_lba * 512
                + bgdt_block * block_size
                + block_group_index * size_of::<BlockGroupDescriptor>()) as u64,
            size_of::<BlockGroupDescriptor>(),
            block_group as *mut u8,
        )
        .unwrap();

        let mut block_group = unsafe { Box::from_raw(block_group) };
        block_group.index = block_group_index;
        block_group
    }

    // writes all the changes made to this block group descriptor back to the disk
    pub fn flush(&self) {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let starting_lba = fs.starting_lba;
        let block_size = fs.block_size;

        let bgdt_block = if block_size > 1024 { 1 } else { 2 };

        ahci::write(
            0,
            (starting_lba * 512
                + bgdt_block * block_size
                + self.index * size_of::<BlockGroupDescriptor>()) as u64,
            size_of::<BlockGroupDescriptor>(),
            self as *const BlockGroup as *const u8,
        )
        .unwrap();
    }

    pub fn get_inode(&self, inode_addr: u32) -> Box<Inode> {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let starting_lba = fs.starting_lba;
        let block_size = fs.block_size;

        let inode_index = Inode::get_table_index(inode_addr as usize);

        let inode =
            unsafe { alloc::alloc::alloc(alloc::alloc::Layout::new::<Inode>()) as *mut Inode };

        ahci::read(
            0,
            (starting_lba * 512
                + self.raw.inode_table as usize * block_size
                + inode_index * size_of::<Inode>()) as u64,
            size_of::<Inode>(),
            inode as *mut u8,
        )
        .unwrap();

        let mut inode = unsafe { Box::from_raw(inode) };
        // might already be set to the inode addr, but just in case
        inode.inode_number = inode_addr;
        inode
    }

    pub fn alloc_block(&mut self, block_cnt: usize) -> Option<Vec<u32>> {
        if (self.raw.unallocated_blocks as usize) < block_cnt {
            return None;
        }

        let fs = unsafe { EXT2_FS.clone().unwrap() };

        let block_bitmap = PmmBox::<u8>::new(fs.block_size).as_mut_ptr();

        ahci::read(
            0,
            (fs.starting_lba * 512 + self.raw.block_bitmap as usize * fs.block_size) as u64,
            fs.block_size,
            block_bitmap,
        )
        .unwrap();

        let mut allocated = 0;
        let mut blocks = Vec::new();
        for i in 0..fs.block_size * 8 {
            unsafe {
                if bitmap::bit_at(block_bitmap, i) == 0 {
                    bitmap::set_bit(block_bitmap, i);
                    blocks.push(i as u32 + self.index as u32 * fs.superblock.blocks_per_group);
                    allocated += 1;

                    self.raw.unallocated_blocks -= 1;

                    if allocated == block_cnt {
                        break;
                    }
                }
            }
        }

        if allocated != block_cnt {
            return None;
        }

        ahci::write(
            0,
            (fs.starting_lba * 512 + self.raw.block_bitmap as usize * fs.block_size) as u64,
            fs.block_size,
            block_bitmap,
        )
        .unwrap();

        self.flush();

        Some(blocks)
    }

    pub fn alloc_inode(&mut self) -> Option<u32> {
        if self.raw.unallocated_inodes == 0 {
            return None;
        }

        let fs = unsafe { EXT2_FS.clone().unwrap() };

        let inode_bitmap = PmmBox::<u8>::new(fs.block_size).as_mut_ptr();

        ahci::read(
            0,
            (fs.starting_lba * 512 + self.raw.inode_bitmap as usize * fs.block_size) as u64,
            fs.block_size,
            inode_bitmap,
        )
        .unwrap();

        for i in 0..fs.block_size * 8 {
            unsafe {
                if bitmap::bit_at(inode_bitmap, i) == 0 {
                    bitmap::set_bit(inode_bitmap, i);
                    self.raw.unallocated_inodes -= 1;

                    ahci::write(
                        0,
                        (fs.starting_lba * 512 + self.raw.inode_bitmap as usize * fs.block_size)
                            as u64,
                        fs.block_size,
                        inode_bitmap,
                    )
                    .unwrap();

                    self.flush();

                    return Some(
                        (i + 1 + self.index * fs.superblock.inodes_per_group as usize) as u32,
                    );
                }
            }
        }

        None
    }
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct Inode {
    type_and_permissions: u16,
    user_id: u16,
    sizel: u32,
    last_access_time: u32,
    creation_time: u32,
    last_mod_time: u32,
    deletion_time: u32,
    group_id: u16,
    ref_cnt: u16,
    sectors_used: u32,
    flags: u32,
    inode_number: u32, // os specific
    direct_pointer: [u32; 12],
    singly_ip: u32,
    doubly_ip: u32,
    triply_ip: u32,
    gen_num: u32,
    ext_ab: u32,
    sizeh_dir_acl: u32,
    fragment_block: u32,
    os_specific2: [u32; 3],
}

impl Inode {
    pub fn get_block_group(inode: usize) -> usize {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        (inode - 1) / fs.superblock.inodes_per_group as usize
    }

    pub fn get_table_index(inode: usize) -> usize {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        (inode - 1) % fs.superblock.inodes_per_group as usize
    }

    pub fn is_directory(&self) -> bool {
        self.type_and_permissions & vfs::FileType::DIRECTORY.bits() != 0
    }

    pub fn is_regular_file(&self) -> bool {
        self.type_and_permissions & vfs::FileType::NORMAL.bits() != 0
    }

    pub fn is_symlink(&self) -> bool {
        self.type_and_permissions & vfs::FileType::SYMLINK.bits() != 0
    }

    pub fn flush(&self) {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let starting_lba = fs.starting_lba;
        let block_size = fs.block_size;

        let inode_table = BlockGroup::get(Inode::get_block_group(self.inode_number as usize))
            .raw
            .inode_table;
        let inode_index = Inode::get_table_index(self.inode_number as usize);

        ahci::write(
            0,
            (starting_lba * 512
                + inode_table as usize * block_size
                + inode_index as usize * size_of::<Inode>()) as u64,
            size_of::<Inode>(),
            self as *const Inode as *const u8,
        )
        .unwrap();
    }

    // TODO: test it
    pub fn resize(&mut self, new_size: usize) {
        if new_size == self.sizel as usize {
            return;
        }

        let fs = unsafe { EXT2_FS.clone().unwrap() };

        let new_block_cnt = div_ceil(new_size, fs.block_size);
        let old_block_cnt = div_ceil(self.sizel as usize, fs.block_size);

        if new_block_cnt == old_block_cnt {
            return;
        }

        if new_block_cnt > old_block_cnt {
            for i in old_block_cnt..new_block_cnt {
                let new_block = fs
                    .alloc_block()
                    .expect("[EXT2] Could not allocate a new block");

                self.set_block_address(i, new_block);
            }
        } else {
            // TODO: free the blocks
        }

        self.sizel = new_size as u32;
        self.sectors_used = ((new_block_cnt * fs.block_size) / 512) as u32;
        self.flush();
    }

    pub fn read(&self, offset: usize, bytes: usize, buffer: *mut u8) -> Result<usize, ()> {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let block_size = fs.block_size;
        let starting_lba = fs.starting_lba;

        let mut bytes_read = 0;
        let mut blocks_read = 0;

        while bytes_read < bytes {
            let block_address = self.get_block_address(offset / block_size + blocks_read);
            serial::print!("block address: {}\n", block_address);
            let count = if bytes_read + block_size <= bytes {
                block_size
            } else {
                if bytes < block_size {
                    bytes
                } else {
                    bytes % bytes_read
                }
            };

            ahci::read(
                0,
                (starting_lba * 512 + block_address as usize * block_size + offset) as u64,
                count,
                buffer,
            )?;

            blocks_read += 1;
            bytes_read += count;
        }

        Ok(bytes_read)
    }

    pub fn write(&mut self, offset: usize, bytes: usize, buffer: *const u8) -> Result<usize, ()> {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let block_size = fs.block_size;
        let starting_lba = fs.starting_lba;

        let mut bytes_written = 0;
        let mut blocks_written = 0;

        self.resize(offset + bytes);

        while bytes_written < bytes {
            let block_address = self.get_block_address(offset / block_size + blocks_written);
            serial::print!("block address: {}\n", block_address);
            let count = if bytes_written + block_size <= bytes {
                block_size
            } else {
                if bytes < block_size {
                    bytes
                } else {
                    bytes % bytes_written
                }
            };

            ahci::write(
                0,
                (starting_lba * 512 + block_address as usize * block_size + offset) as u64,
                count,
                buffer,
            )?;

            blocks_written += 1;
            bytes_written += count;
        }

        Ok(bytes_written)
    }

    pub fn get_block_address(&self, mut block_index: usize) -> u32 {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let block_size = fs.block_size;
        let starting_lba = fs.starting_lba;

        if block_index < 12 {
            return self.direct_pointer[block_index];
        }

        let addresses_per_block = block_size / 4;
        let mut block_address: u32 = 0;
        block_index -= 12;

        if block_index < addresses_per_block {
            // singly indirect
            ahci::read(
                0,
                (starting_lba * 512 + self.singly_ip as usize * block_size + block_index * 4)
                    as u64,
                4,
                &mut block_address as *mut u32 as *mut u8,
            )
            .unwrap(); // TODO: handle the error like a MAN

            return block_address;
        }

        block_index -= addresses_per_block;

        if block_index < addresses_per_block * addresses_per_block {
            // doubly indirect
            let mut indirect: u32 = 0;

            ahci::read(
                0,
                (starting_lba * 512
                    + self.doubly_ip as usize * block_size
                    + (block_index / addresses_per_block) * 4) as u64,
                4,
                &mut indirect as *mut u32 as *mut u8,
            )
            .unwrap(); // TODO: handle the error like a MAN

            ahci::read(
                0,
                (starting_lba * 512
                    + indirect as usize * block_size
                    + (block_index % addresses_per_block) * 4) as u64,
                4,
                &mut block_address as *mut u32 as *mut u8,
            )
            .unwrap(); // TODO: handle the error like a MAN

            return block_address;
        }

        block_index -= addresses_per_block * addresses_per_block;

        // triply indirect

        let base = block_index % (addresses_per_block * addresses_per_block);
        let mut indirect1: u32 = 0;
        let mut indirect2: u32 = 0;

        ahci::read(
            0,
            (starting_lba * 512
                + self.triply_ip as usize * block_size
                + (block_index / (addresses_per_block * addresses_per_block)) * 4)
                as u64,
            4,
            &mut indirect1 as *mut u32 as *mut u8,
        )
        .unwrap(); // TODO: handle the error like a MAN

        ahci::read(
            0,
            (starting_lba * 512 + indirect1 as usize * block_size + (base / 1024) * 4) as u64,
            4,
            &mut indirect2 as *mut u32 as *mut u8,
        )
        .unwrap(); // TODO: handle the error like a MAN

        ahci::read(
            0,
            (starting_lba * 512 + indirect2 as usize * block_size + (base % 1024) * 4) as u64,
            4,
            &mut block_address as *mut u32 as *mut u8,
        )
        .unwrap(); // TODO: handle the error like a MAN

        block_address
    }

    pub fn set_block_address(&mut self, mut block_index: usize, block_address: u32) {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let block_size = fs.block_size;
        let starting_lba = fs.starting_lba;

        if block_index < 12 {
            self.direct_pointer[block_index] = block_address;
            self.flush();
            return;
        }

        let addresses_per_block = block_size / 4;
        block_index -= 12;

        if block_index < addresses_per_block {
            // singly indirect
            if self.singly_ip == 0 {
                // TODO: zero the new block?
                self.singly_ip = fs
                    .alloc_block()
                    .expect("[EXT2] Could not allocate a new block");

                self.flush();
            }

            ahci::write(
                0,
                (starting_lba * 512 + self.singly_ip as usize * block_size + block_index * 4)
                    as u64,
                4,
                &block_address as *const u32 as *const u8,
            )
            .unwrap(); // TODO: handle the error like a MAN

            return;
        }

        block_index -= addresses_per_block;

        if block_index < addresses_per_block * addresses_per_block {
            // doubly indirect
            let mut indirect: u32 = 0;

            /*
                The doubly indirect pointer hasn't been allocated yet,
                so we allocate it along with the new singly indirect pointer
                entry
            */
            if self.doubly_ip == 0 {
                // TODO: zero the new block?
                self.doubly_ip = fs
                    .alloc_block()
                    .expect("[EXT2] Could not allocate a new block");

                self.flush();

                indirect = fs
                    .alloc_block()
                    .expect("[EXT2] Could not allocate a new block");

                ahci::write(
                    0,
                    (starting_lba * 512
                        + self.doubly_ip as usize * block_size
                        + (block_index / addresses_per_block) * 4) as u64,
                    4,
                    &mut indirect as *mut u32 as *mut u8,
                )
                .unwrap(); // TODO: handle the error like a MAN
            } else {
                ahci::read(
                    0,
                    (starting_lba * 512
                        + self.doubly_ip as usize * block_size
                        + (block_index / addresses_per_block) * 4) as u64,
                    4,
                    &mut indirect as *mut u32 as *mut u8,
                )
                .unwrap(); // TODO: handle the error like a MAN
            }

            ahci::write(
                0,
                (starting_lba * 512
                    + indirect as usize * block_size
                    + (block_index % addresses_per_block) * 4) as u64,
                4,
                &block_address as *const u32 as *const u8,
            )
            .unwrap(); // TODO: handle the error like a MAN

            return;
        }

        block_index -= addresses_per_block * addresses_per_block;

        // TODO: finish this lol
        // triply indirect

        // let base = block_index % (addresses_per_block * addresses_per_block);
        // let mut indirect1: u32 = 0;
        // let mut indirect2: u32 = 0;

        // ahci::read(
        //     0,
        //     (starting_lba * 512
        //         + self.triply_ip as usize * block_size
        //         + (block_index / (addresses_per_block * addresses_per_block)) * 4)
        //         as u64,
        //     4,
        //     &mut indirect1 as *mut u32 as *mut u8,
        // )
        // .unwrap(); // TODO: handle the error like a MAN

        // ahci::read(
        //     0,
        //     (starting_lba * 512 + indirect1 as usize * block_size + (base / 1024) * 4) as u64,
        //     4,
        //     &mut indirect2 as *mut u32 as *mut u8,
        // )
        // .unwrap(); // TODO: handle the error like a MAN

        // ahci::read(
        //     0,
        //     (starting_lba * 512 + indirect2 as usize * block_size + (base % 1024) * 4) as u64,
        //     4,
        //     &mut block_address as *mut u32 as *mut u8,
        // )
        // .unwrap(); // TODO: handle the error like a MAN
    }

    pub fn get(inode_addr: u32) -> Box<Inode> {
        let inode_block_group = Inode::get_block_group(inode_addr as usize);

        let block_group = BlockGroup::get(inode_block_group);
        block_group.get_inode(inode_addr)
    }
}

#[repr(C, packed)]
#[derive(Debug)]
struct DirectoryEntry {
    inode: u32,
    entry_size: u16,
    name_length: u8,
    ti_or_length: u8,
    entry_name: [u8; 0],
}

impl DirectoryEntry {
    pub fn search(inode: &Inode, name: &str) -> Option<u32> {
        if !inode.is_directory() {
            return None;
        }

        // just try to search a big directory and we will have some serious troubles
        let entries_buffer = PmmBox::<u8>::new(inode.sizel as usize).as_mut_ptr();

        inode.read(0, inode.sizel as usize, entries_buffer).unwrap();

        let mut i = 0;
        while i < inode.sizel {
            let curr_entry =
                unsafe { &*(entries_buffer.offset(i as isize) as *mut DirectoryEntry) };

            i += curr_entry.entry_size as u32;

            if curr_entry.inode == 0 || curr_entry.name_length as usize != name.len() {
                continue;
            }

            let entry_name = unsafe {
                core::slice::from_raw_parts(
                    curr_entry.entry_name.as_ptr(),
                    curr_entry.name_length as usize,
                )
            };

            if entry_name == name.as_bytes() {
                return Some(curr_entry.inode);
            }
        }

        None
    }

    pub fn add_entry(dir: &mut Inode, inode: u32, name: &str) -> Result<(), ()> {
        if !dir.is_directory() {
            return Err(());
        }

        let entries_buffer = PmmBox::<u8>::new(dir.sizel as usize).as_mut_ptr();

        dir.read(0, dir.sizel as usize, entries_buffer).unwrap();

        let mut i = 0;
        while i < dir.sizel {
            let curr_entry =
                unsafe { &mut *(entries_buffer.offset(i as isize) as *mut DirectoryEntry) };

            let mut true_size = size_of::<DirectoryEntry>() + curr_entry.name_length as usize;

            /*
                The size of every entry must be a multiple of 4 so that each
                directory entry is guaranted to be 4 bytes aligned
            */
            true_size = round_up(true_size, 4);

            // the entry has some empty space in it
            if curr_entry.entry_size as usize > true_size {
                let empty_space = curr_entry.entry_size as usize - true_size;

                let mut space_needed = size_of::<DirectoryEntry>() + name.len();
                space_needed = round_up(space_needed, 4);

                // if the empty space is not large enough to store the new entry, we continue the loop
                if empty_space < space_needed {
                    i += curr_entry.entry_size as u32;
                    continue;
                }

                let new_entry = unsafe {
                    &mut *(entries_buffer.offset((i as usize + true_size) as isize)
                        as *mut DirectoryEntry)
                };

                curr_entry.entry_size = true_size as u16;
                new_entry.name_length = name.len() as u8;
                new_entry.inode = inode;
                new_entry.entry_size = empty_space as u16;
                new_entry.ti_or_length = 1;

                unsafe {
                    new_entry
                        .entry_name
                        .as_mut_ptr()
                        .copy_from(name.as_ptr(), name.len());
                }

                dir.write(0, dir.sizel as usize, entries_buffer).unwrap();

                return Ok(());
            }

            i += curr_entry.entry_size as u32;
        }

        Err(())
    }
}

pub struct Ext2Filesystem {
    superblock: Box<Superblock>,
    block_size: usize,
    block_group_cnt: usize,
    starting_lba: usize,
}

impl Ext2Filesystem {
    pub fn new(starting_lba: u64, superblock: Box<Superblock>) -> Self {
        Ext2Filesystem {
            block_size: 1024 << superblock.block_size,
            block_group_cnt: div_ceil(
                superblock.block_cnt as usize,
                superblock.blocks_per_group as usize,
            ),
            superblock,
            starting_lba: starting_lba as usize,
        }
    }

    // TODO: allocate multiple blocks at the same time
    pub fn alloc_block(&self) -> Option<u32> {
        if self.superblock.unallocated_blocks == 0 {
            return None;
        }

        for bg in 0..self.block_group_cnt {
            let mut block_group = BlockGroup::get(bg);

            if let Some(block_addr) = block_group.alloc_block(1) {
                // TODO: make this possible
                // self.superblock.unallocated_blocks -= 1;
                // self.superblock.flush();
                return Some(block_addr[0]);
            }
        }

        None
    }

    pub fn alloc_inode(&self) -> Option<u32> {
        if self.superblock.unallocated_inodes == 0 {
            return None;
        }

        for bg in 0..self.block_group_cnt {
            let mut block_group = BlockGroup::get(bg);

            if let Some(inode_addr) = block_group.alloc_inode() {
                // TODO: make this possible
                // self.superblock.unallocated_inodes -= 1;
                // self.superblock.flush();
                return Some(inode_addr);
            }
        }

        None
    }

    pub fn new_fd(&self, inode: Box<Inode>, flags: vfs::Flags) -> Option<vfs::FileDescription> {
        for (i, slot) in unsafe { INODE_TABLE.iter().enumerate() } {
            match slot {
                Some(_) => {
                    continue;
                }
                None => unsafe {
                    INODE_TABLE[i] = Some(inode);
                    let fd = vfs::FileDescription::new(i, flags, EXT2_FS.as_ref().unwrap().deref());
                    return Some(fd);
                },
            }
        }

        None
    }
}

impl vfs::Filesystem for Ext2Filesystem {
    fn open(&self, path: &str, flags: vfs::Flags, mode: vfs::Mode) -> Option<vfs::FileDescription> {
        serial::print!("open path: {}\n", path);
        let root_dir = Inode::get(ROOT_DIR_INODE);
        let mut current_dir = root_dir;
        let path: Vec<&str> = path.split('/').collect();
        serial::print!("path vector: {:?}\n", path);

        // TODO: some more testing
        for (i, path_fragment) in path.iter().enumerate() {
            if *path_fragment == "" {
                continue;
            }

            if let Some(inode_addr) = DirectoryEntry::search(&current_dir, path_fragment) {
                let entry_inode = Inode::get(inode_addr);

                if i + 1 == path.len() {
                    return self.new_fd(entry_inode, flags);
                }

                if !entry_inode.is_directory() {
                    return None;
                }

                current_dir = entry_inode;
            } else {
                if i + 1 == path.len() && flags.contains(vfs::Flags::O_CREAT) {
                    let new_inode_addr = self
                        .alloc_inode()
                        .expect("[EXT2] Could not allocate a new inode");

                    let mut new_inode = Inode::get(new_inode_addr);
                    new_inode.type_and_permissions = 0x81ed;
                    new_inode.ref_cnt = 1;
                    new_inode.flush();

                    DirectoryEntry::add_entry(&mut current_dir, new_inode_addr, path_fragment)
                        .unwrap();

                    return self.new_fd(new_inode, flags);
                }

                return None;
            }
        }

        None
    }

    fn mkdir(&self, path: &str, mode: vfs::Mode) -> Option<vfs::FileDescription> {
        todo!()
    }

    fn read(&self, index: usize, buffer: *mut u8, cnt: usize, offset: usize) -> usize {
        let inode_option = unsafe { INODE_TABLE[index].as_ref() };

        if let Some(inode) = inode_option {
            inode.read(offset, cnt, buffer).unwrap()
        } else {
            //TODO: report the error somehow
            0
        }
    }

    fn write(&self, index: usize, buffer: *const u8, cnt: usize, offset: usize) -> usize {
        let inode_option = unsafe { INODE_TABLE[index].as_mut() };

        if let Some(inode) = inode_option {
            inode.write(offset, cnt, buffer).unwrap()
        } else {
            //TODO: report the error somehow
            0
        }
    }
}

pub fn try_and_init(starting_lba: u64) -> Result<(), ()> {
    let superblock = unsafe {
        alloc::alloc::alloc(alloc::alloc::Layout::new::<Superblock>()) as *mut Superblock
    };

    // superblock is always located at LBA 2 of the volume
    ahci::read(
        0,
        (starting_lba + 2) * 512,
        size_of::<Superblock>(),
        superblock as *mut u8,
    )?;

    let superblock = unsafe { Box::from_raw(superblock) };

    if superblock.signature != EXT2_SIGNATURE {
        serial::print!("not ext2\n");
        serial::print!("signature: {:#x}\n", superblock.signature);
        return Err(());
    }

    serial::print!("Found an ext2 filesystem!\n");
    serial::print!(
        "Block size: {}, Inode count: {}\n",
        1024 << superblock.block_size,
        superblock.inode_cnt
    );

    unsafe { EXT2_FS = Some(Arc::new(Ext2Filesystem::new(starting_lba, superblock))) };
    Ok(())
}

pub fn get() -> &'static mut Ext2Filesystem {
    unsafe {
        &mut *(EXT2_FS.as_ref().unwrap().deref() as *const Ext2Filesystem as *mut Ext2Filesystem)
    }
}
