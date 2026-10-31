//! Core-1-only touchscreen UI and RGB565 double-buffer renderer.
use crate::{
    desktop::{self, Command, View},
    driver::{display, touch::Gt911},
    keyboard::{self, Keymap, MacroPlayer, MacroStep},
};
use core::{convert::Infallible, fmt::Write};
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
    text::{Alignment, Text},
};
use esp_hal::{
    Blocking,
    i2c::master::I2c,
    peripherals::{MIPI_DSI, PSRAM, VDMA_CH0},
    psram::{Psram, PsramConfig},
    time::Instant,
};
const BG: Rgb888 = Rgb888::new(232, 247, 255);
const INK: Rgb888 = Rgb888::new(24, 45, 68);
const ACCENT: Rgb888 = Rgb888::new(38, 145, 181);
const IMAGE: &[u8; 700 * 653 * 3] = include_bytes!("../assets/whale-girl-wave-large.rgb");
const CJK: &[u8] = include_bytes!("../assets/cjk16.bin");

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Settings,
    Password,
    Keyboard,
}
struct Ui {
    page: Page,
    list_page: usize,
    selected_ssid: heapless::String<32>,
    password: heapless::String<64>,
    shift: bool,
    symbols: bool,
    layer: usize,
    pressed: u8,
}
fn button(canvas: &mut Canvas<'_>, rect: Rectangle, label: &str, active: bool) {
    // Render only widgets intersecting this internal-RAM strip. Otherwise each
    // rounded button is rasterized forty times for a single full-screen draw.
    if rect.top_left.y >= canvas.1 + 16
        || rect.top_left.y + rect.size.height as i32 <= canvas.1
    {
        return;
    }
    RoundedRectangle::with_equal_corners(rect, Size::new(8, 8))
        .into_styled(PrimitiveStyle::with_fill(if active {
            ACCENT
        } else {
            Rgb888::new(205, 228, 239)
        }))
        .draw(canvas)
        .unwrap();
    Text::with_alignment(
        label,
        rect.top_left + Point::new(rect.size.width as i32 / 2, rect.size.height as i32 / 2 + 6),
        MonoTextStyle::new(&FONT_10X20, if active { Rgb888::WHITE } else { INK }),
        Alignment::Center,
    )
    .draw(canvas)
    .unwrap();
}
fn label(canvas: &mut Canvas<'_>, text: &str, p: Point, small: bool) {
    let text = Text::new(
        text,
        p,
        MonoTextStyle::new(if small { &FONT_6X10 } else { &FONT_10X20 }, INK),
    );
    let bounds = text.bounding_box();
    if bounds.top_left.y < canvas.1 + 16
        && bounds.top_left.y + bounds.size.height as i32 > canvas.1
    {
        text.draw(canvas).unwrap();
    }
}
fn ssid(canvas: &mut Canvas<'_>, value: &str, p: Point) {
    // Both the CJK glyph and ASCII font fit inside this baseline envelope.
    if p.y - 20 >= canvas.1 + 16 || p.y + 5 <= canvas.1 {
        return;
    }
    let mut x = p.x;
    for c in value.chars() {
        if x > 330 {
            break;
        }
        if ('\u{4e00}'..='\u{9fff}').contains(&c) {
            let offset = (c as usize - 0x4e00) * 32;
            for y in 0..16 {
                for col in 0..16 {
                    if CJK[offset + y * 2 + col / 8] & (0x80 >> (col % 8)) != 0 {
                        let _ = canvas.draw_iter([Pixel(
                            Point::new(x + col as i32, p.y - 15 + y as i32),
                            INK,
                        )]);
                    }
                }
            }
            x += 16;
        } else {
            let mut bytes = [0; 4];
            label(canvas, c.encode_utf8(&mut bytes), Point::new(x, p.y), false);
            x += 10;
        }
    }
}
fn key_rect(k: usize) -> Rectangle {
    Rectangle::new(
        Point::new(16 + (k % 2) as i32 * 172, 138 + (k / 2) as i32 * 110),
        Size::new(156, 99),
    )
}
fn characters(ui: &Ui) -> &'static [u8] {
    if ui.symbols {
        b"1234567890!@#$%^&*()-_=+[]{};:'\",.<>/?\\|`~"
    } else if ui.shift {
        b"1234567890QWERTYUIOPASDFGHJKLZXCVBNM"
    } else {
        b"1234567890qwertyuiopasdfghjklzxcvbnm"
    }
}
fn draw(canvas: &mut Canvas<'_>, ui: &Ui, view: &View, map: &Keymap, now: u64) {
    if ui.page == Page::Home {
        let mut time = heapless::String::<24>::new();
        if let Some((utc, at)) = view.utc {
            let seconds = (utc + now.saturating_sub(at) + 8 * 3600) % 86400;
            let _ = write!(
                time,
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
        } else {
            let _ = time.push_str("--:--:--");
        }
        label(canvas, &time, Point::new(12, 32), false);
        button(
            canvas,
            Rectangle::new(Point::new(306, 8), Size::new(44, 32)),
            "SET",
            false,
        );
        let mut status = heapless::String::<64>::new();
        let _ = write!(status, "WiFi: {}   BT: {}", view.wifi, view.bluetooth);
        label(canvas, &status, Point::new(12, 55), true);
        label(
            canvas,
            if view.usb {
                "USB: CONNECTED"
            } else {
                "USB: DISCONNECTED"
            },
            Point::new(12, 74),
            true,
        );
        if let Some(ip) = view.ip {
            let mut text = heapless::String::<24>::new();
            let _ = write!(text, "IP: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
            label(canvas, &text, Point::new(12, 93), true);
        }
        // Sample the existing 700x653 illustration at 2:1 logical resolution;
        // panel scaling restores 700x652 native pixels without allocating an image.
        let start = canvas.1.max(170);
        let end = (canvas.1 + 16).min(496);
        for y in start..end {
            for x in 0..350 {
                let i = (((y - 170) as usize * 2) * 700 + x * 2) * 3;
                let _ = canvas.draw_iter([Pixel(
                    Point::new(x as i32 + 5, y),
                    Rgb888::new(IMAGE[i], IMAGE[i + 1], IMAGE[i + 2]),
                )]);
            }
        }
        return;
    }
    button(
        canvas,
        Rectangle::new(Point::new(10, 8), Size::new(74, 34)),
        "BACK",
        false,
    );
    match ui.page {
        Page::Settings => {
            label(canvas, "SETTINGS", Point::new(105, 33), false);
            button(
                canvas,
                Rectangle::new(Point::new(12, 60), Size::new(156, 42)),
                "SCAN WIFI",
                false,
            );
            button(
                canvas,
                Rectangle::new(Point::new(188, 60), Size::new(156, 42)),
                "KEYBOARD",
                false,
            );
            label(canvas, view.message, Point::new(12, 125), true);
            for row in 0..6 {
                if let Some(ap) = view.networks.get(ui.list_page * 6 + row) {
                    let y = 148 + row as i32 * 60;
                    button(
                        canvas,
                        Rectangle::new(Point::new(12, y), Size::new(332, 52)),
                        "",
                        false,
                    );
                    ssid(canvas, &ap.ssid, Point::new(22, y + 23));
                    let mut info = heapless::String::<32>::new();
                    let _ = write!(
                        info,
                        "{} dBm / {}",
                        ap.rssi,
                        if ap.secure { "secured" } else { "open" }
                    );
                    label(canvas, &info, Point::new(22, y + 43), true);
                }
            }
            button(
                canvas,
                Rectangle::new(Point::new(12, 535), Size::new(100, 44)),
                "PREV",
                false,
            );
            button(
                canvas,
                Rectangle::new(Point::new(124, 535), Size::new(100, 44)),
                "NEXT",
                false,
            );
            button(
                canvas,
                Rectangle::new(Point::new(236, 535), Size::new(108, 44)),
                "DISCON",
                false,
            );
            label(
                canvas,
                "WiFi credentials stay in RAM",
                Point::new(12, 615),
                true,
            );
        }
        Page::Password => {
            label(canvas, "WIFI PASSWORD", Point::new(100, 33), false);
            ssid(canvas, &ui.selected_ssid, Point::new(12, 85));
            let mut mask = heapless::String::<32>::new();
            for _ in 0..ui.password.len().min(28) {
                let _ = mask.push('*');
            }
            label(canvas, &mask, Point::new(12, 135), false);
            label(canvas, "Password hidden / ASCII", Point::new(12, 165), true);
            for (k, &ch) in characters(ui).iter().enumerate() {
                let mut b = [0; 1];
                b[0] = ch;
                let text = core::str::from_utf8(&b).unwrap();
                button(
                    canvas,
                    Rectangle::new(
                        Point::new(10 + (k % 10) as i32 * 34, 205 + (k / 10) as i32 * 50),
                        Size::new(30, 44),
                    ),
                    text,
                    false,
                );
            }
            button(
                canvas,
                Rectangle::new(Point::new(10, 465), Size::new(76, 44)),
                "SHIFT",
                ui.shift,
            );
            button(
                canvas,
                Rectangle::new(Point::new(96, 465), Size::new(76, 44)),
                "SYM",
                ui.symbols,
            );
            button(
                canvas,
                Rectangle::new(Point::new(182, 465), Size::new(76, 44)),
                "SPACE",
                false,
            );
            button(
                canvas,
                Rectangle::new(Point::new(268, 465), Size::new(76, 44)),
                "DEL",
                false,
            );
            button(
                canvas,
                Rectangle::new(Point::new(10, 535), Size::new(334, 52)),
                "CONNECT",
                false,
            );
        }
        Page::Keyboard => {
            label(canvas, "USB MACROPAD", Point::new(100, 33), false);
            for l in 0..4 {
                let text = ["L0", "L1", "L2", "L3"][l];
                button(
                    canvas,
                    Rectangle::new(Point::new(16 + l as i32 * 86, 66), Size::new(70, 42)),
                    text,
                    l == ui.layer,
                );
            }
            for k in 0..8 {
                button(
                    canvas,
                    key_rect(k),
                    &keyboard::label(map.resolved(ui.layer, k)),
                    ui.pressed & (1 << k) != 0,
                );
            }
        }
        Page::Home => {}
    }
}

async fn render(fb: &mut [u8], ui: &Ui, view: &View, map: &Keymap, now: u64, full: bool) {
    let mut strip = [0; 360 * 16 * 3];
    let mut row = [0u32; 360];
    for y in (0..640).step_by(16) {
        if !full && y >= 112 {
            break;
        }
        for p in strip.chunks_exact_mut(3) {
            p.copy_from_slice(&[BG.r(), BG.g(), BG.b()]);
        }
        draw(&mut Canvas(&mut strip, y), ui, view, map, now);
        for line in 0..16 {
            for x in 0..360 {
                let i = (line * 360 + x) * 3;
                let c = ((strip[i + 2] as u16 >> 3) << 11)
                    | ((strip[i + 1] as u16 >> 2) << 5)
                    | (strip[i] as u16 >> 3);
                row[x] = c as u32 | ((c as u32) << 16);
            }
            for dy in 0..2 {
                let offset = ((y as usize + line) * 2 + dy) * 1440;
                // Static PSRAM frame and each row are aligned and exclusively
                // owned by this core until the frame has been committed.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        row.as_ptr(),
                        fb.as_mut_ptr().add(offset).cast::<u32>(),
                        360,
                    );
                }
            }
        }
        embassy_futures::yield_now().await;
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
    const SIZE: usize = 720 * 1280 * 2;
    assert!(len >= SIZE * 2);
    let fb = unsafe { core::slice::from_raw_parts_mut(ptr, SIZE * 2) };
    let mut ui = Ui {
        page: Page::Home,
        list_page: 0,
        selected_ssid: heapless::String::new(),
        password: heapless::String::new(),
        shift: false,
        symbols: false,
        layer: 0,
        pressed: 0,
    };
    let (view, map) = desktop::with(|s| (s.view.clone(), s.keymap.clone()));
    embassy_futures::block_on(render(&mut fb[..SIZE], &ui, &view, &map, 0, true));
    let (a, b) = fb.split_at_mut(SIZE);
    b.copy_from_slice(a);
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
    log::info!(
        "desktop: display owns core {:?}, RGB565 double-buffer 31 Hz",
        esp_hal::system::Cpu::current()
    );
    let mut touch = Gt911::probe(&mut i2c);
    log::info!("desktop: touch probe complete ready={}",touch.is_some());
    let mut last_probe = 0;
    let mut last_touch = 0;
    let mut down = false;
    let mut revision = u32::MAX;
    let mut map_revision = u32::MAX;
    let mut shown_second = 0;
    let mut repair_full = false;
    let mut held = [0u16; 8];
    let mut base_layer = 0usize;
    let mut old_mask = 0;
    let mut last_report = [0; 10];
    let mut player = MacroPlayer::default();
    let mut macro_due = 0;
    loop {
        if desktop::FLASH_REQUEST.load(core::sync::atomic::Ordering::Acquire) {
            desktop::park_for_flash();
        }
        let now = Instant::now().duration_since_epoch().as_millis();
        let mut changed = false;
        if touch.is_none() && now - last_probe > 1000 {
            last_probe = now;
            touch = Gt911::probe(&mut i2c);
        }
        if revision == u32::MAX { log::info!("desktop: before touch poll"); }
        let points = touch.as_mut().and_then(|t| t.poll(&mut i2c));
        let (view, map) = desktop::with(|s| (s.view.clone(), s.keymap.clone()));
        if revision == u32::MAX { log::info!("desktop: shared snapshot ready"); }
        let mut mask = old_mask;
        if let Some(points) = points {
            last_touch = now;
            mask = 0;
            if ui.page == Page::Keyboard {
                for p in &points {
                    let p = Point::new(p.position.x / 2, p.position.y / 2);
                    for k in 0..8 {
                        if key_rect(k).contains(p) {
                            mask |= 1 << k;
                        }
                    }
                }
            }
            if !down {
                if let Some(contact) = points.first() {
                    let p = Point::new(contact.position.x / 2, contact.position.y / 2);
                    let hit =
                        |x, y, w, h| Rectangle::new(Point::new(x, y), Size::new(w, h)).contains(p);
                    if ui.page != Page::Home && hit(10, 8, 74, 34) {
                        ui.page = if ui.page == Page::Settings {
                            Page::Home
                        } else {
                            Page::Settings
                        };
                        ui.password.clear();
                        changed = true;
                    } else {
                        match ui.page {
                            Page::Home => {
                                if hit(306, 8, 44, 32) {
                                    ui.page = Page::Settings;
                                    desktop::with(|s| {
                                        if s.command.is_none() {
                                            s.command = Some(Command::Scan);
                                        }
                                    });
                                    changed = true;
                                }
                            }
                            Page::Settings => {
                                if hit(12, 60, 156, 42) {
                                    desktop::with(|s| {
                                        if s.command.is_none() {
                                            s.command = Some(Command::Scan);
                                        }
                                    });
                                }
                                if hit(188, 60, 156, 42) {
                                    ui.page = Page::Keyboard;
                                    changed = true;
                                }
                                for row in 0..6 {
                                    let index = ui.list_page * 6 + row;
                                    if hit(12, 148 + row as i32 * 60, 332, 52)
                                        && index < view.networks.len()
                                    {
                                        ui.selected_ssid = view.networks[index].ssid.clone();
                                        ui.password.clear();
                                        ui.page = Page::Password;
                                        changed = true;
                                    }
                                }
                                if hit(12, 535, 100, 44) {
                                    ui.list_page = ui.list_page.saturating_sub(1);
                                    changed = true;
                                }
                                if hit(124, 535, 100, 44)
                                    && (ui.list_page + 1) * 6 < view.networks.len()
                                {
                                    ui.list_page += 1;
                                    changed = true;
                                }
                                if hit(236, 535, 108, 44) {
                                    desktop::with(|s| {
                                        s.command = Some(Command::Disconnect);
                                    });
                                }
                            }
                            Page::Password => {
                                for (k, &c) in characters(&ui).iter().enumerate() {
                                    if hit(
                                        10 + (k % 10) as i32 * 34,
                                        205 + (k / 10) as i32 * 50,
                                        30,
                                        44,
                                    ) {
                                        let _ = ui.password.push(c as char);
                                        changed = true;
                                    }
                                }
                                if hit(10, 465, 76, 44) {
                                    ui.shift = !ui.shift;
                                    changed = true;
                                }
                                if hit(96, 465, 76, 44) {
                                    ui.symbols = !ui.symbols;
                                    changed = true;
                                }
                                if hit(182, 465, 76, 44) {
                                    let _ = ui.password.push(' ');
                                    changed = true;
                                }
                                if hit(268, 465, 76, 44) {
                                    ui.password.pop();
                                    changed = true;
                                }
                                if hit(10, 535, 334, 52) {
                                    if !ui.selected_ssid.is_empty() {
                                        desktop::with(|s| {
                                            s.command = Some(Command::Connect {
                                                ssid: ui.selected_ssid.clone(),
                                                password: ui.password.clone(),
                                            });
                                        });
                                    }
                                    unsafe {
                                        ui.password.as_bytes_mut().fill(0);
                                    }
                                    ui.password.clear();
                                    ui.page = Page::Settings;
                                    changed = true;
                                }
                            }
                            Page::Keyboard => {
                                for l in 0..4 {
                                    if hit(16 + l * 86, 66, 70, 42) {
                                        base_layer = l as usize;
                                        ui.layer = base_layer;
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            down = !points.is_empty();
        } else if now - last_touch > 250 {
            down = false;
            mask = 0;
        }
        if ui.page != Page::Keyboard {
            mask = 0;
            player.active = false;
        }
        for k in 0..8 {
            if mask & (1 << k) == 0 {
                held[k] = 0;
            }
        }
        let previous_layer = ui.layer;
        ui.layer = held
            .iter()
            .filter(|c| (0x5220..=0x5223).contains(*c))
            .fold(base_layer, |l, c| l.max((c & 3) as usize));
        let rising = mask & !old_mask;
        for k in 0..8 {
            if rising & (1 << k) == 0 {
                continue;
            }
            let code = map.resolved(ui.layer, k);
            held[k] = code;
            player.active = false;
            match code {
                0x7700..=0x7707 => {
                    player.start(&map, (code - 0x7700) as usize);
                    macro_due = now;
                }
                0x5220..=0x5223 => {
                    ui.layer = ui.layer.max((code & 3) as usize);
                }
                0x5200..=0x5203 | 0x5240..=0x5243 => {
                    base_layer = (code & 3) as usize;
                    ui.layer = base_layer;
                }
                0x5260..=0x5263 => {
                    base_layer = if base_layer == (code & 3) as usize {
                        0
                    } else {
                        (code & 3) as usize
                    };
                    ui.layer = base_layer;
                }
                _ => {}
            }
        }
        changed |= previous_layer != ui.layer;
        let report = if player.active {
            if now >= macro_due {
                match player.next_step() {
                    MacroStep::Report(r) => {
                        macro_due = now + 8;
                        Some(r)
                    }
                    MacroStep::Delay(ms) => {
                        macro_due = now + ms as u64;
                        None
                    }
                }
            } else {
                None
            }
        } else {
            Some(keyboard::report(&held))
        };
        if let Some(report) = report {
            if report != last_report {
                desktop::with(|s| s.report(report));
                last_report = report;
            }
        }
        desktop::with(|s| s.pressed = mask);
        if old_mask != mask {
            changed = true;
        }
        ui.pressed = mask;
        old_mask = mask;
        let second = now / 1000;
        if changed
            || view.revision != revision
            || map.revision != map_revision
            || (ui.page == Page::Home && second != shown_second)
        {
            let draw_started = Instant::now();
            log::debug!("desktop: draw begin");
            let full = changed || ui.page != Page::Home;
            embassy_futures::block_on(async {
                render(
                    dpi.framebuffer_mut(),
                    &ui,
                    &view,
                    &map,
                    second,
                    full || repair_full,
                )
                .await;
                let rendered = Instant::now();
                dpi.commit();
                let committed = Instant::now();
                for _ in 0..3 {
                    dpi.wait_for_vsync_async().await;
                }
                if changed {
                    log::info!("desktop: frame render={}ms flush={}ms total={}ms",
                        (rendered - draw_started).as_millis(),
                        (committed - rendered).as_millis(),
                        (Instant::now() - draw_started).as_millis());
                }
            });
            repair_full = full;
            revision = view.revision;
            map_revision = map.revision;
            shown_second = second;
        }
        esp_hal::delay::Delay::new().delay_millis(8);
    }
}
// A 16-row logical strip lives in internal RAM. Never issue individual pixel
// stores into PSRAM: uncached byte writes can block USB for several seconds.
struct Canvas<'a>(&'a mut [u8], i32);
impl OriginDimensions for Canvas<'_> {
    fn size(&self) -> Size {
        Size::new(360, 640)
    }
}
impl DrawTarget for Canvas<'_> {
    type Color = Rgb888;
    type Error = Infallible;
    fn fill_solid(&mut self, area: &Rectangle, color: Rgb888) -> Result<(), Infallible> {
        let clipped = area.intersection(&Rectangle::new(Point::new(0, self.1), Size::new(360, 16)));
        for y in clipped.top_left.y..clipped.top_left.y + clipped.size.height as i32 {
            let start = ((y - self.1) as usize * 360 + clipped.top_left.x as usize) * 3;
            for pixel in self.0[start..start + clipped.size.width as usize * 3].chunks_exact_mut(3)
            {
                pixel.copy_from_slice(&[color.r(), color.g(), color.b()]);
            }
        }
        Ok(())
    }
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb888>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            if !(0..360).contains(&p.x) || !(self.1..self.1 + 16).contains(&p.y) {
                continue;
            }
            let i = ((p.y - self.1) as usize * 360 + p.x as usize) * 3;
            self.0[i..i + 3].copy_from_slice(&[c.r(), c.g(), c.b()]);
        }
        Ok(())
    }
}
