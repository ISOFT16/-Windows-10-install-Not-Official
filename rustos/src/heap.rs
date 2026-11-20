//! Куча ядра: распределитель «первый подходящий» со списком свободных блоков,
//! отсортированным по адресу, и слиянием соседних блоков при освобождении.
//!
//! Защитные проверки: освобождение указателя вне кучи, двойное освобождение и
//! перекрытие блоков сразу останавливают систему с понятным сообщением,
//! вместо того чтобы незаметно испортить память. Команда `selftest`
//! дополнительно проверяет целостность всего списка.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

use crate::sync::IrqMutex;
use crate::{boot, multiboot};

const BLOCK_ALIGN: usize = 16;
const MAX_HEAP: u64 = 256 * 1024 * 1024;
const MIN_HEAP: u64 = 1024 * 1024;
const FOUR_GIB: u64 = 1 << 32;

#[repr(C)]
struct Node {
    size: usize,
    next: *mut Node,
}

pub struct Heap {
    head: *mut Node,
    start: usize,
    end: usize,
    used: usize,
    allocations: u64,
    frees: u64,
    failures: u64,
}

unsafe impl Send for Heap {}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub start: usize,
    pub size: usize,
    pub used: usize,
    pub free: usize,
    pub free_blocks: usize,
    pub largest_free: usize,
    pub allocations: u64,
    pub frees: u64,
    pub failures: u64,
}

fn align_up(value: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two());
    value.checked_add(align - 1).map(|v| v & !(align - 1))
}

fn block_size(layout: &Layout) -> Option<usize> {
    align_up(layout.size().max(BLOCK_ALIGN), BLOCK_ALIGN)
}

impl Heap {
    const fn empty() -> Self {
        Heap {
            head: null_mut(),
            start: 0,
            end: 0,
            used: 0,
            allocations: 0,
            frees: 0,
            failures: 0,
        }
    }

    unsafe fn init(&mut self, start: usize, size: usize) {
        let start = align_up(start, BLOCK_ALIGN).expect("heap start overflow");
        let size = size & !(BLOCK_ALIGN - 1);
        let node = start as *mut Node;
        node.write(Node { size, next: null_mut() });
        self.head = node;
        self.start = start;
        self.end = start + size;
        self.used = 0;
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = match block_size(&layout) {
            Some(s) => s,
            None => return null_mut(),
        };
        let align = layout.align().max(BLOCK_ALIGN);

        let mut prev: *mut *mut Node = &mut self.head;
        while !(*prev).is_null() {
            let node = *prev;
            let node_start = node as usize;
            let node_end = node_start + (*node).size;
            if let Some(alloc_start) = align_up(node_start, align) {
                if let Some(alloc_end) = alloc_start.checked_add(size) {
                    if alloc_end <= node_end {
                        // Остатки спереди и сзади кратны 16 => либо 0, либо
                        // достаточно велики для заголовка свободного блока.
                        let mut link = (*node).next;
                        if node_end > alloc_end {
                            let back = alloc_end as *mut Node;
                            back.write(Node { size: node_end - alloc_end, next: link });
                            link = back;
                        }
                        if alloc_start > node_start {
                            (*node).size = alloc_start - node_start;
                            (*node).next = link;
                        } else {
                            *prev = link;
                        }
                        self.used += size;
                        self.allocations += 1;
                        return alloc_start as *mut u8;
                    }
                }
            }
            prev = &mut (*node).next;
        }
        self.failures += 1;
        null_mut()
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let addr = ptr as usize;
        let size = block_size(&layout).expect("heap: invalid layout in dealloc");
        if addr < self.start || addr.checked_add(size).map_or(true, |e| e > self.end) {
            panic!("heap: free of pointer {:#x} outside the heap", addr);
        }
        if addr % BLOCK_ALIGN != 0 {
            panic!("heap: free of misaligned pointer {:#x}", addr);
        }

        let mut prev: *mut Node = null_mut();
        let mut cur = self.head;
        while !cur.is_null() && (cur as usize) < addr {
            prev = cur;
            cur = (*cur).next;
        }
        if !cur.is_null() && addr + size > cur as usize {
            panic!("heap: double free or corruption at {:#x}", addr);
        }
        if !prev.is_null() && prev as usize + (*prev).size > addr {
            panic!("heap: double free or corruption at {:#x}", addr);
        }

        let node = addr as *mut Node;
        node.write(Node { size, next: cur });
        if prev.is_null() {
            self.head = node;
        } else {
            (*prev).next = node;
        }
        if !cur.is_null() && addr + size == cur as usize {
            (*node).size += (*cur).size;
            (*node).next = (*cur).next;
        }
        if !prev.is_null() && prev as usize + (*prev).size == addr {
            (*prev).size += (*node).size;
            (*prev).next = (*node).next;
        }
        self.used -= size;
        self.frees += 1;
    }

