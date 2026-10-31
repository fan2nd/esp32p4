//! Slint desktop on core 1; core 0 retains USB/audio/radio and flash persistence.
use crate::{
    desktop::{self, AccessPoint, Command},
    driver::{display, touch::Gt911},
    desktop_controls::MacroController,
    keyboard,
    password_keyboard::{self, Action, Password},
    slint_backend::{
        DesktopWindow, FRAME, HEAP, HEAP_SIZE, Lines, MacroKey, Network, Platform, TypingKey,
    },
};
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::{cell::RefCell, convert::Infallible, fmt::Write};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::*,
    text::Text,
};
use esp_hal::{
    Blocking,
    i2c::master::I2c,
    peripherals::{MIPI_DSI, PSRAM, VDMA_CH0},
    psram::{Psram, PsramConfig},
    time::Instant,
};
use slint::{
    ComponentHandle, Model, ModelRc, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer, VecModel,
    platform::{
        self, PointerEventButton, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel},
    },
};

const IMAGE: &[u8] = include_bytes!("../assets/whale-girl-wave-large.rgb");
const CJK: &[u8] = include_bytes!("../assets/cjk16.bin");

/// Reuse the project's licensed CJK bitmap for dynamic SSIDs. Slint's embedded
/// font subset cannot anticipate the Chinese characters returned by a scan.
struct SsidCanvas(SharedPixelBuffer<Rgba8Pixel>);
impl OriginDimensions for SsidCanvas {
    fn size(&self) -> Size {
        Size::new(320, 20)
    }
}
impl DrawTarget for SsidCanvas {
    type Color = Rgb888;
    type Error = Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb888>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            if (0..320).contains(&p.x) && (0..20).contains(&p.y) {
                self.0.make_mut_slice()[p.y as usize * 320 + p.x as usize] =
                    Rgba8Pixel::new(c.r(), c.g(), c.b(), 255);
            }
        }
        Ok(())
    }
}
fn ssid_image(value: &str) -> slint::Image {
    let mut canvas = SsidCanvas(SharedPixelBuffer::new(320, 20));
    let mut x = 0;
    for ch in value.chars() {
        if x > 300 {
            break;
        }
        if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            let offset = (ch as usize - 0x4e00) * 32;
            for y in 0..16 {
                for col in 0..16 {
                    if CJK[offset + y * 2 + col / 8] & (0x80 >> (col % 8)) != 0 {
                        let _ = canvas.draw_iter([Pixel(
                            Point::new(x + col as i32, y as i32 + 2),
                            Rgb888::new(24, 45, 68),
                        )]);
                    }
                }
            }
            x += 16;
        } else {
            let mut bytes = [0; 4];
            Text::new(
                ch.encode_utf8(&mut bytes),
                Point::new(x, 16),
                MonoTextStyle::new(&FONT_10X20, Rgb888::new(24, 45, 68)),
            )
            .draw(&mut canvas)
            .unwrap();
            x += 10;
        }
    }
    slint::Image::from_rgba8(canvas.0)
}

#[derive(Default)]
struct Input {
    password: Password,
    ssid: heapless::String<32>,
}

