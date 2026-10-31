//! Shared Slint MCU backend: core-1 window, SRAM raster lines and PSRAM output.
use alloc::rc::Rc;
use core::ops::Range;
use esp_hal::time::Instant;
use slint::platform::{
    self,
    software_renderer::{LineBufferProvider, MinimalSoftwareWindow, Rgb565Pixel},
};

slint::include_modules!();

#[global_allocator]
pub static HEAP: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();
pub const FRAME: usize = 720 * 1280 * 2;
pub const HEAP_SIZE: usize = 4 * 1024 * 1024;

pub struct Platform {
    pub window: Rc<MinimalSoftwareWindow>,
}
impl platform::Platform for Platform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_micros(Instant::now().duration_since_epoch().as_micros())
    }
}

/// Slint rasterizes only dirty line spans in SRAM. Each logical pixel becomes
/// one aligned word in each of two physical rows; no PSRAM byte stores are needed.
/// SwappedBuffers also supplies the historical damage of the old back buffer.
pub struct Lines<'a> {
    pub framebuffer: &'a mut [u8],
    pub scratch: &'a mut [Rgb565Pixel; 360],
    pub pixels: &'a mut u32,
}
impl LineBufferProvider for Lines<'_> {
    type TargetPixel = Rgb565Pixel;
    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        render: impl FnOnce(&mut [Rgb565Pixel]),
    ) {
        render(&mut self.scratch[range.clone()]);
        *self.pixels += range.len() as u32;
        for x in range {
            let rgb = self.scratch[x].0;
            // The verified bridge/panel expects BGR565 in memory.
            let bgr = (rgb & 0x07e0) | ((rgb & 0x001f) << 11) | ((rgb & 0xf800) >> 11);
            let word = u32::from(bgr) * 0x0001_0001;
            for y in [line * 2, line * 2 + 1] {
                unsafe {
                    self.framebuffer
                        .as_mut_ptr()
                        .add((y * 720 + x * 2) * 2)
                        .cast::<u32>()
                        .write_volatile(word);
                }
            }
        }
    }
}
