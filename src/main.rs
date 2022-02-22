#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_sym)]
#![feature(default_alloc_error_handler)]
#![feature(panic_info_message)]
#![feature(core_intrinsics)]

extern crate alloc;

pub mod arch;
pub mod drivers;
pub mod fs;
pub mod mm;
pub mod proc;
pub mod serial;
pub mod utils;
pub mod video;

use arch::cpu;
use core::{panic::PanicInfo, mem::align_of};
use core::arch::asm;
use fs::{partitions, vfs};
use mm::{slab, vmm};
use stivale_boot::v2::{
    StivaleFramebufferHeaderTag, StivaleHeader, StivaleMemoryMapEntry, StivaleStruct,
};

#[repr(align(16))]
struct AlignedArray<T>(T);

// we do not want to overflow this shit again...
const STACK_SIZE: usize = 0x1000 * 16;

static STACK: AlignedArray<[u8; STACK_SIZE]> = AlignedArray([0; STACK_SIZE]);
static FRAMEBUFFER_HEADER_TAG: StivaleFramebufferHeaderTag = StivaleFramebufferHeaderTag::new();

#[link_section = ".stivale2hdr"]
#[no_mangle]
#[used]
static STIVALE_HEADER: StivaleHeader = StivaleHeader::new()
    .flags(30)
    .stack(&STACK.0[STACK_SIZE - 1] as *const u8)
    .tags((&FRAMEBUFFER_HEADER_TAG as *const StivaleFramebufferHeaderTag) as *const ());

#[no_mangle]
unsafe extern "C" fn _start(tags: &'static StivaleStruct) -> ! {
    let framebuffer_tag = tags.framebuffer().unwrap();
    let mmap_tag = tags.memory_map().unwrap();
    let rsdp_tag = tags.rsdp().unwrap();

    serial::SerialWriter::init();

    let mut video = video::Video::new(framebuffer_tag);

    video.print("Hello, world, from Rust!\n");
    video.print("Is everything fine?");

    arch::mm::pmm::init(
        &mmap_tag.entry_array as *const StivaleMemoryMapEntry,
        mmap_tag.entries_len,
    );
    slab::init();
    arch::gdt::init();
    arch::interrupts::init();
    vmm::init();
    cpu::start();
    arch::acpi::init(rsdp_tag);
    
    drivers::hpet::init();
   
    arch::apic::init();
    // arch::apic::get().calibrate_timer(1000);

    arch::pci::enumerate_devices();
    partitions::scan();
    vfs::mount(fs::ext2::get(), "/");
    let mut fd = vfs::open("/home/limine.cfg", vfs::Flags::empty(), vfs::Mode::empty()).unwrap();
    serial::print!("file index: {}\n", fd.file_index);

    let mut content = alloc::vec::Vec::with_capacity(50);
    vfs::read(fd.fs, fd.file_index, content.as_mut_ptr(), 50, fd.offset);
    content.set_len(50);
    serial::print!(
        "res: {}\n",
        core::str::from_utf8(content.as_slice()).unwrap()
    );
    
    proc::process::init_bitmaps(); 
    proc::process::Process::new(alloc::string::String::from("crap"), 0, None);
    serial::print!("hey!\n");
    cpu::halt();
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    let location = info.location().unwrap();
    serial::print!(
        "PANIC at file {}, line {}: {}\n",
        location.file(),
        location.line(),
        info.message().unwrap()
    );
    cpu::halt();
}
