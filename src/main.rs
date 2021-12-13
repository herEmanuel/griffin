#![no_std]
#![no_main]
#![feature(asm)]

use core::panic::PanicInfo;
use stivale_boot::v2::{StivaleFramebufferHeaderTag, StivaleHeader, StivaleStruct};

static STACK: [u8; 8192] = [0; 8192];
static FRAMEBUFFER_HEADER_TAG: StivaleFramebufferHeaderTag = StivaleFramebufferHeaderTag::new();

#[link_section = ".stivale2hdr"]
#[no_mangle]
#[used]
static STIVALE_HEADER: StivaleHeader = StivaleHeader::new()
    .stack(&STACK[8191] as *const u8)
    .tags((&FRAMEBUFFER_HEADER_TAG as *const StivaleFramebufferHeaderTag) as *const ());

#[no_mangle]
extern "C" fn _start(_tags: usize) -> ! {
    let tags;
    unsafe { tags = &*(_tags as *const StivaleStruct) }

    let framebuffer_tag = tags.framebuffer().unwrap();

    let fb = framebuffer_tag.framebuffer_addr as *mut u32;
    for i in 0..100 {
        unsafe {
            *fb.offset(i) = 0xff6677;
        }
    }

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    loop {}
}
