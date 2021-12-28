use super::io::{inl, outl};
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
    useless: u32,
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
    device_id: u16,
    vendor_id: u16,
    class: u8,
    subclass: u8,
    prog_if: u8,
    revision: u8,
    bars: [u32; 6],
}

impl PciDevice {
    pub fn new(
        dev_id: u16,
        vend_id: u16,
        class: u8,
        subclass: u8,
        prog: u8,
        revision: u8,
        bars: [u32; 6],
    ) -> Self {
        PciDevice {
            device_id: dev_id,
            vendor_id: vend_id,
            class,
            subclass,
            prog_if: prog,
            revision,
            bars,
        }
    }
}

pub fn enumerate_devices() {
    for bus in 0..=255 {
        for device in 0..=31 {
            for function in 0..=7 {
                let cnfg = read(bus, device, function, 0);
                if cnfg != u32::MAX {
                    serial::print!(
                        "Found device at bus {}, device {} and function {}\n",
                        bus,
                        device,
                        function
                    );
                    let device = PciDevice::new(45, 65, 43, 34, 54, 54, [34, 54, 343, 23, 54, 65]);
                    unsafe {
                        PCI_DEVICES.push(device);
                    }
                }
            }
        }
    }

    unsafe {
        for device in PCI_DEVICES.iter() {
            serial::print!("device: {:?}\n", device);
        }
    }
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
