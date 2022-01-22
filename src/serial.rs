use crate::arch::io::{inb, outb};
use core::fmt::Write;

const COM1: u16 = 0x3f8;

pub struct SerialWriter;

impl SerialWriter {
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

    fn is_transmit_empty() -> u8 {
        unsafe { inb(COM1 + 5) & 0x20 }
    }

    pub fn send_char(c: char) {
        while SerialWriter::is_transmit_empty() == 0 {}

        unsafe {
            outb(COM1, c as u8);
        }
    }

    pub fn print(msg: &str) {
        for c in msg.chars() {
            SerialWriter::send_char(c);
        }
    }
}

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        SerialWriter::print(s);
        Ok(())
    }
}

macro_rules! print {
    ($($arg:tt)*) => {
        {
            use crate::serial::SerialWriter;
            use core::fmt::Write;
            write!(&mut SerialWriter {}, $($arg)*).unwrap();
        }
    };
}

pub(crate) use print;
