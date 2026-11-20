//! GDT и TSS. TSS задаёт отдельные стеки (IST) для двойной ошибки, NMI и
//! machine check: даже если основной стек ядра сломан или переполнен,
//! обработчик сможет выполниться и показать диагностику, а не перезагрузить
//! машину тройной ошибкой.

use core::arch::asm;
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut};

pub const KERNEL_CODE: u16 = 0x08;
pub const KERNEL_DATA: u16 = 0x10;
const TSS_SELECTOR: u16 = 0x18;

pub const IST_DOUBLE_FAULT: u8 = 1;
pub const IST_NMI_MCE: u8 = 2;

const IST_STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct Stack([u8; IST_STACK_SIZE]);

static mut DOUBLE_FAULT_STACK: Stack = Stack([0; IST_STACK_SIZE]);
static mut NMI_STACK: Stack = Stack([0; IST_STACK_SIZE]);

#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}

static mut TSS: Tss = Tss {
    reserved0: 0,
    rsp: [0; 3],
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    iomap_base: 0,
};

static mut GDT: [u64; 5] = [0; 5];

#[repr(C, packed)]
struct DescriptorPointer {
    limit: u16,
    base: u64,
}

fn stack_top(stack: *const Stack) -> u64 {
    stack as u64 + IST_STACK_SIZE as u64
}

pub fn init() {
    unsafe {
        let df_top = stack_top(addr_of!(DOUBLE_FAULT_STACK));
        let nmi_top = stack_top(addr_of!(NMI_STACK));
        addr_of_mut!(TSS).write(Tss {
            reserved0: 0,
            rsp: [0; 3],
            reserved1: 0,
            ist: [df_top, nmi_top, 0, 0, 0, 0, 0],
            reserved2: 0,
            reserved3: 0,
            iomap_base: size_of::<Tss>() as u16,
        });

        let base = addr_of!(TSS) as u64;
        let limit = (size_of::<Tss>() - 1) as u64;
        let tss_low = (limit & 0xFFFF)
            | ((base & 0xFF_FFFF) << 16)
            | (0x89 << 40) // присутствует, 64-битный свободный TSS
            | (((limit >> 16) & 0xF) << 48)
            | (((base >> 24) & 0xFF) << 56);
        let tss_high = base >> 32;

        addr_of_mut!(GDT).write([
            0,
            0x00AF_9A00_0000_FFFF, // код ядра, 64 бита
            0x00CF_9200_0000_FFFF, // данные ядра
            tss_low,
            tss_high,
        ]);

        let ptr = DescriptorPointer {
            limit: (size_of::<[u64; 5]>() - 1) as u16,
            base: addr_of!(GDT) as u64,
        };
        asm!("lgdt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));

        // перезагрузить CS дальним возвратом, затем сегменты данных
        asm!(
            "push {code}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            "mov {tmp:x}, {data}",
            "mov ds, {tmp:x}",
            "mov es, {tmp:x}",
            "mov ss, {tmp:x}",
            "xor {tmp:e}, {tmp:e}",
            "mov fs, {tmp:x}",
            "mov gs, {tmp:x}",
            code = const KERNEL_CODE as u64,
            data = const KERNEL_DATA as u64,
            tmp = out(reg) _,
        );

        asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nostack, preserves_flags));
    }
}

#[allow(unused_unsafe)]
pub fn ist_stack_ranges() -> [(u64, u64); 2] {
    let df = unsafe { addr_of!(DOUBLE_FAULT_STACK) } as u64;
    let nmi = unsafe { addr_of!(NMI_STACK) } as u64;
    [(df, df + IST_STACK_SIZE as u64), (nmi, nmi + IST_STACK_SIZE as u64)]
}
