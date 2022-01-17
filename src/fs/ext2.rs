use crate::arch::x86_64::mm::pmm;
use crate::drivers::ahci;
use crate::serial;
use crate::utils::bitmap;
use crate::utils::math::div_ceil;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::intrinsics::size_of;

const EXT2_SIGNATURE: u16 = 0xef53;
const ROOT_DIR_INODE: u32 = 0x2;

static mut EXT2_FS: Option<Arc<Ext2Filesystem>> = None;

bitflags::bitflags! {
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

#[repr(packed)]
struct Superblock {
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

#[repr(packed)]
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

    pub fn get_inode(&self, inode_index: usize) -> Box<Inode> {
        let fs = unsafe { EXT2_FS.clone().unwrap() };
        let starting_lba = fs.starting_lba;
        let block_size = fs.block_size;

        let inode =
            unsafe { alloc::alloc::alloc(alloc::alloc::Layout::new::<Inode>()) as *mut Inode };

        ahci::read(
            0,
            (starting_lba as usize * 512
                + self.raw.inode_table as usize * block_size
                + inode_index * size_of::<Inode>()) as u64,
            size_of::<Inode>(),
            inode as *mut u8,
        )
        .unwrap();

        let inode = unsafe { Box::from_raw(inode) };
        inode
    }

    pub fn alloc_block(&mut self, block_cnt: usize) -> Option<Vec<u32>> {
        if (self.raw.unallocated_blocks as usize) < block_cnt {
            return None;
        }

        let fs = unsafe { EXT2_FS.clone().unwrap() };

        let block_bitmap_pages = div_ceil(fs.block_size, pmm::PAGE_SIZE as usize);
        let block_bitmap: *mut u8 = pmm::get()
            .calloc(block_bitmap_pages)
            .expect("Could not allocate the pages for the block bitmap")
            .higher_half()
            .as_mut_ptr();

        ahci::read(
            0,
            (fs.starting_lba as usize * 512 + self.raw.block_bitmap as usize * fs.block_size)
                as u64,
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

                    self.raw.unallocated_blocks -= 1; //TODO: flush

                    if allocated == block_cnt {
                        break;
                    }
                }
            }
        }

        if allocated != block_cnt {
            pmm::get().free(block_bitmap, block_bitmap_pages);
            return None;
        }

        ahci::write(
            0,
            (fs.starting_lba as usize * 512 + self.raw.block_bitmap as usize * fs.block_size)
                as u64,
            fs.block_size,
            block_bitmap,
        )
        .unwrap();

        pmm::get().free(block_bitmap, block_bitmap_pages);

        Some(blocks)
    }
}

#[repr(packed)]
#[derive(Debug)]
struct Inode {
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
    os_specific: u32,
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
        self.type_and_permissions & FileType::DIRECTORY.bits() != 0
    }

    pub fn is_regular_file(&self) -> bool {
        self.type_and_permissions & FileType::NORMAL.bits() != 0
    }

    pub fn is_symlink(&self) -> bool {
        self.type_and_permissions & FileType::SYMLINK.bits() != 0
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

    pub fn get(inode_addr: u32) -> Box<Inode> {
        let inode_block_group = Inode::get_block_group(inode_addr as usize);
        let inode_table_index = Inode::get_table_index(inode_addr as usize);

        let block_group = BlockGroup::get(inode_block_group);

        block_group.get_inode(inode_table_index)
    }
}

#[repr(packed)]
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

        // TODO: free that (bruh im so lazy)
        // just try to search a big directory and we will have some serious troubles
        let entries_buffer: *mut u8 = pmm::get()
            .calloc(div_ceil(inode.sizel as usize, pmm::PAGE_SIZE as usize))
            .expect("Could not allocate the pages for the directory entries")
            .higher_half()
            .as_mut_ptr();

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
}

struct Ext2Filesystem {
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

    pub fn open(&self, raw_path: &str) -> Option<Box<Inode>> {
        serial::print!("===============at open==============\n");
        let root_dir = Inode::get(ROOT_DIR_INODE);
        let mut current_dir = root_dir;
        let path: Vec<&str> = raw_path.split('/').collect();
        serial::print!("path vector: {:?}\n", path);
        if path[0] != "" {
            // relative path, not supported yet
            return None;
        }

        // TODO: some more testing
        for (i, path_fragment) in path.iter().enumerate() {
            if *path_fragment == "" {
                continue;
            }

            if let Some(inode_addr) = DirectoryEntry::search(&current_dir, path_fragment) {
                let entry_inode = Inode::get(inode_addr);

                if i + 1 == path.len() {
                    return Some(entry_inode);
                }

                if !entry_inode.is_directory() {
                    return None;
                }

                current_dir = entry_inode;
            } else {
                return None; //TODO: create the inode when told so by the flags
            }
        }

        None
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
    unsafe {
        let fs = EXT2_FS.clone().unwrap();
        serial::print!(
            "file size: {}\n",
            fs.open("/home/limine.cfg").unwrap().sizel
        );

        let mut bgd = BlockGroup::get(0);
        serial::print!("========== GOT HERE ==============\n");
        let blocks = bgd.alloc_block(20).expect("could not allocate the blocks");
        serial::print!("blocks: {:?}\n", blocks);
    }
    Ok(())
}
