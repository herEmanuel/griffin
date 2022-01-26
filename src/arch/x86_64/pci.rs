use super::io::{inl, outl};
use crate::arch::mm::pmm::PhysAddr;
use crate::drivers::ahci;
use crate::serial;
use alloc::vec::Vec;

const CONFIG_ADDR: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;
const MSI_CAPABILITY_ID: u8 = 0x5;

pub static mut PCI_DEVICES: Vec<PciDevice> = alloc::vec![];

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
    msi_offset: u8,
}

impl PciDevice {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        let device_vendor = read(bus, device, function, 0);
        let class = read(bus, device, function, 0x8);

        let mut device = PciDevice {
            bus,
            device,
            function,
            device_id: (device_vendor >> 16) as u16,
            vendor_id: device_vendor as u16,
            class: (class >> 24) as u8,
            subclass: (class >> 16) as u8,
            prog_if: (class >> 8) as u8,
            revision: class as u8,
            msi_offset: 0,
        };

        if device.has_capabilities() {
            let mut cap_offset = device.read(0x34) as u8;

            while cap_offset != 0 {
                let capability = device.read(cap_offset);
                if capability as u8 == MSI_CAPABILITY_ID {
                    device.msi_offset = cap_offset;
                    break;
                }

                // get the pointer to the next capability
                cap_offset = (capability >> 8) as u8;
            }
        }

        device
    }

    pub fn read(&self, offset: u8) -> u32 {
        read(self.bus, self.device, self.function, offset)
    }

    pub fn write(&self, data: u32, offset: u8) {
        write(data, self.bus, self.device, self.function, offset);
    }

    pub fn has_capabilities(&self) -> bool {
        (self.read(0x4) >> 16) & 1 << 4 != 0
    }

    pub fn get_bar(&self, bar_num: u8) -> PhysAddr {
        let offset = 0x10 + bar_num * 4;
        let bar = self.read(offset);

        if bar & 1 == 1 {
            // I/O space
            return PhysAddr::new((bar & !0b11) as u64);
        }

        if bar & 6 == 4 {
            // 64 bits bar
            return PhysAddr::new((bar & 0xfffffff0) as u64 | (self.read(offset + 4) as u64) << 32);
        }

        PhysAddr::new((bar & 0xfffffff0) as u64)
    }

    pub fn bus_master(&self) {
        let mut command_reg = self.read(0x4);
        command_reg |= 4;
        self.write(command_reg, 0x4);
    }

    pub fn enable_mmio(&self) {
        let mut command_reg = self.read(0x4);
        command_reg |= 2;
        self.write(command_reg, 0x4);
    }

    pub fn set_msi(&self, vector: usize) {
        if self.msi_offset == 0 {
            panic!("This device does not support MSIs");
        }

        let control = (self.read(self.msi_offset) >> 16) & 0xffff;

        let mut data_reg_offset = 0x8;
        if control & 1 << 7 != 0 {
            data_reg_offset = 0xc;
        }

        // destination is 0, use physical destination mode
        let msi_address: u32 = 0xfee00000 | 1 << 3;
        let msi_data =
            self.read(self.msi_offset + data_reg_offset) & 0xffff0000 | (vector & 0xff) as u32;

        self.write(msi_address, self.msi_offset + 0x4);
        self.write(msi_data, self.msi_offset + data_reg_offset);
        self.write((control | 1) << 16, self.msi_offset); // enable the MSI
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
            if dev.class == 0x1 && dev.subclass == 0x6 && dev.prog_if == 0x1 {
                // ahci controller
                ahci::init(dev);
            }
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
