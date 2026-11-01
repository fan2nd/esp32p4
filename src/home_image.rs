//! Bounded homepage animation format and cross-core bank ownership.
use core::{
    cell::RefCell,
    sync::atomic::{AtomicUsize, Ordering},
};
use critical_section::Mutex;
use embassy_futures::yield_now;
pub const SLOT: usize = 0x400000;
pub const LIMIT: usize = SLOT - 4096;
pub const WIDTH: usize = 350;
pub const HEIGHT: usize = 326;
pub const ARENA: usize = 2 * SLOT + WIDTH * HEIGHT * 2;
static BASE: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Copy)]
pub struct Item {
    pub bank: usize,
    pub len: usize,
    pub serial: u32,
}
pub struct Shared {
    pub active: Option<Item>,
    pub displayed: Option<usize>,
    pub pending: Option<Item>,
    pub uploading: Option<usize>,
    pub serial: u32,
    pub completed: u32,
    pub ok: bool,
    pub loaded: bool,
    pub storage: bool,
}
static SHARED: Mutex<RefCell<Shared>> = Mutex::new(RefCell::new(Shared {
    active: None,
    displayed: None,
    pending: None,
    uploading: None,
    serial: 0,
    completed: 0,
    ok: false,
    loaded: false,
    storage: false,
}));
pub fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    critical_section::with(|cs| f(&mut SHARED.borrow(cs).borrow_mut()))
}
/// Called once by the rendering core with an exclusive PSRAM arena, disjoint
/// from framebuffers. Two upload banks never alias the decode framebuffer.
pub fn init(arena: &'static mut [u8]) -> &'static mut [u16] {
    assert!(arena.len() >= ARENA && arena.as_ptr() as usize % 64 == 0);
    let ptr = arena.as_mut_ptr();
    BASE.store(ptr as usize, Ordering::Release);
    unsafe { core::slice::from_raw_parts_mut(ptr.add(SLOT * 2).cast(), WIDTH * HEIGHT) }
}
pub fn ready() -> bool {
    BASE.load(Ordering::Acquire) != 0
}
/// Caller must own the bank (uploading/pending, boot restore, or displayed).
/// No Rust slice is retained after releasing ownership in Shared.
pub unsafe fn bank_ptr(bank: usize) -> *mut u8 {
    assert!(bank < 2 && ready());
    (BASE.load(Ordering::Acquire) + bank * SLOT) as *mut u8
}
pub struct Upload {
    pub bank: usize,
    committed: bool,
}
impl Upload {
    pub fn begin() -> Option<Self> {
        with(|s| {
            if !ready() || !s.loaded || !s.storage || s.pending.is_some() || s.uploading.is_some() {
                return None;
            }
            let bank = (0..2)
                .find(|b| s.active.is_none_or(|a| a.bank != *b) && s.displayed != Some(*b))?;
            s.uploading = Some(bank);
            Some(Self {
                bank,
                committed: false,
            })
        })
    }
    pub fn submit(mut self, len: usize) -> u32 {
        let serial = with(|s| {
            s.serial = s.serial.wrapping_add(1).max(1);
            s.pending = Some(Item {
                bank: self.bank,
                len,
                serial: s.serial,
            });
            s.uploading = None;
            s.serial
        });
        self.committed = true;
        serial
    }
}
impl Drop for Upload {
    fn drop(&mut self) {
        if !self.committed {
            with(|s| s.uploading = None);
        }
    }
}
pub fn crc(mut crc: u32, data: &[u8]) -> u32 {
    const T: [u32; 16] = [
        0, 0x1db71064, 0x3b6e20c8, 0x26d930ac, 0x76dc4190, 0x6b6b51f4, 0x4db26158, 0x5005713c,
        0xedb88320, 0xf00f9344, 0xd6d6a3e8, 0xcb61b38c, 0x9b64c2b0, 0x86d3d2d4, 0xa00ae278,
        0xbdbdf21c,
    ];
    for b in data {
        crc ^= u32::from(*b);
        crc = (crc >> 4) ^ T[(crc & 15) as usize];
        crc = (crc >> 4) ^ T[(crc & 15) as usize];
    }
    crc
}
fn u16_at(b: &[u8], n: usize) -> usize {
    u16::from_le_bytes([b[n], b[n + 1]]) as usize
}
pub fn u32_at(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(b[n..n + 4].try_into().unwrap())
}
/// Validate every RLE run before persistence/publication; yield during large uploads.
pub async fn validate(bank: usize, len: usize) -> bool {
    if !(16..=LIMIT).contains(&len) {
        return false;
    }
    let data = unsafe { core::slice::from_raw_parts(bank_ptr(bank), len) };
    if &data[..8] != b"P4IMAGE1" {
        return false;
    }
    let w = u16_at(data, 8);
    let h = u16_at(data, 10);
    let frames = u16_at(data, 12);
    if w == 0
        || h == 0
        || w > WIDTH
        || h > HEIGHT
        || !(1..=300).contains(&frames)
        || data[14..16] != [0, 0]
    {
        return false;
    }
    let mut at = 16;
    for _ in 0..frames {
        if at + 8 > len {
            return false;
        }
        let delay = u32_at(data, at);
        let bytes = u32_at(data, at + 4) as usize;
        at += 8;
        if !(80..=10000).contains(&delay) || bytes == 0 || bytes % 4 != 0 || bytes > len - at {
            return false;
        }
        let end = at + bytes;
        let mut pixels = 0;
        while at < end {
            let count = u16_at(data, at);
            if count == 0 || count > w * h - pixels {
                return false;
            }
            pixels += count;
            at += 4;
            if at % 4096 == 0 {
                yield_now().await;
            }
        }
        if pixels != w * h {
            return false;
        }
        yield_now().await;
    }
    at == len
}
/// Core 1 owns decoded pixels; only the tiny-ui overlay reads them during draw.
pub struct Player {
    pub pixels: &'static mut [u16],
    pub width: usize,
    pub height: usize,
    item: Option<Item>,
    offset: usize,
    frame: usize,
    due: u64,
}
impl Player {
    pub fn new(pixels: &'static mut [u16]) -> Self {
        Self {
            pixels,
            width: 0,
            height: 0,
            item: None,
            offset: 16,
            frame: 0,
            due: 0,
        }
    }
    pub fn tick(&mut self, now: u64, visible: bool) -> bool {
        let (active, saving) = with(|s| {
            // All reads of the previous bank finished before this ownership switch.
            s.displayed = s.active.map(|a| a.bank);
            (s.active, s.pending.is_some())
        });
        let changed = active.map(|a| a.serial) != self.item.map(|a| a.serial);
        if changed {
            self.item = active;
            self.offset = 16;
            self.frame = 0;
            self.due = 0;
            self.width = 0;
            self.height = 0;
        }
        let Some(item) = self.item else {
            return changed;
        };
        if !visible || saving {
            // Pause off the home page instead of catching up minutes of frames.
            if self.due != u64::MAX {
                self.due = now;
            }
            return changed;
        }
        if !changed && now < self.due {
            return false;
        }
        let data = unsafe { core::slice::from_raw_parts(bank_ptr(item.bank), item.len) };
        self.width = u16_at(data, 8);
        self.height = u16_at(data, 10);
        let frames = u16_at(data, 12);
        if self.due == 0 || now.saturating_sub(self.due) > 1000 {
            self.due = now;
        }
        // Advance deadlines, skipping overdue full frames without decoding them.
        // Rendering may take longer than one GIF interval; don't slow its timeline.
        loop {
            if self.frame == frames {
                self.frame = 0;
                self.offset = 16;
            }
            let delay = u32_at(data, self.offset) as u64;
            let end = self.offset + 8 + u32_at(data, self.offset + 4) as usize;
            self.frame += 1;
            self.due = if frames == 1 {
                u64::MAX
            } else {
                self.due + delay
            };
            if self.due <= now {
                self.offset = end;
                continue;
            }
            self.offset += 8;
            let mut pixel = 0;
            while self.offset < end {
                let count = u16_at(data, self.offset);
                let color = u16_at(data, self.offset + 2) as u16;
                self.pixels[pixel..pixel + count].fill(color);
                pixel += count;
                self.offset += 4;
            }
            break;
        }
        true
    }
}
