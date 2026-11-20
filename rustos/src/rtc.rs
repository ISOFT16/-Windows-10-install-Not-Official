//! Часы реального времени (CMOS RTC): текущие дата и время.

use crate::cpu::{inb, outb};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

fn read(reg: u8) -> u8 {
    unsafe {
        outb(0x70, reg | 0x80); // бит 7: не трогать NMI
        inb(0x71)
    }
}

fn updating() -> bool {
    read(0x0A) & 0x80 != 0
}

fn raw() -> [u8; 6] {
    let mut spins = 0;
    while updating() && spins < 100_000 {
        spins += 1;
        core::hint::spin_loop();
    }
    [read(0x00), read(0x02), read(0x04), read(0x07), read(0x08), read(0x09)]
}

fn bcd(v: u8) -> u8 {
    (v & 0x0F) + (v >> 4) * 10
}

pub fn now() -> DateTime {
    // читаем, пока два подряд чтения не совпадут (часы могли обновиться посередине)
    let mut a = raw();
    for _ in 0..5 {
        let b = raw();
        if a == b {
            break;
        }
        a = b;
    }
    let status_b = read(0x0B);
    let binary = status_b & 0x04 != 0;
    let h24 = status_b & 0x02 != 0;
    let conv = |v: u8| if binary { v } else { bcd(v) };
    let pm = a[2] & 0x80 != 0;
    let mut hour = conv(a[2] & 0x7F);
    if !h24 {
        hour %= 12;
        if pm {
            hour += 12;
        }
    }
    DateTime {
        second: conv(a[0]).min(59),
        minute: conv(a[1]).min(59),
        hour: hour.min(23),
        day: conv(a[3]).clamp(1, 31),
        month: conv(a[4]).clamp(1, 12),
        year: 2000 + conv(a[5]) as u16,
    }
}
