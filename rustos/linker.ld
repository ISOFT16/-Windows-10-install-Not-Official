/* RustOS linker script: everything lives in one loadable segment at 1 MiB,
   so the Multiboot "a.out kludge" header can describe the image exactly. */
ENTRY(_start)

PHDRS
{
    image PT_LOAD FLAGS(7);
}

SECTIONS
{
    . = 0x100000;
    __kernel_start = .;

    .text : ALIGN(16)
    {
        KEEP(*(.multiboot))
        *(.boot.text)
        *(.text .text.*)
    } :image

    .rodata : ALIGN(16)
    {
        *(.rodata .rodata.*)
        *(.data.rel.ro .data.rel.ro.*)
        *(.got .got.*)
    } :image

    .data : ALIGN(16)
    {
        *(.data .data.*)
    } :image

    . = ALIGN(16);
    __load_end = .;

    .bss : ALIGN(4096)
    {
        __bss_start = .;
        *(.bss.boot)
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4096);
        __bss_end = .;
    } :image

    __kernel_end = .;

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.note .note.*)
        *(.comment)
    }

    ASSERT(boot_stack_top <= 0x400000, "kernel too large: boot stack must stay inside the first 4 MiB")
}
