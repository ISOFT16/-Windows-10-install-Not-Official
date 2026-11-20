//! Таблица прерываний (IDT), ассемблерные «входы» для всех 256 векторов и
//! диспетчер на Rust.
//!
//! Каждый вектор имеет обработчик, поэтому неожиданное прерывание никогда не
//! приводит к тройной ошибке и перезагрузке: неизвестные векторы просто
//! игнорируются, а исключения процессора выводят подробный отчёт.

use core::arch::asm;
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::gdt;
use crate::pic;
use crate::{fatal, keyboard, serial, time};

// Каждый вход занимает ровно 16 байт: адрес входа = isr_stubs + 16 * вектор.
// Для векторов без кода ошибки кладём фиктивный 0, чтобы кадр стека был
// одинаковым для всех.
core::arch::global_asm!(
    r#"
    .section .text
    .balign 16
    .global isr_stubs
isr_stubs:
    .set vec, 0
    .rept 256
        .balign 16
        .if (vec == 8) || (vec == 10) || (vec == 11) || (vec == 12) || (vec == 13) || (vec == 14) || (vec == 17) || (vec == 21) || (vec == 29) || (vec == 30)
        .else
            pushq $0
        .endif
        pushq $vec
        jmp isr_common
        .set vec, vec + 1
    .endr

    .balign 16
isr_common:
    pushq %rax
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rsi
    pushq %rdi
    pushq %rbp
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    movq %rsp, %rdi
    cld
    call interrupt_dispatch
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r11
    popq %r10
    popq %r9
    popq %r8
    popq %rbp
    popq %rdi
    popq %rsi
    popq %rdx
    popq %rcx
    popq %rbx
    popq %rax
    addq $16, %rsp
    iretq
"#,
    options(att_syntax)
);

extern "C" {
    static isr_stubs: u8;
}

/// Состояние процессора в момент прерывания (в порядке на стеке).
#[repr(C)]
#[derive(Debug)]
pub struct InterruptFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const EMPTY: IdtEntry = IdtEntry {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_mid: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn new(handler: u64, ist: u8) -> Self {
        IdtEntry {
            offset_low: handler as u16,
            selector: gdt::KERNEL_CODE,
            ist,
            attributes: 0x8E, // присутствует, DPL 0, шлюз прерывания (IF сбрасывается)
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::EMPTY; 256];

#[repr(C, packed)]
struct DescriptorPointer {
    limit: u16,
    base: u64,
}

static BREAKPOINTS: AtomicU64 = AtomicU64::new(0);
static SPURIOUS: AtomicU64 = AtomicU64::new(0);
static UNKNOWN: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    unsafe {
        let base = addr_of!(isr_stubs) as u64;
        let idt = &mut *addr_of_mut!(IDT);
        for (vector, entry) in idt.iter_mut().enumerate() {
            let ist = match vector {
                8 => gdt::IST_DOUBLE_FAULT,
                2 | 18 => gdt::IST_NMI_MCE,
                _ => 0,
            };
            *entry = IdtEntry::new(base + 16 * vector as u64, ist);
        }
        let ptr = DescriptorPointer {
            limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: addr_of!(IDT) as u64,
        };
        asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
    }
}

/// Загрузить пустую IDT (используется как последнее средство перезагрузки).
pub unsafe fn load_empty_idt() {
    let ptr = DescriptorPointer { limit: 0, base: 0 };
    asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
}

pub fn breakpoint_count() -> u64 {
    BREAKPOINTS.load(Ordering::Relaxed)
}

pub fn spurious_count() -> u64 {
    SPURIOUS.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn unknown_count() -> u64 {
    UNKNOWN.load(Ordering::Relaxed)
}

#[no_mangle]
extern "C" fn interrupt_dispatch(frame: *mut InterruptFrame) {
    let frame = unsafe { &mut *frame };
    match frame.vector {
        1 => {} // отладочное исключение: продолжаем
        3 => {
            BREAKPOINTS.fetch_add(1, Ordering::Relaxed);
        }
        0..=31 => fatal::exception(frame),
        32..=47 => handle_irq((frame.vector - 32) as u8),
        _ => {
            UNKNOWN.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn handle_irq(irq: u8) {
    if pic::handle_spurious(irq) {
        SPURIOUS.fetch_add(1, Ordering::Relaxed);
        return;
    }
    match irq {
        pic::IRQ_TIMER => time::tick(),
        pic::IRQ_KEYBOARD => keyboard::irq(),
        pic::IRQ_COM1 => serial::irq(),
        _ => {}
    }
    pic::end_of_interrupt(irq);
}

pub const EXCEPTION_NAMES: [&str; 32] = [
    "Divide Error (#DE)",
    "Debug (#DB)",
    "Non-Maskable Interrupt (NMI)",
    "Breakpoint (#BP)",
    "Overflow (#OF)",
    "Bound Range Exceeded (#BR)",
    "Invalid Opcode (#UD)",
    "Device Not Available (#NM)",
    "Double Fault (#DF)",
    "Coprocessor Segment Overrun",
    "Invalid TSS (#TS)",
    "Segment Not Present (#NP)",
    "Stack-Segment Fault (#SS)",
    "General Protection Fault (#GP)",
    "Page Fault (#PF)",
    "Reserved (15)",
    "x87 Floating-Point Error (#MF)",
    "Alignment Check (#AC)",
    "Machine Check (#MC)",
    "SIMD Floating-Point (#XM)",
    "Virtualization (#VE)",
    "Control Protection (#CP)",
    "Reserved (22)",
    "Reserved (23)",
    "Reserved (24)",
    "Reserved (25)",
    "Reserved (26)",
    "Reserved (27)",
    "Hypervisor Injection (#HV)",
    "VMM Communication (#VC)",
    "Security Exception (#SX)",
    "Reserved (31)",
];