fn sync_password(ui: &DesktopWindow, input: &Input, keys: &VecModel<TypingKey>) {
    let mask: alloc::string::String =
        core::iter::repeat_n('*', input.password.text.len()).collect();
    ui.set_password_mask(mask.as_str().into());
    ui.set_password_hint(
        alloc::format!(
            "{} / 64 characters   Shift: {}   Caps: {}",
            input.password.text.len(),
            if input.password.shift { "ON" } else { "off" },
            if input.password.caps { "ON" } else { "off" }
        )
        .into(),
    );
    for (index, key) in password_keyboard::layout().iter().enumerate() {
        let mut bytes = [0; 4];
        let label = match key.action {
            Action::Character(a, b) => input.password.character(a, b).encode_utf8(&mut bytes),
            Action::Shift => "Shift",
            Action::Caps => "Caps",
            Action::Backspace => "Bksp",
            Action::Enter => "Enter",
            Action::Clear => "Clear",
            Action::Space => "Space",
        };
        let item = TypingKey {
            label: label.into(),
            x: key.x,
            width: key.width,
            row: key.row as i32,
            active: (key.action == Action::Shift && input.password.shift)
                || (key.action == Action::Caps && input.password.caps),
        };
        if let Some(old) = keys.row_data(index) {
            if old.label != item.label || old.active != item.active {
                keys.set_row_data(index, item);
            }
        } else {
            keys.push(item);
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
    assert!(len >= FRAME * 2 + HEAP_SIZE);
    // Only this core allocates Slint objects. Audio DMA and USB queues remain SRAM.
    let fb = unsafe {
        HEAP.lock().init(ptr.add(FRAME * 2), HEAP_SIZE);
        core::slice::from_raw_parts_mut(ptr, FRAME * 2)
    };
    fb.fill(0);
    let (mut dpi, mut i2c) = display::init(
        dsi,
        vdma,
        i2c,
        fb,
        esp_hal::mipi_dsi::dpi::DpiConfig {
            in_color_format: esp_hal::mipi_dsi::dpi::ColorFormat::Rgb565,
            timing: esp_hal::mipi_dsi::dpi::FrameTiming {
                vfp: 1023,
                ..display::DEFAULT_DPI_CONFIG.timing
            },
            ..display::DEFAULT_DPI_CONFIG
        },
    );
    unsafe {
        riscv::interrupt::enable();
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::SwappedBuffers);
    platform::set_platform(Box::new(Platform {
        window: window.clone(),
    }))
    .unwrap();
    let ui = DesktopWindow::new().unwrap();
    window.set_size(slint::PhysicalSize::new(360, 640));
    let mut whale = SharedPixelBuffer::<Rgb8Pixel>::new(350, 326);
    for y in 0..326 {
        for x in 0..350 {
            let source = (y * 2 * 700 + x * 2) * 3;
            let target = (y * 350 + x) * 3;
            whale.make_mut_bytes()[target..target + 3].copy_from_slice(&IMAGE[source..source + 3]);
        }
    }
    ui.set_whale(slint::Image::from_rgb8(whale));
    let networks = Rc::new(VecModel::<Network>::default());
    let aps = Rc::new(RefCell::new(heapless::Vec::<AccessPoint, 64>::new()));
    let input = Rc::new(RefCell::new(Input::default()));
    let typing = Rc::new(VecModel::<TypingKey>::default());
    let macro_keys = Rc::new(VecModel::from(Vec::from_iter((0..8).map(|_| MacroKey {
        label: "".into(),
        active: false,
    }))));
    ui.set_networks(ModelRc::from(networks.clone()));
    ui.set_typing_keys(ModelRc::from(typing.clone()));
    ui.set_macro_keys(ModelRc::from(macro_keys.clone()));
    sync_password(&ui, &input.borrow(), &typing);
    {
        let weak = ui.as_weak();
        let input = input.clone();
        ui.on_navigate(move |page| {
            let ui = weak.unwrap();
            input.borrow_mut().password.clear();
            ui.set_password_mask("".into());
            ui.set_page(page);
            if page == 1 {
                desktop::with(|s| {
                    if s.command.is_none() {
                        s.command = Some(Command::Scan)
                    }
                });
            }
        });
    }
    ui.on_scan(|| {
        desktop::with(|s| {
            if s.command.is_none() {
                s.command = Some(Command::Scan);
            }
        })
    });
    ui.on_disconnect(|| {
        desktop::with(|s| {
            if s.command.is_none() {
                s.command = Some(Command::Disconnect);
            }
        })
    });
    {
        let weak = ui.as_weak();
        let aps = aps.clone();
        let input = input.clone();
        let typing = typing.clone();
        ui.on_choose_network(move |index| {
            let Some(ap) = aps.borrow().get(index as usize).cloned() else {
                return;
            };
            let ui = weak.unwrap();
            let mut input = input.borrow_mut();
            input.password.clear();
            input.ssid = ap.ssid;
            ui.set_selected_ssid(ssid_image(&input.ssid));
            sync_password(&ui, &input, &typing);
            ui.set_page(2);
        });
    }
    {
        let weak = ui.as_weak();
        let input = input.clone();
        ui.on_connect(move || {
            let ui = weak.unwrap();
            let mut input = input.borrow_mut();
            if input.ssid.is_empty() {
                return;
            }
            let accepted = desktop::with(|s| {
                if s.command.is_some() {
                    return false;
                }
                s.command = Some(Command::Connect {
                    ssid: input.ssid.clone(),
                    password: input.password.text.clone(),
                });
                true
            });
            if accepted {
                input.password.clear();
                ui.set_password_mask("".into());
                ui.set_page(1);
            } else {
                ui.set_password_hint("Wi-Fi busy. Please retry.".into());
            }
        });
    }
    {
        let weak = ui.as_weak();
        let input = input.clone();
        let typing = typing.clone();
        ui.on_type_key(move |index| {
            let Some(key) = password_keyboard::layout().get(index as usize).copied() else {
                return;
            };
            let ui = weak.unwrap();
            let enter = {
                let mut input = input.borrow_mut();
                let enter = input.password.press(key.action);
                sync_password(&ui, &input, &typing);
                enter
            };
            if enter {
                ui.invoke_connect();
            }
        });
    }
    ui.show().unwrap();
    let mut touch = Gt911::probe(&mut i2c);
    log::info!("desktop: Slint core1 ready; touch={}", touch.is_some());
    let mut scratch = [Rgb565Pixel(0); 360];
    let mut position = slint::LogicalPosition::new(0., 0.);
    let mut down = false;
    let mut last_touch = 0;
    let mut last_probe = 0;
    let mut revision = u32::MAX;
    let mut map_revision = u32::MAX;
    let mut shown_second = u64::MAX;
    let mut old_mask = 0u8;
    let mut macros = MacroController::default();
    loop {
        if desktop::FLASH_REQUEST.load(core::sync::atomic::Ordering::Acquire) {
            desktop::park_for_flash();
        }
        let now = Instant::now().duration_since_epoch().as_millis();
        if touch.is_none() && now - last_probe > 1000 {
            touch = Gt911::probe(&mut i2c);
            last_probe = now;
        }
        let mut mask = old_mask;
        if let Some(points) = touch.as_mut().and_then(|t| t.poll(&mut i2c)) {
            last_touch = now;
            mask = 0;
            // Preserve simultaneous macro-key chords; Slint uses one pointer for navigation.
            for contact in &points {
                let key = ui.invoke_key_at(
                    contact.position.x as f32 / 2.,
                    contact.position.y as f32 / 2.,
                );
                if (0..8).contains(&key) {
                    mask |= 1 << key;
                }
            }
            if let Some(contact) = points.first() {
                position = slint::LogicalPosition::new(
                    contact.position.x as f32 / 2.,
                    contact.position.y as f32 / 2.,
                );
                window.dispatch_event(if down {
                    WindowEvent::PointerMoved { position }
                } else {
                    WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    }
                });
                down = true;
            } else if down {
                window.dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                });
                window.dispatch_event(WindowEvent::PointerExited);
                down = false;
            }
        } else if now - last_touch > 500 {
            mask = 0;
            if down {
                window.dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                });
                window.dispatch_event(WindowEvent::PointerExited);
                down = false;
            }
        }
        let (view, map) = desktop::with(|s| (s.view.clone(), s.keymap.clone()));
        if let Some(report) = macros.update(ui.get_page() == 3, mask, &map, now) {
            desktop::with(|s| s.report(report));
        }
        mask = macros.pressed();
        desktop::with(|s| s.pressed = mask);
        if old_mask != mask || map_revision != map.revision {
            for k in 0..8 {
                let item = MacroKey {
                    label: keyboard::label(map.resolved(0, k)).as_str().into(),
                    active: mask & (1 << k) != 0,
                };
                let old = macro_keys.row_data(k).unwrap();
                if old.label != item.label || old.active != item.active {
                    macro_keys.set_row_data(k, item);
                }
            }
            map_revision = map.revision;
        }
        old_mask = mask;
        if view.revision != revision {
            ui.set_wifi_ready(view.ip.is_some());
            ui.set_bluetooth_ready(view.bluetooth == "READY");
            ui.set_usb_ready(view.usb);
            ui.set_usb_status(
                if view.usb {
                    "USB connected"
                } else {
                    "USB inactive"
                }
                .into(),
            );
            ui.set_message(view.message.into());
            ui.set_ip_address(
                view.ip
                    .map(|ip| alloc::format!("IP  {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]))
                    .unwrap_or_default()
                    .into(),
            );
            // Rebuild only when scan results really change; status/time updates
            // must not reset the scroll offset or reallocate every SSID image.
            if *aps.borrow() != view.networks {
                networks.set_vec(
                    view.networks
                        .iter()
                        .map(|ap| Network {
                            name: ssid_image(&ap.ssid),
                            detail: alloc::format!(
                                "{} dBm / {}",
                                ap.rssi,
                                if ap.secure { "secured" } else { "open" }
                            )
                            .into(),
                        })
                        .collect::<Vec<_>>(),
                );
                *aps.borrow_mut() = view.networks.clone();
            }
            revision = view.revision;
            shown_second = u64::MAX;
        }
        let second = now / 1000;
        if second != shown_second {
            let mut time = heapless::String::<16>::new();
            if let Some((utc, at)) = view.utc {
                let s = (utc + second.saturating_sub(at) + 8 * 3600) % 86400;
                let _ = write!(time, "{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60);
            } else {
                let _ = time.push_str("--:--:--");
            }
            ui.set_clock(time.as_str().into());
            shown_second = second;
        }
        platform::update_timers_and_animations();
        let started = Instant::now();
        let mut pixels = 0;
        if window.draw_if_needed(|renderer| {
            renderer.render_by_line(Lines {
                framebuffer: dpi.framebuffer_mut(),
                scratch: &mut scratch,
                pixels: &mut pixels,
            });
        }) {
            let rendered = Instant::now();
            dpi.commit();
            for _ in 0..3 {
                dpi.wait_for_vsync();
            }
            log::debug!(
                "desktop: Slint pixels={} render={}ms total={}ms",
                pixels,
                (rendered - started).as_millis(),
                started.elapsed().as_millis()
            );
        }
        esp_hal::delay::Delay::new().delay_millis(5);
    }
}
