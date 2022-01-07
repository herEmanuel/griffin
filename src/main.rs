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
use mm::slab;
use stivale_boot::v2::{
    StivaleFramebufferHeaderTag, StivaleHeader, StivaleMemoryMapEntry, StivaleStruct,
};

pub mod arch;
pub mod drivers;
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

    let mut video = video::Video::new(framebuffer_tag);

    video.print("Hello, world, from Rust!\n");
    video.print("Is everything fine?");

    // video.print("Lorem ipsum dolor sit amet, consectetur adipiscing elit. Suspendisse dictum ligula erat, sit amet lobortis lacus facilisis id. Cras at congue enim. Aenean arcu arcu, aliquam vitae vehicula id, cursus in diam. Quisque et elit euismod, pulvinar nunc eget, venenatis felis. Etiam leo lorem, egestas ut luctus sit amet, posuere quis velit. Quisque ac felis suscipit, facilisis libero et, rhoncus felis. Curabitur id congue leo. Donec lobortis, arcu ac hendrerit commodo, velit ante tempus libero, sed pellentesque sem turpis sed tortor. Vestibulum feugiat egestas mauris vulputate ultrices. ");

    unsafe {
        arch::x86_64::gdt::init();
        arch::x86_64::idt::init();

        arch::x86_64::mm::pmm::init(
            &mmap_tag.entry_array as *const StivaleMemoryMapEntry,
            mmap_tag.entries_len,
        );
        serial::print!("pmm done yey\n");
        slab::init();
    }

    serial::print!("slab allocator running\n");

    arch::x86_64::pci::enumerate_devices();
    unsafe {
        slab::SLAB_ALLOCATOR.dump();
    }
    let mut msg = alloc::string::String::from("hellooooppl");
    msg.push_str("ayup");

    msg.push_str("huh");
    serial::print!("{}\n", msg);

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
