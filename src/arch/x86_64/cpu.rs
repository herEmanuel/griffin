use crate::arch::{gdt, mm::pmm};
use crate::serial;
use alloc::boxed::Box;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct InterruptContext {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(C, packed)]
#[derive(Default)]
pub struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved2: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved3: u64,
    iobm: u32,
}

#[derive(Default)]
pub struct Cpuid {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
}

impl Cpuid {
    pub fn raw(eax: u32, ecx: u32) -> Self {
        let mut res = Cpuid::default();

        unsafe {
            asm!("cpuid", "mov rdi, rbx", in("eax") eax, in("ecx") ecx, 
                    lateout("rax") res.rax, lateout("rdi") res.rbx, lateout("rcx") res.rcx, lateout("rdx") res.rdx)
        }

        res
    }

    pub fn has_smap() -> bool {
        let res = Cpuid::raw(7, 0);
        if res.rbx & 1 << 20 != 0 {
            true
        } else {
            false
        }
    }

    pub fn has_smep() -> bool {
        let res = Cpuid::raw(7, 0);
        if res.rbx & 1 << 7 != 0 {
            true
        } else {
            false
        }
    }

    pub fn has_fsgsbase() -> bool {
        let res = Cpuid::raw(7, 0);
        if res.rbx & 1 != 0 {
            true
        } else {
            false
        }
    }

    pub fn has_umip() -> bool {
        let res = Cpuid::raw(7, 0);
        if res.rcx & 1 << 2 != 0 {
            true
        } else {
            false
        }
    }
}

pub fn start() {
    init_features();

    let mut tss = Box::new(Tss::default());
    tss.rsp0 = pmm::get()
        .calloc(2)
        .expect("Could not allocate the pages for rsp0")
        .higher_half()
        .as_u64();

    // page fault's ist
    tss.ist1 = pmm::get()
        .calloc(2)
        .expect("Could not allocate the pages for rsp0")
        .higher_half()
        .as_u64();

    let leaked_tss = Box::leak(tss);
    unsafe {
        gdt::load_tss(leaked_tss as *mut Tss as u64);
    }
}

pub fn init_features() {
    let mut cr4: u64;
    unsafe {
        asm!("mov {}, cr4", out(reg) cr4);
    }

    if Cpuid::has_smap() {
        cr4 |= 1 << 21;
    }

    if Cpuid::has_smep() {
        cr4 |= 1 << 20;
    }

    if Cpuid::has_umip() {
        cr4 |= 1 << 11;
    }

    if Cpuid::has_fsgsbase() {
        cr4 |= 1 << 16;
    }

    unsafe {
        asm!("mov cr4, {}", in(reg) cr4);
    }
}

#[repr(u32)]
pub enum MsrList {
    ApicBase = 0x1b,
    GsBase = 0xc0000101,
}

pub fn rdmsr(msr: MsrList) -> u64 {
    let mut low: u32;
    let mut high: u32;

    unsafe {
        asm!("rdmsr", in("ecx") msr as u32, out("eax") low, out("edx") high);
    }

    low as u64 | (high as u64) << 32
}

pub fn wrmsr(msr: MsrList, value: u64) {
    unsafe {
        asm!("wrmsr", in("ecx") msr as u32, in("eax") value as u32, in("edx") (value >> 32) as u32);
    }
}

pub fn halt() -> ! {
    unsafe {
        loop {
            asm!("hlt");
        }
    }
}

pub fn sti() {
    unsafe {
        asm!("sti");
    }
}
