use crate::drivers::ahci;
use crate::serial;
use crate::utils::math::div_ceil;
use alloc::boxed::Box;
use core::intrinsics::size_of;
use core::u8;

const EXT2_SIGNATURE: u16 = 0xef53;

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
struct BlockGroupDescriptor {
    block_bitmap: u32,
    inode_bitmap: u32,
    inode_table: u32,
    unallocated_blocks: u16,
    unallocated_inodes: u16,
    directories_cnt: u16,
    unused: [u8; 14],
}

#[repr(packed)]
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
    pub fn read(
        &self,
        fs: &Ext2Filesystem,
        offset: usize,
        bytes: usize,
        buffer: *mut u8,
    ) -> Result<usize, ()> {
        let mut bytes_read = 0;
        let mut blocks_read = 0;

        while bytes_read < bytes {
            let block_address =
                self.get_block_address(fs.block_size, offset / fs.block_size + blocks_read);

            let count = if bytes_read + fs.block_size < bytes {
                fs.block_size
            } else {
                if bytes < fs.block_size {
                    bytes
                } else {
                    bytes % bytes_read
                }
            };

            ahci::read(
                0,
                (block_address as usize * fs.block_size + offset) as u64,
                count,
                buffer,
            )?;

            blocks_read += 1;
            bytes_read += count;
        }

        Ok(bytes_read)
    }

    pub fn get_block_address(&self, block_size: usize, mut block_index: usize) -> u32 {
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
                (self.singly_ip as usize * block_size + block_index * 4) as u64,
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
                (self.doubly_ip as usize * block_size + (block_index / addresses_per_block) * 4)
                    as u64,
                4,
                &mut indirect as *mut u32 as *mut u8,
            )
            .unwrap(); // TODO: handle the error like a MAN

            ahci::read(
                0,
                (indirect as usize * block_size + (block_index % addresses_per_block) * 4) as u64,
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
            (self.triply_ip as usize * block_size
                + (block_index / (addresses_per_block * addresses_per_block)) * 4)
                as u64,
            4,
            &mut indirect1 as *mut u32 as *mut u8,
        )
        .unwrap(); // TODO: handle the error like a MAN

        ahci::read(
            0,
            (indirect1 as usize * block_size + (base / 1024) * 4) as u64,
            4,
            &mut indirect2 as *mut u32 as *mut u8,
        )
        .unwrap(); // TODO: handle the error like a MAN

        ahci::read(
            0,
            (indirect2 as usize * block_size + (base % 1024) * 4) as u64,
            4,
            &mut block_address as *mut u32 as *mut u8,
        )
        .unwrap(); // TODO: handle the error like a MAN

        block_address
    }
}

struct Ext2Filesystem {
    superblock: Box<Superblock>,
    block_size: usize,
    block_group_cnt: usize,
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

    Ok(())
}
