//! Разбор информации от загрузчика (Multiboot 1). Всё нужное копируется в
//! статическую память ядра сразу при загрузке, поэтому дальше область, где
//! лежала исходная структура, можно спокойно отдавать под кучу.

use crate::sync::IrqMutex;

pub const BOOTLOADER_MAGIC: u32 = 0x2BAD_B002;
const MAX_REGIONS: usize = 64;
const MAX_CMDLINE: usize = 256;
const MAX_LOADER_NAME: usize = 64;
const FOUR_GIB: u64 = 1 << 32;

#[derive(Clone, Copy, Debug)]
pub struct MemRegion {
    pub base: u64,
    pub len: u64,
    pub kind: u32,
}

impl MemRegion {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            1 => "usable",
            2 => "reserved",
            3 => "ACPI reclaimable",
            4 => "ACPI NVS",
            5 => "bad memory",
            _ => "unknown",
        }
    }
}

/// Графический буфер, который настроил загрузчик (GRUB).
#[derive(Clone, Copy, Debug)]
pub struct Framebuffer {
    pub addr: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub kind: u8,
    pub red_pos: u8,
    pub green_pos: u8,
    pub blue_pos: u8,
}

pub struct BootInfo {
    pub valid: bool,
    pub framebuffer: Option<Framebuffer>,
    pub mem_lower_kb: Option<u32>,
    pub mem_upper_kb: Option<u32>,
    regions: [MemRegion; MAX_REGIONS],
    region_count: usize,
    cmdline: [u8; MAX_CMDLINE],
    cmdline_len: usize,
    loader: [u8; MAX_LOADER_NAME],
    loader_len: usize,
}

impl BootInfo {
    pub fn regions(&self) -> &[MemRegion] {
        &self.regions[..self.region_count]
    }

    pub fn cmdline(&self) -> &str {
        core::str::from_utf8(&self.cmdline[..self.cmdline_len]).unwrap_or("")
    }

    pub fn loader_name(&self) -> &str {
        core::str::from_utf8(&self.loader[..self.loader_len]).unwrap_or("")
    }

    pub fn usable_bytes(&self) -> u64 {
        self.regions()
            .iter()
            .filter(|r| r.kind == 1)
            .fold(0u64, |acc, r| acc.saturating_add(r.len))
    }
}

static INFO: IrqMutex<BootInfo> = IrqMutex::new(BootInfo {
    valid: false,
    framebuffer: None,
    mem_lower_kb: None,
    mem_upper_kb: None,
    regions: [MemRegion { base: 0, len: 0, kind: 0 }; MAX_REGIONS],
    region_count: 0,
    cmdline: [0; MAX_CMDLINE],
    cmdline_len: 0,
    loader: [0; MAX_LOADER_NAME],
    loader_len: 0,
});

/// Адрес считается допустимым, если он вне нулевой страницы и ниже 4 ГиБ
/// (вся эта память отображена).
fn valid_ptr(addr: u64, len: u64) -> bool {
    addr >= 0x1000 && addr.checked_add(len).map_or(false, |end| end <= FOUR_GIB)
}

unsafe fn read_u32(addr: u64) -> u32 {
    (addr as *const u32).read_unaligned()
}

unsafe fn read_u64(addr: u64) -> u64 {
    (addr as *const u64).read_unaligned()
}

unsafe fn copy_cstr(addr: u64, out: &mut [u8]) -> usize {
    let mut n = 0;
    while n < out.len() && valid_ptr(addr + n as u64, 1) {
        let b = *((addr + n as u64) as *const u8);
        if b == 0 {
            break;
        }
        out[n] = if b.is_ascii() { b } else { b'?' };
        n += 1;
    }
    n
}

pub fn parse(magic: u32, mbi: u32) -> bool {
    let mut info = INFO.lock();
    let mbi = mbi as u64;
    if magic != BOOTLOADER_MAGIC || !valid_ptr(mbi, 88) {
        return false;
    }
    unsafe {
        let flags = read_u32(mbi);
        if flags & (1 << 0) != 0 {
            info.mem_lower_kb = Some(read_u32(mbi + 4));
            info.mem_upper_kb = Some(read_u32(mbi + 8));
        }
        if flags & (1 << 2) != 0 {
            let addr = read_u32(mbi + 16) as u64;
            let BootInfo { cmdline, cmdline_len, .. } = &mut *info;
            *cmdline_len = copy_cstr(addr, cmdline);
        }
        if flags & (1 << 6) != 0 {
            let len = read_u32(mbi + 44) as u64;
            let addr = read_u32(mbi + 48) as u64;
            if valid_ptr(addr, len) {
                let mut off = 0u64;
                while off + 24 <= len && info.region_count < MAX_REGIONS {
                    let entry = addr + off;
                    let size = read_u32(entry) as u64;
                    if size < 20 {
                        break; // повреждённая запись: дальше не доверяем
                    }
                    let region = MemRegion {
                        base: read_u64(entry + 4),
                        len: read_u64(entry + 12),
                        kind: read_u32(entry + 20),
                    };
                    let i = info.region_count;
                    info.regions[i] = region;
                    info.region_count += 1;
                    off += size + 4;
                }
            }
        }
        if flags & (1 << 12) != 0 && valid_ptr(mbi, 118) {
            // Поля цвета лежат по смещению 112, а не 110, как в тексте спецификации:
            // в реальной структуре GRUB объединение выровнено на 4 байта.
            let b = |o: u64| *((mbi + o) as *const u8);
            info.framebuffer = Some(Framebuffer {
                addr: read_u64(mbi + 88),
                pitch: read_u32(mbi + 96),
                width: read_u32(mbi + 100),
                height: read_u32(mbi + 104),
                bpp: b(108),
                kind: b(109),
                red_pos: b(112),
                green_pos: b(114),
                blue_pos: b(116),
            });
        }
        if flags & (1 << 9) != 0 {
            let addr = read_u32(mbi + 64) as u64;
            let BootInfo { loader, loader_len, .. } = &mut *info;
            *loader_len = copy_cstr(addr, loader);
        }
    }
    info.valid = true;
    true
}

pub fn with<R>(f: impl FnOnce(&BootInfo) -> R) -> R {
    let info = INFO.lock();
    f(&info)
}

pub fn cmdline_has(word: &str) -> bool {
    with(|i| i.cmdline().split_whitespace().any(|w| w == word))
}
