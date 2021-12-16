pub unsafe fn outb(port: u16, byte: u8) {
    asm!("out dx, al", in("dx") port, in("al") byte);
}

pub unsafe fn outw(port: u16, word: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") word);
}

pub unsafe fn outl(port: u16, dword: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") dword);
}

pub unsafe fn inb(port: u16) -> u8 {
    let mut val: u8;
    asm!("in al, dx", out("al") val, in("dx") port);
    val as u8
}

pub unsafe fn inw(port: u16) -> u16 {
    let mut val: u16;
    asm!("in ax, dx", out("ax") val, in("dx") port);
    val
}

pub unsafe fn inl(port: u16) -> u32 {
    let mut val: u32;
    asm!("in eax, dx", out("eax") val, in("dx") port);
    val
}
