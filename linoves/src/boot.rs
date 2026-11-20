//! Самое начало загрузки: Multiboot-заголовок и 32-битная заглушка,
//! которая проверяет процессор, строит таблицы страниц, включает
//! 64-битный режим (long mode) и вызывает `kernel_main`.
//!
//! Карта памяти после загрузки (тождественное отображение, адрес = физический):
//! * первые 4 МиБ отображены страницами по 4 КиБ;
//!   страница 0 НЕ отображена (разыменование нулевого указателя -> #PF),
//!   страница под стеком ядра НЕ отображена (переполнение стека -> #PF/#DF,
//!   а не тихая порча памяти);
//! * 4 МиБ .. 4 ГиБ отображены большими страницами по 2 МиБ.

// Флаги Multiboot: бит 0 — выравнивание модулей, бит 1 — карта памяти,
// бит 16 — адреса в заголовке. Для ISO (GRUB) дополнительно бит 2: попросить
// загрузчик включить графический режим 1024x768x32 (нужно на реальных ПК).
// Встроенный загрузчик QEMU (-kernel) бит 2 не поддерживает, поэтому
// обычная сборка его не ставит, а графику включает сама через Bochs VBE.
#[cfg(feature = "grub-video")]
core::arch::global_asm!(".set MB_FLAGS, 0x00010007");
#[cfg(not(feature = "grub-video"))]
core::arch::global_asm!(".set MB_FLAGS, 0x00010003");

