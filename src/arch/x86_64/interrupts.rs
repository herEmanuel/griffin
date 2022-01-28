use crate::serial;

#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    offset: u64,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IdtGate {
    offset1: u16,
    selector: u16,
    ist: u8,
    gate_type: u8,
    offset2: u16,
    offset3: u32,
    zero: u32,
}

impl IdtGate {
    const fn new(offset: u64, ist: u8, gate_type: u8, selector: u16) -> Self {
        IdtGate {
            offset1: (offset & 0xffff) as u16,
            selector,
            ist,
            gate_type,
            offset2: ((offset >> 16) & 0xffff) as u16,
            offset3: (offset >> 32) as u32,
            zero: 0,
        }
    }
}

macro_rules! isr {
    ($name:ident, |$stack: ident| $code:block) => {
        #[naked]
        unsafe extern "C" fn $name() {
            unsafe extern "C" fn inner_isr($stack: &crate::arch::cpu::InterruptContext) {
                $code
            }

            asm!(
                "push r15",
                "push r14",
                "push r13",
                "push r12",
                "push r11",
                "push r10",
                "push r9",
                "push r8",
                "push rbp",
                "push rdi",
                "push rsi",
                "push rdx",
                "push rcx",
                "push rbx",
                "push rax",
                "cld",

                "mov rdi, rsp",
                "call {isr}",

                "pop rax",
                "pop rbx",
                "pop rcx",
                "pop rdx",
                "pop rsi",
                "pop rdi",
                "pop rbp",
                "pop r8",
                "pop r9",
                "pop r10",
                "pop r11",
                "pop r12",
                "pop r13",
                "pop r14",
                "pop r15",
                "iretq",
                isr = sym inner_isr,
                options(noreturn)
            );
        }
    };
}

macro_rules! isr_err {
    ($name:ident, |$stack: ident, $error: ident| $code:block) => {
        #[naked]
        unsafe extern "C" fn $name() {
            unsafe extern "C" fn inner_isr($stack: &crate::arch::cpu::InterruptContext, $error: u64) {
                $code
            }

            asm!(
                "xchg [rsp], r15", // put the error code in r15 and r15 right after the rip
                "push r14",
                "push r13",
                "push r12",
                "push r11",
                "push r10",
                "push r9",
                "push r8",
                "push rbp",
                "push rdi",
                "push rsi",
                "push rdx",
                "push rcx",
                "push rbx",
                "push rax",
                "push r15", // push the error code
                "cld",

                "mov rdi, rsp",
                "call {isr}",

                "add rsp, 8", // get rid of the error code
                "pop rax",
                "pop rbx",
                "pop rcx",
                "pop rdx",
                "pop rsi",
                "pop rdi",
                "pop rbp",
                "pop r8",
                "pop r9",
                "pop r10",
                "pop r11",
                "pop r12",
                "pop r13",
                "pop r14",
                "pop r15",
                "iretq",
                isr = sym inner_isr,
                options(noreturn)
            );
        }
    };
}

pub(crate) use isr;
pub(crate) use isr_err;

static mut IDT: [IdtGate; 256] = [IdtGate::new(0, 0, 0, 0); 256];
static mut IDT_DESCRIPTOR: IdtDescriptor = IdtDescriptor {
    limit: 16 * 256,
    offset: 0,
};

pub unsafe fn register_isr(vector: usize, addr: u64, ist: u8, gate_type: u8) {
    IDT[vector] = IdtGate::new(addr, ist, gate_type, 0x8);
}

pub fn alloc_vector() -> Option<usize> {
    for i in 32..256 {
        if unsafe { IDT[i].gate_type } == 0 {
            return Some(i);
        }
    }

    None
}

pub unsafe fn init() {
    register_isr(0x3, int3 as u64, 0, 0x8e);
    register_isr(0x6, invalid_opcode as u64, 0, 0x8e);

    IDT_DESCRIPTOR.offset = &IDT as *const IdtGate as u64;
    asm!("lidt [{}]", in(reg) &IDT_DESCRIPTOR);
}

pub fn enable() {
    unsafe {
        asm!("sti");
    }
}

pub fn disable() {
    unsafe {
        asm!("cli");
    }
}

isr!(int3, |_stack| {
    serial::print!("Breakpoint yeeee\n");
});

isr!(invalid_opcode, |_stack| {
    serial::print!("INVALID OPCODE\n");
    loop {
        asm!("hlt");
    }
});
