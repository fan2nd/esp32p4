#![no_std]
#![no_main]

//! Pure Rust component-framework showcase matching ui/demo.slint geometry.
use core::{
    convert::Infallible,
    sync::atomic::{AtomicU32, Ordering},
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use esp_hal::{
    delay::Delay,
    i2c::master::{Config, I2c},
    mipi_dsi::dpi::{ColorFormat, DpiConfig},
    psram::{Psram, PsramConfig},
    system::{CpuControl, Stack},
    time::Instant,
};
use riscv_rt::entry;
use static_cell::StaticCell;
use tiny_ui_demo::{Action, Event, Ui};
use waveshare_p4_eth::driver::{display, touch::Gt911};

esp_bootloader_esp_idf::esp_app_desc!();
static STACK: StaticCell<Stack<32768>> = StaticCell::new();
static FRAMES: AtomicU32 = AtomicU32::new(0);
const PANEL_WIDTH: usize = 720;
const PANEL_HEIGHT: usize = 1280;
const FRAME: usize = PANEL_WIDTH * PANEL_HEIGHT * 2;
const SCALE: i32 = 2;
const OFFSET: Point = Point::zero();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("rust_ui_demo PANIC: {}", info);
    loop {
        core::hint::spin_loop();
    }
}

/// Logical RGB565 pixels, doubled into the verified panel BGR565 byte layout.
struct Panel<'a>(&'a mut [u8]);
impl OriginDimensions for Panel<'_> {
    fn size(&self) -> Size {
        Size::new(tiny_ui_demo::WIDTH, tiny_ui_demo::HEIGHT)
    }
}
impl DrawTarget for Panel<'_> {
    type Color = Rgb565;
    type Error = Infallible;
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(p, c) in pixels {
            if !self.bounding_box().contains(p) {
                continue;
            }
            let word = (u16::from(c.b()) << 11) | (u16::from(c.g()) << 5) | u16::from(c.r());
            let bytes = word.to_le_bytes();
            let x = (p.x * SCALE + OFFSET.x) as usize;
            let y = (p.y * SCALE + OFFSET.y) as usize;
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = ((y + dy) * PANEL_WIDTH + x + dx) * 2;
                    self.0[i..i + 2].copy_from_slice(&bytes);
                }
            }
        }
        Ok(())
    }
}
fn logical(p: Point) -> Point {
    // Euclidean division keeps pixels just outside the canvas outside hit targets.
    Point::new(
        (p.x - OFFSET.x).div_euclid(SCALE),
        (p.y - OFFSET.y).div_euclid(SCALE),
    )
}
fn dispatch(ui: &mut Ui, event: Event) {
    // Application-level actions are separate from components and hardware I/O.
    if let Some(action) = ui.handle(event) {
        match action {
            Action::VolumeChanged(value) => esp_println::println!("rust-ui: volume={}", value),
            Action::ItemSelected(index) => esp_println::println!("rust-ui: item={}", index + 1),
            Action::ModeChanged(index) => esp_println::println!("rust-ui: mode={}", index),
            Action::Reset => esp_println::println!("rust-ui: reset controls"),
        }
    }
}

#[entry]
fn main() -> ! {
    let p =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    let psram = Psram::new(p.PSRAM, PsramConfig::default());
    let (ptr, len) = psram.raw_parts();
    assert!(len >= FRAME * 2);
    // Two fixed scanout buffers, no allocator. Only core 1 owns their slice.
    let fb = unsafe { core::slice::from_raw_parts_mut(ptr, FRAME * 2) };
    let mut cpu = CpuControl::new(p.CPU_CTRL);
    let _guard = cpu
        .start_app_core(STACK.init(Stack::new()), move || {
            fb.fill(0);
            let mut ui = Ui::new();
            let (front, back) = fb.split_at_mut(FRAME);
            ui.render(&mut Panel(front)).unwrap();
            ui.render_full(&mut Panel(back)).unwrap();
            let i2c = I2c::new(p.I2C1, Config::default())
                .unwrap()
                .with_sda(p.GPIO7)
                .with_scl(p.GPIO8);
            let (mut dpi, mut i2c) = display::init(
                p.MIPI_DSI,
                p.VDMA_CH0,
                i2c,
                fb,
                DpiConfig {
                    in_color_format: ColorFormat::Rgb565,
                    ..display::DEFAULT_DPI_CONFIG
                },
            );
            unsafe {
                riscv::interrupt::enable();
            }
            let mut touch = Gt911::probe(&mut i2c);
            let mut contact = None;
            let mut position = Point::zero();
            let mut suppressed = false;
            let mut last_report = Instant::now();
            let mut last_probe = Instant::now();
            let mut previous_dirty: Option<Rectangle> = None;
            esp_println::println!(
                "rust-ui: core1 ready; touch={}; no allocator",
                touch.is_some()
            );
            loop {
                if touch.is_none() && last_probe.elapsed().as_millis() >= 1000 {
                    touch = Gt911::probe(&mut i2c);
                    last_probe = Instant::now();
                }
                if let Some(points) = touch.as_mut().and_then(|t| t.poll(&mut i2c)) {
                    last_report = Instant::now();
                    if points.is_empty() {
                        if contact.take().is_some() {
                            dispatch(&mut ui, Event::Up(position));
                        }
                        suppressed = false;
                    } else if !suppressed {
                        if let Some(id) = contact {
                            if let Some(point) = points.iter().find(|point| point.id == id) {
                                position = logical(point.position);
                                dispatch(&mut ui, Event::Move(position));
                            } else {
                                // Never transfer an active gesture to a second finger.
                                dispatch(&mut ui, Event::Cancel);
                                contact = None;
                                suppressed = true;
                            }
                        } else {
                            let point = points[0];
                            contact = Some(point.id);
                            position = logical(point.position);
                            dispatch(&mut ui, Event::Down(position));
                        }
                    }
                }
                if contact.is_some() && last_report.elapsed().as_millis() > 500 {
                    // Missing reports are not a click/release. Require all-up to rearm.
                    dispatch(&mut ui, Event::Cancel);
                    contact = None;
                    suppressed = true;
                }
                if let Some(dirty) = ui.dirty() {
                    let mut target = Panel(dpi.framebuffer_mut());
                    // This back buffer is one UI update behind. Repair the previous
                    // update too; otherwise alternating buffers resurrect stale pixels.
                    ui.render_with_extra_damage(&mut target, previous_dirty)
                        .unwrap();
                    previous_dirty = Some(dirty);
                    dpi.commit();
                    // Preserve the repository's validated DMA descriptor reuse guard.
                    for _ in 0..3 {
                        dpi.wait_for_vsync();
                    }
                    FRAMES.fetch_add(1, Ordering::Release);
                }
                Delay::new().delay_millis(5);
            }
        })
        .unwrap();
    loop {
        Delay::new().delay_millis(2000);
        esp_println::println!(
            "rust-ui: core0 alive; updates={}",
            FRAMES.load(Ordering::Acquire)
        );
    }
}
