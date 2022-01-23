use super::ext2;
use crate::arch::mm::pmm::{self, PmmBox};
use crate::drivers::ahci;
use crate::serial;
use crate::utils::math::div_ceil;
use alloc::alloc::{alloc, dealloc, Layout};
use core::intrinsics::size_of;

#[repr(C, packed)]
struct GptHeader {
    signature: [u8; 8],
    revision: u32,
    hdr_size: u32,
    checksum: u32,
    reserved: u32,
    hdr_lba: u64,
    alt_hdr_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: [u8; 16],
    start_lba: u64,
    partition_entries: u32,
    entry_size: u32,
    pea_checksum: u32,
}

#[repr(C, packed)]
struct GptPartitionEntry {
    pt_guid: [u64; 2],
    unique_guid: [u64; 2],
    start_lba: u64,
    end_lba: u64,
    attributes: u64,
    name: [u8; 72],
}

pub fn scan() -> Result<(), ()> {
    let gpt_header_layout = Layout::new::<GptHeader>();
    let gpt_header = unsafe { &mut *(alloc(gpt_header_layout) as *mut GptHeader) };
    ahci::read(
        0,
        512,
        size_of::<GptHeader>(),
        gpt_header as *mut GptHeader as *mut u8,
    )?;

    if gpt_header
        .signature
        .iter()
        .zip(b"EFI PART".iter())
        .all(|(a, b)| a != b)
    {
        return scan_mbr();
    }

    serial::print!(
        "revision: {}, starting lba: {}, partitions: {}, first and last block: {} and {}\n",
        gpt_header.revision,
        gpt_header.start_lba,
        gpt_header.partition_entries,
        gpt_header.first_usable,
        gpt_header.last_usable
    );

    let gpt_entries = PmmBox::<GptPartitionEntry>::new(
        gpt_header.partition_entries as usize * size_of::<GptPartitionEntry>(),
    )
    .as_mut_ptr();

    ahci::read(
        0,
        gpt_header.start_lba * 512,
        gpt_header.partition_entries as usize * size_of::<GptPartitionEntry>(),
        gpt_entries as *mut u8,
    )?;

    for i in 0..gpt_header.partition_entries {
        let entry = unsafe { &*gpt_entries.offset(i as isize) };

        if entry.pt_guid[0] == 0 {
            // unused entry
            continue;
        }

        serial::print!("Found a partition at LBA {}\n", entry.start_lba);
        ext2::try_and_init(entry.start_lba);
    }

    unsafe {
        dealloc(gpt_header as *mut GptHeader as *mut u8, gpt_header_layout);
    }

    Ok(())
}

fn scan_mbr() -> Result<(), ()> {
    todo!()
}
