//! Power-loss-safe image slots. Flash ownership remains with the desktop KV task.
use crate::{
    desktop,
    home_image::{self as image, Item},
};
use core::sync::atomic::Ordering;
use embassy_futures::yield_now;
use embassy_time::{Duration, Instant};
use embedded_storage_async::nor_flash::NorFlash;
pub const START: u32 = 0x700000;
pub const END: u32 = 0xf00000;
pub struct Store {
    loaded: bool,
    slot: usize,
}
impl Store {
    pub fn new() -> Self {
        Self {
            loaded: false,
            slot: 1,
        }
    }
    pub async fn service<F: NorFlash>(&mut self, flash: &mut F, enabled: bool) {
        if !image::ready() {
            return;
        }
        if !self.loaded {
            image::with(|s| s.storage = enabled);
            if enabled {
                self.restore(flash).await;
            }
            image::with(|s| s.loaded = true);
            self.loaded = true;
        }
        let pending = image::with(|s| s.pending);
        if let Some(item) = pending {
            let started = Instant::now();
            let target = 1 - self.slot;
            let ok = enabled && self.save(flash, target, item).await;
            image::with(|s| {
                if ok {
                    s.active = if item.len == 0 { None } else { Some(item) };
                }
                s.pending = None;
                s.completed = item.serial;
                s.ok = ok;
            });
            if ok {
                self.slot = target;
            }
            log::info!(
                "home image: save ok={} bytes={} elapsed_ms={} display_lli_recoveries={}",
                ok,
                item.len,
                started.elapsed().as_millis(),
                esp_hal::mipi_dsi::dpi::LLI_RECOVERIES.load(Ordering::Relaxed)
            );
        }
    }
    async fn restore<F: NorFlash>(&mut self, flash: &mut F) {
        let mut heads = [[0u8; 24]; 2];
        let mut valid = [false; 2];
        for slot in 0..2 {
            if flash
                .read(START + (slot * image::SLOT) as u32, &mut heads[slot])
                .await
                .is_ok()
            {
                let h = &heads[slot];
                valid[slot] = &h[..8] == b"P4ASSET1"
                    && image::u32_at(h, 12) as usize <= image::LIMIT
                    && !image::crc(!0, &h[..20]) == image::u32_at(h, 20);
            }
        }
        let newest = if valid[1]
            && (!valid[0]
                || (image::u32_at(&heads[1], 8).wrapping_sub(image::u32_at(&heads[0], 8)) as i32)
                    > 0)
        {
            1
        } else {
            0
        };
        for slot in [newest, 1 - newest] {
            if !valid[slot] {
                continue;
            }
            let h = &heads[slot];
            let len = image::u32_at(h, 12) as usize;
            let serial = image::u32_at(h, 8);
            let mut crc = !0;
            let mut ok = true;
            let mut scratch = [0u8; 1024];
            for at in (0..len).step_by(scratch.len()) {
                let n = (len - at).min(scratch.len());
                // SRAM staging avoids ROM flash reads writing directly into cached PSRAM.
                if flash
                    .read(
                        START + (slot * image::SLOT + 4096 + at) as u32,
                        &mut scratch[..n],
                    )
                    .await
                    .is_err()
                {
                    ok = false;
                    break;
                }
                crc = image::crc(crc, &scratch[..n]);
                unsafe {
                    core::ptr::copy_nonoverlapping(scratch.as_ptr(), image::bank_ptr(0).add(at), n);
                }
                yield_now().await;
            }
            if !ok || !crc != image::u32_at(h, 16) || (len != 0 && !image::validate(0, len).await) {
                continue;
            }
            self.slot = slot;
            image::with(|s| {
                s.serial = serial;
                s.active = if len == 0 {
                    None
                } else {
                    Some(Item {
                        bank: 0,
                        len,
                        serial,
                    })
                };
            });
            log::info!("home image: restored bytes={}", len);
            break;
        }
    }
    async fn save<F: NorFlash>(&self, flash: &mut F, slot: usize, item: Item) -> bool {
        let base = START + (slot * image::SLOT) as u32;
        let mut scratch = [0u8; 4096];
        let mut crc = !0;
        // Erase only the inactive slot; the old committed header survives any interruption.
        for offset in (0..4096 + item.len.div_ceil(4096) * 4096).step_by(4096) {
            if !operation(flash, base + offset as u32, None).await {
                return false;
            }
            if offset == 0 {
                continue;
            }
            let start = offset - 4096;
            for at in (start..(start + 4096).min(item.len)).step_by(4096) {
                let n = (item.len - at).min(4096);
                scratch.fill(0xff);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        image::bank_ptr(item.bank).add(at),
                        scratch.as_mut_ptr(),
                        n,
                    );
                }
                crc = image::crc(crc, &scratch[..n]);
                if !operation(flash, base + 4096 + at as u32, Some(&scratch)).await {
                    return false;
                }
            }
        }
        // Read back the data before publishing a commit header.
        let mut verify = !0;
        for at in (0..item.len).step_by(4096) {
            let n = (item.len - at).min(4096);
            if flash
                .read(base + 4096 + at as u32, &mut scratch)
                .await
                .is_err()
            {
                return false;
            }
            verify = image::crc(verify, &scratch[..n]);
            yield_now().await;
        }
        if verify != crc {
            return false;
        }
        scratch.fill(0xff);
        scratch[..8].copy_from_slice(b"P4ASSET1");
        scratch[8..12].copy_from_slice(&item.serial.to_le_bytes());
        scratch[12..16].copy_from_slice(&(item.len as u32).to_le_bytes());
        scratch[16..20].copy_from_slice(&(!crc).to_le_bytes());
        let header_crc = !image::crc(!0, &scratch[..20]);
        scratch[20..24].copy_from_slice(&header_crc.to_le_bytes());
        if !operation(flash, base, Some(&scratch)).await {
            return false;
        }
        let mut header = [0u8; 24];
        flash.read(base, &mut header).await.is_ok() && header == scratch[..24]
    }
}
struct Park;
impl Drop for Park {
    fn drop(&mut self) {
        desktop::FLASH_REQUEST.store(false, Ordering::Release);
    }
}
/// One sector-sized operation per park; yield afterwards so audio, USB and Wi-Fi run.
async fn operation<F: NorFlash>(flash: &mut F, addr: u32, bytes: Option<&[u8]>) -> bool {
    desktop::FLASH_REQUEST.store(true, Ordering::Release);
    let guard = Park;
    let started = Instant::now();
    while !desktop::FLASH_PAUSED.load(Ordering::Acquire) {
        if started.elapsed() > Duration::from_secs(2) {
            return false;
        }
        yield_now().await;
    }
    let ok = if let Some(data) = bytes {
        flash.write(addr, data).await.is_ok()
    } else {
        flash.erase(addr, addr + 4096).await.is_ok()
    };
    drop(guard);
    // Wait for acknowledgement to clear before requesting the next park.
    while desktop::FLASH_PAUSED.load(Ordering::Acquire) {
        yield_now().await;
    }
    yield_now().await;
    ok
}
