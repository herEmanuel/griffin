use core::{intrinsics::size_of, ptr::null_mut};
use stivale_boot::v2::StivaleRsdpTag;

#[repr(C, packed)]
struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_addr: u32,

    // acpi versiom 2.0 or greater
    length: u32,
    xsdt_addr: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[repr(C, packed)]
pub struct Sdt {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

impl Sdt {
    fn data_address(&self) -> u64 {
        unsafe { (self as *const _ as *const u8).offset(size_of::<Sdt>() as isize) as u64 }
    }
}

static mut RSDP: *mut Rsdp = null_mut();

pub fn init(rsdp_tag: &StivaleRsdpTag) {
    let rsdp = rsdp_tag.rsdp as *mut Rsdp;

    unsafe {
        RSDP = rsdp;
    }
}

pub unsafe fn find_table(signature: [u8; 4]) -> Option<&'static Sdt> {
    if (*RSDP).revision == 0 {
        let rsdt_header = &*((*RSDP).rsdt_addr as *const Sdt);
        let table_cnt = (rsdt_header.length - size_of::<Sdt>() as u32) / 4;

        let tables = rsdt_header.data_address() as *const u32;

        for i in 0..table_cnt {
            let curr_table = &*(*tables.offset(i as isize) as *const Sdt);
            if curr_table
                .signature
                .iter()
                .zip(signature.iter())
                .all(|(a, b)| a == b)
            {
                return Some(curr_table);
            }
        }
    } else {
        let xsdt_header = &*((*RSDP).xsdt_addr as *const Sdt);
        let table_cnt = (xsdt_header.length - size_of::<Sdt>() as u32) / 8;

        let tables = xsdt_header.data_address() as *const u64;

        for i in 0..table_cnt {
            let curr_table = &*(*tables.offset(i as isize) as *const Sdt);
            if curr_table
                .signature
                .iter()
                .zip(signature.iter())
                .all(|(a, b)| a == b)
            {
                return Some(curr_table);
            }
        }
    }

    None
}