    fn stats(&self) -> Stats {
        let mut s = Stats {
            start: self.start,
            size: self.end - self.start,
            used: self.used,
            allocations: self.allocations,
            frees: self.frees,
            failures: self.failures,
            ..Stats::default()
        };
        let mut cur = self.head;
        while !cur.is_null() {
            let size = unsafe { (*cur).size };
            s.free += size;
            s.free_blocks += 1;
            s.largest_free = s.largest_free.max(size);
            cur = unsafe { (*cur).next };
        }
        s
    }

    fn check(&self) -> Result<(), &'static str> {
        if self.start == 0 {
            return Err("heap is not initialised");
        }
        let max_nodes = (self.end - self.start) / BLOCK_ALIGN + 1;
        let mut count = 0;
        let mut free = 0usize;
        let mut last_end = 0usize;
        let mut cur = self.head;
        while !cur.is_null() {
            count += 1;
            if count > max_nodes {
                return Err("free list contains a cycle");
            }
            let a = cur as usize;
            if a < self.start || a >= self.end || a % BLOCK_ALIGN != 0 {
                return Err("free block outside heap or misaligned");
            }
            let size = unsafe { (*cur).size };
            if size < BLOCK_ALIGN || size % BLOCK_ALIGN != 0 || a + size > self.end {
                return Err("free block has an invalid size");
            }
            if last_end != 0 && a <= last_end {
                return Err("free blocks overlap, are unsorted or were not merged");
            }
            last_end = a + size;
            free += size;
            cur = unsafe { (*cur).next };
        }
        if free + self.used != self.end - self.start {
            return Err("free + used does not equal heap size (leak or corruption)");
        }
        Ok(())
    }
}

pub struct LockedHeap(IrqMutex<Heap>);

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0.lock().dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap(IrqMutex::new(Heap::empty()));

/// Выбрать для кучи самый большой свободный участок памяти выше ядра.
pub fn init_from_boot_info() -> Result<Stats, &'static str> {
    let (_, kernel_end) = boot::kernel_bounds();
    let min_start = (kernel_end.max(0x10_0000) + 0xFFF) & !0xFFF;

    let mut best: (u64, u64) = (0, 0);
    multiboot::with(|info| {
        for r in info.regions() {
            if r.kind != 1 {
                continue;
            }
            let start = r.base.max(min_start);
            let end = r.base.saturating_add(r.len).min(FOUR_GIB);
            if end > start && end - start > best.1 - best.0 {
                best = (start, end);
            }
        }
        if best.1 == 0 {
            if let Some(upper) = info.mem_upper_kb {
                let end = (0x10_0000 + upper as u64 * 1024).min(FOUR_GIB);
                if end > min_start {
                    best = (min_start, end);
                }
            }
        }
    });

    let start = (best.0 + 0xFFF) & !0xFFF;
    if best.1 <= start {
        return Err("no usable memory found in the boot memory map");
    }
    let size = (best.1 - start).min(MAX_HEAP) & !0xFFF;
    if size < MIN_HEAP {
        return Err("not enough memory for the kernel heap (need at least 1 MiB)");
    }
    unsafe { ALLOCATOR.0.lock().init(start as usize, size as usize) };
    Ok(stats())
}

pub fn stats() -> Stats {
    ALLOCATOR.0.lock().stats()
}

pub fn check_integrity() -> Result<(), &'static str> {
    ALLOCATOR.0.lock().check()
}
