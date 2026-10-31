//! Core 1 Rust-component backend for the composite desktop firmware.
//! USB/audio/network/storage remain in the same Core 0 implementation as Slint.
use crate::{
    desktop::{self, Command},
    desktop_controls::MacroController,
    driver::{display, touch::Gt911},
    keyboard,
    tiny_desktop_page::{Action, HEIGHT, Network, Ui, WIDTH},
};
use core::{convert::Infallible, fmt::Write};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use esp_hal::{
    Blocking,
    delay::Delay,
    i2c::master::I2c,
    mipi_dsi::dpi::{ColorFormat, DpiConfig, FrameTiming},
    peripherals::{MIPI_DSI, PSRAM, VDMA_CH0},
    psram::{Psram, PsramConfig},
    time::Instant,
};
use tiny_ui::Event;

const PANEL_WIDTH: usize = 720;
const FRAME: usize = PANEL_WIDTH * 1280 * 2;

/// The panel wire order is BGR565. UI components remain ordinary RGB565.
struct Panel<'a>(&'a mut [u8]);
impl OriginDimensions for Panel<'_> {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
impl DrawTarget for Panel<'_> {
    type Color = Rgb565;
    type Error = Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(point, color) in pixels {
            if !self.bounding_box().contains(point) {
                continue;
            }
            let word =
                (u16::from(color.b()) << 11) | (u16::from(color.g()) << 5) | u16::from(color.r());
            let bytes = word.to_le_bytes();
            let x = point.x as usize * 2;
            let y = point.y as usize * 2;
            for dy in 0..2 {
                for dx in 0..2 {
                    let offset = ((y + dy) * PANEL_WIDTH + x + dx) * 2;
                    self.0[offset..offset + 2].copy_from_slice(&bytes);
                }
            }
        }
        Ok(())
    }
}

fn dispatch(ui: &mut Ui, event: Event) {
    let Some(action) = ui.handle(event) else {
        return;
    };
    match action {
        Action::Scan => desktop::with(|state| {
            if state.command.is_none() {
                state.command = Some(Command::Scan);
            }
        }),
        Action::Disconnect => desktop::with(|state| {
            if state.command.is_none() {
                state.command = Some(Command::Disconnect);
            }
        }),
        Action::Connect { ssid, mut password } => {
            let accepted = desktop::with(|state| {
                if state.command.is_some() {
                    return false;
                }
                state.command = Some(Command::Connect {
                    ssid,
                    password: password.clone(),
                });
                true
            });
            // This temporary request does not outlive dispatch; retained input is
            // cleared by the page only after the shared command slot accepts it.
            unsafe {
                password.as_mut_vec().as_mut_slice().fill(0);
            }
            password.clear();
            ui.connect_result(accepted);
        }
    }
}

