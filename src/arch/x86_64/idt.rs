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
    ($name:ident, $code:block) => {
        #[naked]
        unsafe extern "C" fn $name() {
            unsafe extern "C" fn inner_isr() {
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

static mut IDT: [IdtGate; 256] = [IdtGate::new(0, 0, 0, 0); 256];
static mut IDT_DESCRIPTOR: IdtDescriptor = IdtDescriptor {
    limit: 16 * 256,
    offset: 0,
};

pub unsafe fn register_isr(vector: usize, addr: u64, ist: u8, gate_type: u8) {
    IDT[vector] = IdtGate::new(addr, ist, gate_type, 0x8);
}

pub unsafe fn init() {
    register_isr(3, int3 as u64, 0, 0x8E);
    register_isr(0xE, page_fault as u64, 0, 0x8E);

    IDT_DESCRIPTOR.offset = &IDT as *const IdtGate as u64;
    asm!("lidt [{}]", in(reg) &IDT_DESCRIPTOR);
}

isr!(int3, {
    serial::print!("Breakpoint yeeee\n");
});

isr!(page_fault, {
    serial::print!("PAGE FAULT\n");
    unsafe {
        loop {
            asm!("hlt");
        }
    }
});
