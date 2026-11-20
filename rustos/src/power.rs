//! Перезагрузка и выключение.

use crate::cpu::{self, inb, outb, outl, outw};
use crate::interrupts;

pub fn reboot() -> ! {
    cpu::disable_interrupts();
    unsafe {
        // 1) импульс сброса через контроллер клавиатуры 8042
        for _ in 0..100_000 {
            if inb(0x64) & 0x02 == 0 {
                break;
            }
        }
        outb(0x64, 0xFE);
        for _ in 0..1_000_000 {
            core::hint::spin_loop();
        }
        // 2) сброс через порт 0xCF9 (чипсеты PCI)
        outb(0xCF9, 0x02);
        outb(0xCF9, 0x06);
        for _ in 0..1_000_000 {
            core::hint::spin_loop();
        }
        // 3) последнее средство: тройная ошибка
        interrupts::load_empty_idt();
        core::arch::asm!("int3", options(nomem, nostack));
    }
    cpu::halt_forever();
}

pub fn shutdown() -> ! {
    cpu::disable_interrupts();
    unsafe {
        outw(0x604, 0x2000); // QEMU (q35 / новые версии)
        outw(0xB004, 0x2000); // Bochs, старые QEMU
        outw(0x4004, 0x3400); // VirtualBox
    }
    crate::println!();
    crate::println!("ACPI power-off is not available here.");
    crate::println!("It is now safe to turn off your computer.");
    cpu::halt_forever();
}

/// Выход из QEMU с кодом (только если запущен с устройством isa-debug-exit).
pub fn qemu_exit(success: bool) {
    unsafe { outl(0xF4, if success { 0x10 } else { 0x11 }) };
}
