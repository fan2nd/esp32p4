#![no_std]
#![no_main]

use core::convert::Infallible;
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::{Baseline, Text},
};
use esp_hal::{
    delay::Delay,
    i2c::master::{Config, I2c},
    psram::{Psram, PsramConfig},
};
use panic_halt as _;
use riscv_rt::entry;
use waveshare_p4_eth::driver::display::{self, HEIGHT, WIDTH};

esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    let psram = Psram::new(p.PSRAM, PsramConfig::default());
    let i2c = I2c::new(p.I2C1, Config::default())
        .expect("I2C init failed")
        .with_sda(p.GPIO7)
        .with_scl(p.GPIO8);
    let (fb_ptr, fb_len) = psram.raw_parts();
    assert!(fb_len >= display::FRAMEBUFFER_SIZE);
    let framebuffer = unsafe { core::slice::from_raw_parts_mut(fb_ptr, display::FRAMEBUFFER_SIZE) };
    draw_pattern(framebuffer);
    // Draw before DMA starts and keep the DPI handle alive while streaming.
    let (_display, _i2c) = display::init(p.MIPI_DSI, p.VDMA_CH0, i2c, framebuffer);
    esp_println::println!(
        "verify_display: display ready; native origin=(0,0) top-left, x right 0..719, y down 0..1279"
    );
    esp_println::println!(
        "verify_display: coordinate guide on dark background; native panel orientation"
    );
    loop {
        Delay::new().delay_millis(1000);
    }
}

// Adapter for the panel's packed RGB888 PSRAM buffer; DMA starts after drawing.
struct Framebuffer<'a>(&'a mut [u8]);

impl OriginDimensions for Framebuffer<'_> {
    fn size(&self) -> Size {
        Size::new(u32::from(WIDTH), u32::from(HEIGHT))
    }
}

impl DrawTarget for Framebuffer<'_> {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            // Primitives can extend outside the panel, including negative coordinates.
            if self.bounding_box().contains(point) {
                let offset = (point.y as usize * WIDTH as usize + point.x as usize) * 3;
                self.0[offset..offset + 3].copy_from_slice(&[color.r(), color.g(), color.b()]);
            }
        }
        Ok(())
    }
}

fn draw_pattern(framebuffer: &mut [u8]) {
    assert_eq!(framebuffer.len(), WIDTH as usize * HEIGHT as usize * 3);
    let mut target = Framebuffer(framebuffer);
    target.clear(Rgb888::new(0x12, 0x1c, 0x2c)).unwrap();
    let stroke = PrimitiveStyle::with_stroke(Rgb888::WHITE, 2);
    for (start, end) in [
        ((40, 80), (650, 80)),
        ((40, 80), (40, 1160)),
        ((650, 80), (630, 70)),
        ((650, 80), (630, 90)),
        ((40, 1160), (30, 1140)),
        ((40, 1160), (50, 1140)),
    ] {
        Line::new(Point::from(start), Point::from(end))
            .into_styled(stroke)
            .draw(&mut target)
            .unwrap();
    }
    let font = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);
    for (position, text) in [
        ((12, 12), "ORIGIN (0,0): TOP LEFT"),
        ((55, 95), "(40,80)"),
        ((540, 50), "X RIGHT"),
        ((55, 1170), "Y DOWN"),
        ((540, 1220), "720x1280"),
    ] {
        Text::with_baseline(text, Point::from(position), font, Baseline::Top)
            .draw(&mut target)
            .unwrap();
    }
}
