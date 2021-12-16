use crate::arch::x86_64::io::*;

const COM1: u16 = 0x3f8;

pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1 + 0, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xC7);
        outb(COM1 + 4, 0x0B);
    }
}

pub fn is_transmit_empty() -> u8 {
    unsafe { inb(COM1 + 5) & 0x20 }
}

pub fn send_char(c: char) {
    while is_transmit_empty() == 0 {}

    unsafe {
        outb(COM1, c as u8);
    }
}

pub fn print(msg: &str) {
    for c in msg.chars() {
        send_char(c);
    }
}
