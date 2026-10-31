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
            let word = u32::from(word) * 0x0001_0001;
            let x = point.x as usize * 2;
            let y = point.y as usize * 2;
            for dy in 0..2 {
                // Match the Slint backend: an aligned SRAM/cache word store
                // writes both physical pixels, avoiding PSRAM byte stores.
                let offset = ((y + dy) * PANEL_WIDTH + x) * 2;
                unsafe {
                    self.0
                        .as_mut_ptr()
                        .add(offset)
                        .cast::<u32>()
                        .write_volatile(word);
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Self::Error> {
        let area = area.intersection(&self.bounding_box());
        let Some(end) = area.bottom_right() else {
            return Ok(());
        };
        let bgr = (u16::from(color.b()) << 11) | (u16::from(color.g()) << 5) | u16::from(color.r());
        let word = u32::from(bgr) * 0x0001_0001;
        // Backgrounds and filled controls use contiguous spans instead of
        // sending every pixel through the general draw_iter clipping path.
        for y in area.top_left.y as usize * 2..(end.y as usize + 1) * 2 {
            let start = (y * PANEL_WIDTH + area.top_left.x as usize * 2) * 2;
            for x in 0..area.size.width as usize {
                unsafe {
                    self.0
                        .as_mut_ptr()
                        .add(start + x * 4)
                        .cast::<u32>()
                        .write_volatile(word);
                }
            }
        }
        Ok(())
    }
}

fn dispatch(ui: &mut Ui, event: Event, i2c: &mut I2c<'_, Blocking>) {
    // Once reset is confirmed, freeze UI mutations until the durable reset
    // record is verified and Core 0 restarts the device.
    if desktop::with(|s| s.reset_requested) {
        return;
    }
    let Some(action) = ui.handle(event) else {
        return;
    };
    match action {
        Action::SyncTime => desktop::with(|s| {
            if matches!(s.command, None | Some(Command::Scan)) {
                s.command = Some(Command::SyncTime);
                s.view.message = "Time sync queued...";
            } else {
                s.view.message = "Request pending. Tap to retry";
            }
            s.view.revision += 1;
        }),
        Action::WifiEnabled(enabled) => desktop::with(|s| {
            // A switch is the latest user intent, including during a scan.
            s.command = Some(Command::Enable(enabled));
            s.view.message = "Switching Wi-Fi...";
            s.view.revision += 1;
        }),
        Action::BleEnabled(enabled) => desktop::with(|s| {
            if s.view.bluetooth == "READY"
                && s.ble_name_command.is_none()
                && s.ble_enable_command.is_none()
                && s.view.ble_name_status != "Applying..."
            {
                s.ble_enable_command = Some(enabled);
                s.view.ble_name_status = "Applying...";
                s.view.revision += 1;
            }
        }),
        Action::ConnectSaved(ssid) => {
            let accepted = desktop::with(|s| {
                if !s.settings.wifi_enabled || !matches!(s.command, None | Some(Command::Scan)) {
                    return false;
                }
                if let Some(profile) = s.settings.profiles.iter().find(|p| p.ssid == ssid) {
                    s.command = Some(Command::Connect {
                        ssid,
                        password: profile.password.clone(),
                    });
                    s.view.message = if s.wifi_busy {
                        "Connection queued..."
                    } else {
                        "Connecting..."
                    };
                    s.view.revision += 1;
                    true
                } else {
                    false
                }
            });
            ui.connect_result(accepted);
        }
        Action::Forget(ssid) => desktop::with(|s| {
            if s.command.is_none() {
                s.command = Some(Command::Forget(ssid));
            }
        }),
        Action::Reset => desktop::with(|s| {
            s.reset_requested = true;
        }),
        Action::Brightness(value) => {
            let ok = display::set_brightness(i2c, value);
            if ok {
                desktop::with(|s| {
                    if s.settings.brightness != value {
                        s.settings.brightness = value;
                        s.settings.revision = s.settings.revision.wrapping_add(1);
                    }
                });
            }
            ui.brightness_result(value, ok);
        }
        Action::Volume(value) => {
            desktop::PLAYBACK_VOLUME.store(value.min(100), core::sync::atomic::Ordering::Relaxed);
            desktop::with(|s| {
                if s.settings.volume != value {
                    s.settings.volume = value;
                    s.settings.revision = s.settings.revision.wrapping_add(1);
                }
            });
            ui.set_setting_message("Playback volume applied");
        }
        Action::BleName(name) => {
            let status = desktop::with(|state| {
                if state.view.bluetooth != "READY" {
                    return "Bluetooth is not ready";
                }
                if state.ble_name_command.is_some()
                    || state.ble_enable_command.is_some()
                    || state.view.ble_name_status == "Applying..."
                {
                    return "Bluetooth busy. Please retry.";
                }
                state.ble_name_command = Some(name);
                state.view.ble_name_status = "Applying...";
                state.view.revision = state.view.revision.wrapping_add(1);
                "Applying..."
            });
            ui.set_setting_message(status);
        }

        Action::Scan => desktop::with(|state| {
            if state.command.is_none() && !state.wifi_busy && state.settings.wifi_enabled {
                state.command = Some(Command::Scan);
            }
        }),
        Action::Disconnect => desktop::with(|state| {
            if state.command.is_none() && !state.wifi_busy && state.settings.wifi_enabled {
                state.command = Some(Command::Disconnect);
            }
        }),
        Action::Connect { ssid, mut password } => {
            let accepted = desktop::with(|state| {
                // An in-flight scan owns the controller, not the command slot.
                // Queue the connection behind it; do not force password re-entry.
                if !state.settings.wifi_enabled
                    || !matches!(state.command, None | Some(Command::Scan))
                {
                    return false;
                }
                state.command = Some(Command::Connect {
                    ssid,
                    password: password.clone(),
                });
                state.view.message = if state.wifi_busy {
                    "Connection queued..."
                } else {
                    "Connecting..."
                };
                state.view.revision += 1;
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
    let brightness = desktop::with(|s| s.settings.brightness);
    let _ = display::set_brightness(&mut i2c, brightness);
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
                    dispatch(&mut ui, Event::Up(position), &mut i2c);
                }
                suppressed = false;
            } else if !suppressed {
                if let Some(id) = contact_id {
                    if let Some(point) = points.iter().find(|point| point.id == id) {
                        position = Point::new(point.position.x / 2, point.position.y / 2);
                        dispatch(&mut ui, Event::Move(position), &mut i2c);
                    } else {
                        dispatch(&mut ui, Event::Cancel, &mut i2c);
                        contact_id = None;
                        suppressed = true;
                    }
                } else {
                    let point = points[0];
                    contact_id = Some(point.id);
                    position = Point::new(point.position.x / 2, point.position.y / 2);
                    dispatch(&mut ui, Event::Down(position), &mut i2c);
                }
            }
        } else if now.saturating_sub(last_touch) > 500 {
            mask = 0;
            if contact_id.take().is_some() {
                dispatch(&mut ui, Event::Cancel, &mut i2c);
                suppressed = true;
            }
        }
        let (view, map) = desktop::with(|state| (state.view.clone(), state.keymap.clone()));
        let (brightness, volume) = desktop::with(|s| (s.settings.brightness, s.settings.volume));
        ui.sync_settings(brightness, volume, view.save_status);
        let switches = desktop::with(|s| (s.settings.wifi_enabled, s.view.ble_advertising));
        ui.sync_radio_switches(switches.0, switches.1);
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
            ui.sync_ble_name(&view.ble_name, view.ble_name_status);
            ui.sync_status(
                &clock,
                view.ip.is_some(),
                view.bluetooth == "READY" && view.ble_advertising,
                view.usb,
                &ip,
                view.message,
            );
            if view.revision != revision {
                // Copy public profile names only; credentials never enter the display core.
                let networks = desktop::with(|s| {
                    let mut networks = heapless::Vec::<Network, 64>::new();
                    for profile in s.settings.profiles.iter().rev() {
                        let ap = view.networks.iter().find(|ap| ap.ssid == profile.ssid);
                        networks
                            .push(Network {
                                ssid: profile.ssid.clone(),
                                rssi: ap.map_or(0, |a| a.rssi),
                                secure: ap.is_none_or(|a| a.secure),
                                saved: true,
                                connected: view.ip.is_some() && s.settings.ssid == profile.ssid,
                                available: ap.is_some(),
                            })
                            .ok();
                    }
                    for ap in &view.networks {
                        if !networks.iter().any(|n| n.ssid == ap.ssid) {
                            networks
                                .push(Network {
                                    ssid: ap.ssid.clone(),
                                    rssi: ap.rssi,
                                    secure: ap.secure,
                                    saved: false,
                                    connected: false,
                                    available: true,
                                })
                                .ok();
                        }
                    }
                    networks
                });
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
