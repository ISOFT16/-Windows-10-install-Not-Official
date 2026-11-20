//! Шина PCI: чтение конфигурационного пространства через порты 0xCF8/0xCFC.

use crate::cpu::{inl, outl};

pub fn read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | (bus as u32) << 16
        | (dev as u32) << 11
        | (func as u32) << 8
        | (offset as u32 & 0xFC);
    unsafe {
        outl(0xCF8, addr);
        inl(0xCFC)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Device {
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub bar0: u32,
}

/// Найти первое устройство, подходящее под условие (просматриваются шины 0..=7).
pub fn find(mut pred: impl FnMut(&Device) -> bool) -> Option<Device> {
    for bus in 0..8u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let id = read32(bus, dev, func, 0);
                let vendor = id as u16;
                if vendor == 0xFFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let class = read32(bus, dev, func, 8);
                let d = Device {
                    vendor,
                    device: (id >> 16) as u16,
                    class: (class >> 24) as u8,
                    bar0: read32(bus, dev, func, 0x10),
                };
                if pred(&d) {
                    return Some(d);
                }
                let header = (read32(bus, dev, func, 0x0C) >> 16) as u8;
                if func == 0 && header & 0x80 == 0 {
                    break; // не многофункциональное устройство
                }
            }
        }
    }
    None
}
