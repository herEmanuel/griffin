#![no_std]
#![no_main]
#![feature(asm)]
#![feature(naked_functions)]
#![feature(asm_sym)]
#![feature(default_alloc_error_handler)]
#![feature(panic_info_message)]
#![feature(core_intrinsics)]

extern crate alloc;

use arch::x86_64::cpu;
use core::panic::PanicInfo;
use mm::{slab, vmm};
use stivale_boot::v2::{
    StivaleFramebufferHeaderTag, StivaleHeader, StivaleMemoryMapEntry, StivaleStruct,
};

pub mod arch;
pub mod drivers;
pub mod fs;
pub mod mm;
pub mod serial;
pub mod spinlock;
pub mod utils;
pub mod video;

static STACK: [u8; 8192] = [0; 8192];
static FRAMEBUFFER_HEADER_TAG: StivaleFramebufferHeaderTag = StivaleFramebufferHeaderTag::new();

#[link_section = ".stivale2hdr"]
#[no_mangle]
#[used]
static STIVALE_HEADER: StivaleHeader = StivaleHeader::new()
    .flags(30)
    .stack(&STACK[8191] as *const u8)
    .tags((&FRAMEBUFFER_HEADER_TAG as *const StivaleFramebufferHeaderTag) as *const ());

#[no_mangle]
extern "C" fn _start(_tags: usize) -> ! {
    let tags;
    unsafe { tags = &*(_tags as *const StivaleStruct) }

    serial::SerialWriter::init();

    let framebuffer_tag = tags.framebuffer().unwrap();
    let mmap_tag = tags.memory_map().unwrap();
    let rsdp_tag = tags.rsdp().unwrap();
    serial::print!("rsdp at: {:#x}\n", rsdp_tag.rsdp);

    let mut video = video::Video::new(framebuffer_tag);

    video.print("Hello, world, from Rust!\n");
    video.print("Is everything fine?");

    unsafe {
        arch::x86_64::gdt::init();
        arch::x86_64::idt::init();

        arch::x86_64::mm::pmm::init(
            &mmap_tag.entry_array as *const StivaleMemoryMapEntry,
            mmap_tag.entries_len,
        );
        serial::print!("pmm done yey\n");
        vmm::init();
        slab::init();
        arch::x86_64::acpi::init(rsdp_tag);
    }

    serial::print!("slab allocator running\n");

    arch::x86_64::pci::enumerate_devices();
    fs::partitions::scan();

    unsafe {
        cpu::halt();
    }
}

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    serial::print!("PANIC: {}\n", _info.message().unwrap());
    unsafe {
        cpu::halt();
    }
}