core::arch::global_asm!(
    r#"
    .set MB_MAGIC, 0x1BADB002

    .section .multiboot, "a"
    .balign 4
multiboot_header:
    .long MB_MAGIC
    .long MB_FLAGS
    .long -(MB_MAGIC + MB_FLAGS)
    .long multiboot_header
    .long __kernel_start
    .long __load_end
    .long __bss_end
    .long _start
    .long 0                      /* тип видеорежима: графический */
    .long 1024                   /* ширина */
    .long 768                    /* высота */
    .long 32                     /* бит на пиксель */

    .section .boot.text, "ax"
    .code32
    .global _start
_start:
    cli
    cld
    movl %eax, %esi              /* магическое число Multiboot */
    movl %ebx, %ebp              /* адрес структуры Multiboot info */

    /* обнуляем .bss (таблицы страниц, стеки, статические переменные) */
    movl $__bss_start, %edi
    movl $__bss_end, %ecx
    subl %edi, %ecx
    shrl $2, %ecx
    xorl %eax, %eax
    rep stosl

    movl $boot_stack_top, %esp

    /* есть ли инструкция CPUID? (бит ID в EFLAGS) */
    pushfl
    popl %eax
    movl %eax, %ecx
    xorl $(1 << 21), %eax
    pushl %eax
    popfl
    pushfl
    popl %eax
    pushl %ecx
    popfl
    cmpl %eax, %ecx
    je .Lno_long_mode

    /* поддерживает ли процессор 64-битный режим? */
    movl $0x80000000, %eax
    cpuid
    cmpl $0x80000001, %eax
    jb .Lno_long_mode
    movl $0x80000001, %eax
    cpuid
    testl $(1 << 29), %edx
    jz .Lno_long_mode

    /* PML4[0] -> PDPT */
    movl $boot_pdpt, %eax
    orl $3, %eax
    movl %eax, boot_pml4

    /* PDPT[0..4] -> 4 каталога страниц (4 ГиБ) */
    movl $boot_pdpt, %edi
    movl $boot_pd, %eax
    orl $3, %eax
    movl $4, %ecx
1:
    movl %eax, (%edi)
    addl $4096, %eax
    addl $8, %edi
    loop 1b

    /* 2048 больших страниц по 2 МиБ: present | writable | huge */
    movl $boot_pd, %edi
    movl $0x83, %eax
    movl $2048, %ecx
2:
    movl %eax, (%edi)
    movl $0, 4(%edi)
    addl $0x200000, %eax
    addl $8, %edi
    loop 2b

    /* первые 4 МиБ: две таблицы страниц по 4 КиБ */
    movl $boot_pt, %eax
    orl $3, %eax
    movl %eax, boot_pd
    addl $4096, %eax
    movl %eax, boot_pd + 8

    movl $boot_pt, %edi
    movl $3, %eax
    movl $1024, %ecx
3:
    movl %eax, (%edi)
    addl $4096, %eax
    addl $8, %edi
    loop 3b

    /* защитные страницы: нулевая и страница под стеком */
    movl $0, boot_pt
    movl $boot_stack_guard, %eax
    shrl $12, %eax
    movl $0, boot_pt(, %eax, 8)

    /* включаем PAE, long mode, страничную адресацию и защиту от записи */
    movl $boot_pml4, %eax
    movl %eax, %cr3
    movl %cr4, %eax
    orl $(1 << 5), %eax
    movl %eax, %cr4
    movl $0xC0000080, %ecx
    rdmsr
    orl $(1 << 8), %eax
    wrmsr
    movl %cr0, %eax
    orl $((1 << 31) | (1 << 16) | 1), %eax
    movl %eax, %cr0

    lgdt boot_gdt_ptr
    ljmp $0x08, $long_mode_start

.Lno_long_mode:
    /* сообщение прямо в видеопамять: белым по красному */
    movl $0xB8000, %edi
    movl $boot_err_msg, %esi
    movb $0x4F, %ah
4:
    lodsb
    testb %al, %al
    jz 5f
    stosw
    jmp 4b
5:
    cli
    hlt
    jmp 5b

    .code64
long_mode_start:
    xorl %eax, %eax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs
    leaq boot_stack_top(%rip), %rsp
    movl %esi, %edi              /* 1-й аргумент: magic */
    movl %ebp, %esi              /* 2-й аргумент: адрес Multiboot info */
    xorl %ebp, %ebp
    call kernel_main
6:
    cli
    hlt
    jmp 6b

    .section .rodata
    .balign 16
boot_gdt:
    .quad 0
    .quad 0x00AF9A000000FFFF     /* 64-битный сегмент кода */
    .quad 0x00CF92000000FFFF     /* сегмент данных */
boot_gdt_end:
boot_gdt_ptr:
    .word boot_gdt_end - boot_gdt - 1
    .long boot_gdt
    .long 0
boot_err_msg:
    .asciz "Linoves: ERROR - this CPU does not support 64-bit (x86_64) mode. System halted."

    .section .bss.boot, "aw", @nobits
    .balign 4096
boot_pml4:
    .skip 4096
boot_pdpt:
    .skip 4096
boot_pd:
    .skip 4096 * 4
boot_pt:
    .skip 4096 * 2
    .global boot_stack_guard
boot_stack_guard:
    .skip 4096
    .global boot_stack_bottom
boot_stack_bottom:
    .skip 128 * 1024
    .global boot_stack_top
boot_stack_top:
"#,
    options(att_syntax)
);

extern "C" {
    static boot_stack_guard: u8;
    static boot_stack_bottom: u8;
    static boot_stack_top: u8;
    static __kernel_start: u8;
    static __kernel_end: u8;
}

/// Адрес защитной (неотображённой) страницы под стеком ядра.
#[allow(unused_unsafe)]
pub fn stack_guard_page() -> u64 {
    unsafe { core::ptr::addr_of!(boot_stack_guard) as u64 }
}

#[allow(unused_unsafe)]
pub fn stack_bounds() -> (u64, u64) {
    unsafe {
        (
            core::ptr::addr_of!(boot_stack_bottom) as u64,
            core::ptr::addr_of!(boot_stack_top) as u64,
        )
    }
}

#[allow(unused_unsafe)]
pub fn kernel_bounds() -> (u64, u64) {
    unsafe {
        (
            core::ptr::addr_of!(__kernel_start) as u64,
            core::ptr::addr_of!(__kernel_end) as u64,
        )
    }
}
