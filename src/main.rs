#![no_std]
#![no_main]
#![feature(asm)]
#![feature(naked_functions)]
#![feature(asm_sym)]

use core::panic::PanicInfo;
use stivale_boot::v2::{StivaleFramebufferHeaderTag, StivaleHeader, StivaleStruct};

pub mod arch;
pub mod serial;
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

    serial::init();
    serial::print("Hello, world?\n");

    unsafe {
        arch::x86_64::gdt::init();
        arch::x86_64::idt::init();
    }

    let framebuffer_tag = tags.framebuffer().unwrap();
    let mut video = video::Video::new(framebuffer_tag);

    video.print("Hello, world, from Rust!");

    unsafe {
        asm!("int 0x3");
    }

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    serial::print("at panic handler\n");
    loop {}
}
