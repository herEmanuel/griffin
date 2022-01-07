use super::io::{inl, outl};
use crate::drivers::ahci;
use crate::serial;
use alloc::vec::Vec;

const CONFIG_ADDR: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

pub static mut PCI_DEVICES: Vec<PciDevice> = alloc::vec![];

// we dont care about the other formats
#[repr(C, packed)]
struct ConfigurationSpace {
    vendor: u16,
    device: u16,
    command: u16,
    status: u16,
    revision: u8,
    prog_if: u8,
    subclass: u8,
    class: u8,
    hdr_type: u32, // not only it
    bars: [u32; 6],
    ccisptr: u32,
    subsystem_info: u32,
    erba: u32,
    capabilities_ptr: u8,
    reserved: [u8; 7],
    interrupt_line: u8,
    interrupt_pin: u8,
    min_grant: u8,
    max_latency: u8,
}

#[derive(Debug)]
pub struct PciDevice {
    bus: u8,
    device: u8,
    function: u8,
    device_id: u16,
    vendor_id: u16,
    class: u8,
    subclass: u8,
    prog_if: u8,
    revision: u8,
}

impl PciDevice {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        let device_vendor = read(bus, device, function, 0);
        let class = read(bus, device, function, 0x8);

        PciDevice {
            bus,
            device,
            function,
            device_id: (device_vendor >> 16) as u16,
            vendor_id: device_vendor as u16,
            class: (class >> 24) as u8,
            subclass: (class >> 16) as u8,
            prog_if: (class >> 8) as u8,
            revision: class as u8,
        }
    }

    pub fn get_bar(&self, bar_num: u8) -> u64 {
        let offset = 0x10 + bar_num * 4;
        let bar = read(self.bus, self.device, self.function, offset);

        if bar & 1 == 1 {
            // I/O space
            return (bar & !0b11) as u64;
        }

        if bar & 6 == 4 {
            // 64 bits bar
            return (bar & 0xfffffff0) as u64
                | (read(self.bus, self.device, self.function, offset + 4) as u64) << 32;
        }

        (bar & 0xfffffff0) as u64
    }

    pub fn bus_master(&self) {
        let mut command_reg = read(self.bus, self.device, self.function, 0x4);
        command_reg |= 4;
        write(command_reg, self.bus, self.device, self.function, 0x4);
    }

    pub fn enable_mmio(&self) {
        let mut command_reg = read(self.bus, self.device, self.function, 0x4);
        command_reg |= 2;
        write(command_reg, self.bus, self.device, self.function, 0x4);
    }
}

fn get_header_type(bus: u8, device: u8, function: u8) -> u8 {
    let res = read(bus, device, function, 0xc);
    (res >> 16) as u8
}

// good old bruteforce
pub fn enumerate_devices() {
    for bus in 0..=255 {
        for device in 0..=31 {
            for function in 0..=7 {
                let cnfg = read(bus, device, function, 0);
                if cnfg == u32::MAX {
                    continue;
                }

                unsafe {
                    PCI_DEVICES.push(PciDevice::new(bus, device, function));
                }
            }
        }
    }

    unsafe {
        for dev in PCI_DEVICES.iter() {
            // serial::print!("Found PCI device: {:?}\n", dev);

            if dev.class == 0x1 && dev.subclass == 0x6 && dev.prog_if == 0x1 {
                // ahci controller
                ahci::init(dev);
            }
        }
    }

    serial::print!("yes?\n");
}

pub fn read(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x80000000
        | (bus as u32) << 16
        | (device as u32 & 0x1f) << 11
        | (function as u32 & 0x7) << 8
        | offset as u32 & 0xfc;

    unsafe {
        outl(CONFIG_ADDR, address);
        inl(CONFIG_DATA)
    }
}

pub fn write(data: u32, bus: u8, device: u8, function: u8, offset: u8) {
    let address = 0x80000000
        | (bus as u32) << 16
        | (device as u32 & 0x1f) << 11
        | (function as u32 & 0x7) << 8
        | offset as u32 & 0xfc;

    unsafe {
        outl(CONFIG_ADDR, address);
        outl(CONFIG_DATA, data);
    }
}
