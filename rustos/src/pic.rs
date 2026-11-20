//! Контроллер прерываний 8259A (два каскадно соединённых PIC).

use crate::cpu::{inb, io_wait, outb};

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const EOI: u8 = 0x20;

pub const IRQ_BASE: u8 = 32;

pub const IRQ_TIMER: u8 = 0;
pub const IRQ_KEYBOARD: u8 = 1;
pub const IRQ_CASCADE: u8 = 2;
pub const IRQ_COM1: u8 = 4;

/// Перенести IRQ 0..15 на векторы 32..47 (векторы 0..31 заняты исключениями)
/// и замаскировать все линии, кроме нужных.
pub fn init(enabled: &[u8]) {
    unsafe {
        outb(PIC1_CMD, 0x11);
        io_wait();
        outb(PIC2_CMD, 0x11);
        io_wait();
        outb(PIC1_DATA, IRQ_BASE);
        io_wait();
        outb(PIC2_DATA, IRQ_BASE + 8);
        io_wait();
        outb(PIC1_DATA, 4);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        let mut mask: u16 = 0xFFFF;
        for &irq in enabled {
            mask &= !(1u16 << irq);
        }
        if mask & 0xFF00 != 0xFF00 {
            mask &= !(1u16 << IRQ_CASCADE);
        }
        outb(PIC1_DATA, (mask & 0xFF) as u8);
        outb(PIC2_DATA, (mask >> 8) as u8);
    }
}

fn read_isr() -> u16 {
    unsafe {
        outb(PIC1_CMD, 0x0B);
        outb(PIC2_CMD, 0x0B);
        ((inb(PIC2_CMD) as u16) << 8) | inb(PIC1_CMD) as u16
    }
}

/// IRQ 7 и 15 могут быть «ложными» (помеха на линии). Для них EOI
/// посылать нельзя (кроме EOI ведущему PIC для ложного IRQ 15).
pub fn handle_spurious(irq: u8) -> bool {
    if irq != 7 && irq != 15 {
        return false;
    }
    if read_isr() & (1 << irq) != 0 {
        return false;
    }
    if irq == 15 {
        unsafe { outb(PIC1_CMD, EOI) };
    }
    true
}

pub fn end_of_interrupt(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_CMD, EOI);
        }
        outb(PIC1_CMD, EOI);
    }
}