pub fn run(
    dsi: MIPI_DSI<'static>,
    vdma: VDMA_CH0<'static>,
    psram: PSRAM<'static>,
    i2c: I2c<'static, Blocking>,
) -> ! {
    let memory = Psram::new(psram, PsramConfig::default());
    let (ptr, len) = memory.raw_parts();
    assert!(len >= FRAME * 2);
    // No UI heap: just the same two fixed-size PSRAM scanout buffers.
    let framebuffer = unsafe { core::slice::from_raw_parts_mut(ptr, FRAME * 2) };
    let mut ui = Ui::new();
    let (front, back) = framebuffer.split_at_mut(FRAME);
    ui.render_full(&mut Panel(front)).unwrap();
    ui.render_full(&mut Panel(back)).unwrap();
    let (mut dpi, mut i2c) = display::init(
        dsi,
        vdma,
        i2c,
        framebuffer,
        DpiConfig {
            in_color_format: ColorFormat::Rgb565,
            timing: FrameTiming {
                vfp: 1023,
                ..display::DEFAULT_DPI_CONFIG.timing
            },
            ..display::DEFAULT_DPI_CONFIG
        },
    );
    unsafe {
        riscv::interrupt::enable();
    }
    let mut touch = Gt911::probe(&mut i2c);
    let mut contact_id = None;
    let mut position = Point::zero();
    let mut suppressed = false;
    let mut last_touch = 0u64;
    let mut last_probe = 0u64;
    let mut mask = 0u8;
    let mut macros = MacroController::default();
    let mut previous_dirty: Option<Rectangle> = None;
    let mut revision = u32::MAX;
    let mut shown_second = u64::MAX;
    loop {
        if desktop::FLASH_REQUEST.load(core::sync::atomic::Ordering::Acquire) {
            desktop::park_for_flash();
        }
        let now = Instant::now().duration_since_epoch().as_millis();
        if touch.is_none() && now.saturating_sub(last_probe) > 1000 {
            touch = Gt911::probe(&mut i2c);
            last_probe = now;
        }
        if let Some(points) = touch.as_mut().and_then(|driver| driver.poll(&mut i2c)) {
            last_touch = now;
            mask = 0;
            for point in &points {
                if let Some(key) =
                    ui.macro_key_at(Point::new(point.position.x / 2, point.position.y / 2))
                {
                    mask |= 1 << key;
                }
            }
            if points.is_empty() {
                if contact_id.take().is_some() {
                    dispatch(&mut ui, Event::Up(position));
                }
                suppressed = false;
            } else if !suppressed {
                if let Some(id) = contact_id {
                    if let Some(point) = points.iter().find(|point| point.id == id) {
                        position = Point::new(point.position.x / 2, point.position.y / 2);
                        dispatch(&mut ui, Event::Move(position));
                    } else {
                        dispatch(&mut ui, Event::Cancel);
                        contact_id = None;
                        suppressed = true;
                    }
                } else {
                    let point = points[0];
                    contact_id = Some(point.id);
                    position = Point::new(point.position.x / 2, point.position.y / 2);
                    dispatch(&mut ui, Event::Down(position));
                }
            }
        } else if now.saturating_sub(last_touch) > 500 {
            mask = 0;
            if contact_id.take().is_some() {
                dispatch(&mut ui, Event::Cancel);
                suppressed = true;
            }
        }
        let (view, map) = desktop::with(|state| (state.view.clone(), state.keymap.clone()));
        if let Some(report) = macros.update(ui.page() == 3, mask, &map, now) {
            desktop::with(|state| state.report(report));
        }
        desktop::with(|state| state.pressed = macros.pressed());
        let labels: [_; 8] = core::array::from_fn(|key| keyboard::label(map.resolved(0, key)));
        let texts: [&str; 8] = core::array::from_fn(|key| labels[key].as_str());
        ui.sync_macros(&texts, macros.pressed());
        let second = now / 1000;
        if view.revision != revision || second != shown_second {
            let mut clock = heapless::String::<16>::new();
            if let Some((utc, at)) = view.utc {
                let seconds = (utc + second.saturating_sub(at) + 8 * 3600) % 86400;
                let _ = write!(
                    clock,
                    "{:02}:{:02}:{:02}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60
                );
            } else {
                let _ = clock.push_str("--:--:--");
            }
            let mut ip = heapless::String::<24>::new();
            if let Some(address) = view.ip {
                let _ = write!(
                    ip,
                    "IP  {}.{}.{}.{}",
                    address[0], address[1], address[2], address[3]
                );
            }
            ui.sync_status(
                &clock,
                view.ip.is_some(),
                view.bluetooth == "READY",
                view.usb,
                &ip,
                view.message,
            );
            if view.revision != revision {
                let networks: heapless::Vec<Network, 64> = view
                    .networks
                    .iter()
                    .map(|ap| Network {
                        ssid: ap.ssid.clone(),
                        rssi: ap.rssi,
                        secure: ap.secure,
                    })
                    .collect();
                ui.sync_networks(&networks);
            }
            revision = view.revision;
            shown_second = second;
        }
        if let Some(dirty) = ui.dirty() {
            ui.render_with_extra_damage(&mut Panel(dpi.framebuffer_mut()), previous_dirty)
                .unwrap();
            previous_dirty = Some(dirty);
            dpi.commit();
            for _ in 0..3 {
                dpi.wait_for_vsync();
            }
        }
        Delay::new().delay_millis(5);
    }
}
