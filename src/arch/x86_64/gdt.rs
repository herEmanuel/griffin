use core::arch::asm;

#[repr(C, packed)]
struct GdtDescriptor {
    limit: u16,
    offset: u64,
}

#[repr(C, packed)]
struct GdtEntry {
    limit1: u16,
    base1: u16,
    base2: u8,
    access: u8,
    flags: u8,
    base3: u8,
}

#[repr(C, packed)]
struct TssEntry {
    limit: u16,
    base1: u16,
    base2: u8,
    flags1: u8,
    flags2: u8,
    base3: u8,
    base4: u32,
    reserved: u32,
}

#[repr(C, packed)]
struct Gdt {
    null: GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
    user_code: GdtEntry,
    user_data: GdtEntry,
    tss: TssEntry,
}

impl GdtEntry {
    const fn new(access: u8, flags: u8) -> Self {
        GdtEntry {
            limit1: 0,
            base1: 0,
            base2: 0,
            access,
            flags,
            base3: 0,
        }
    }
}

impl TssEntry {
    const fn new(limit: u16, flags1: u8) -> Self {
        TssEntry {
            limit,
            base1: 0,
            base2: 0,
            flags1,
            flags2: 0,
            base3: 0,
            base4: 0,
            reserved: 0,
        }
    }

    fn set_base(&mut self, base: u64) {
        self.base1 = base as u16;
        self.base2 = (base >> 16) as u8;
        self.base3 = (base >> 24) as u8;
        self.base4 = (base >> 32) as u32;
    }
}

static mut GDT: Gdt = Gdt {
    null: GdtEntry::new(0, 0),
    kernel_code: GdtEntry::new(0x9A, 0x20),
    kernel_data: GdtEntry::new(0x92, 0),
    user_code: GdtEntry::new(0xFA, 0x20),
    user_data: GdtEntry::new(0xF2, 0),
    tss: TssEntry::new(104, 0x89),
};

static mut GDT_DESCRIPTOR: GdtDescriptor = GdtDescriptor {
    limit: 55, // yes, I hardcoded the limit. Get over it.
    offset: 0,
};

pub unsafe fn init() {
    GDT_DESCRIPTOR.offset = &GDT as *const Gdt as u64;

    asm!(
        "lgdt [{descriptor}]",
        "mov ax, 0x10",
        "mov ds, ax",
        "mov gs, ax",
        "mov fs, ax",
        "mov es, ax",
        "mov ss, ax",
        "lea {tmp}, [1f + rip]",
        "push 0x8",
        "push {tmp}",
        "retfq",
        "1:",
        descriptor = in(reg) &GDT_DESCRIPTOR,
        tmp = out(reg) _
    );
}

pub unsafe fn load_tss(tss_addr: u64) {
    let tss_selector = 0x28;
    GDT.tss.set_base(tss_addr);
    asm!("ltr {:x}", in(reg) tss_selector);
}
