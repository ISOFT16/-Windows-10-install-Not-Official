//! Низкоуровневые инструкции процессора: порты ввода-вывода, прерывания, CPUID.

use core::arch::asm;

#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}

#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}

/// Короткая пауза для медленных устройств (запись в неиспользуемый порт 0x80).
#[inline]
pub fn io_wait() {
    unsafe { outb(0x80, 0) };
}

#[inline]
pub fn rflags() -> u64 {
    let r: u64;
    unsafe { asm!("pushfq", "pop {}", out(reg) r, options(nomem, preserves_flags)) };
    r
}

#[inline]
pub fn interrupts_enabled() -> bool {
    rflags() & (1 << 9) != 0
}

#[inline]
pub fn disable_interrupts() {
    unsafe { asm!("cli", options(nomem, nostack)) };
}

/// # Safety
/// Обработчики прерываний (IDT) должны быть уже установлены.
#[inline]
pub unsafe fn enable_interrupts() {
    asm!("sti", options(nomem, nostack));
}

/// Атомарно включить прерывания и уснуть до следующего прерывания.
/// `sti` вступает в силу только после следующей инструкции, поэтому
/// прерывание между проверкой условия и `hlt` не может «потеряться».
#[inline]
pub unsafe fn enable_interrupts_and_halt() {
    asm!("sti", "hlt", options(nomem, nostack));
}

#[inline]
pub fn halt() {
    unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
}

/// Остановить процессор навсегда.
pub fn halt_forever() -> ! {
    loop {
        disable_interrupts();
        halt();
    }
}

#[inline]
pub fn read_cr2() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr2", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn read_cr3() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr3", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn read_cr0() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr0", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn read_cr4() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr4", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[derive(Clone, Copy, Debug)]
pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

pub fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    #[allow(unused_unsafe)]
    let r = unsafe { core::arch::x86_64::__cpuid_count(leaf, subleaf) };
    CpuidResult { eax: r.eax, ebx: r.ebx, ecx: r.ecx, edx: r.edx }
}

#[inline]
pub fn rdtsc() -> u64 {
    #[allow(unused_unsafe)]
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
}

/// Программная точка останова (исключение #BP, вектор 3).
#[inline]
pub fn breakpoint() {
    unsafe { asm!("int3", options(nomem, nostack)) };
}
