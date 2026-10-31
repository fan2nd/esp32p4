#![no_std]
#![no_main]

use core::{
    cell::{Cell, RefCell},
    convert::Infallible,
};
use embassy_futures::{block_on, join::join, yield_now};
use embassy_usb::class::hid::{Config as HidConfig, HidReaderWriter, HidWriter, State};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
    text::{Alignment, Text},
};
use esp_hal::{
    i2c::master::{Config as I2cConfig, I2c},
    psram::{Psram, PsramConfig},
    time::Instant,
    usb::otg::{
        Usb,
        embassy_usb_device::{Config as DriverConfig, Driver},
    },
};
use esp_storage::FlashStorage;
use panic_halt as _;
use riscv_rt::entry;
use waveshare_p4_eth::{
    driver::{display, touch::Gt911},
    keyboard::{self, Keymap, MacroPlayer, MacroStep, RECORD},
};

esp_bootloader_esp_idf::esp_app_desc!();

// Three independent HID interfaces: keyboard, consumer controls and VIA Raw HID.
const KEYBOARD_DESC: &[u8] = &[
    0x05, 1, 0x09, 6, 0xa1, 1, 0x05, 7, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0, 0x25, 1, 0x75, 1, 0x95, 8,
    0x81, 2, 0x75, 8, 0x95, 1, 0x81, 1, 0x19, 0, 0x29, 0xff, 0x15, 0, 0x26, 0xff, 0, 0x75, 8, 0x95,
    6, 0x81, 0, 0xc0,
];
const CONSUMER_DESC: &[u8] = &[
    0x05, 0x0c, 0x09, 1, 0xa1, 1, 0x15, 0, 0x26, 0xff, 3, 0x19, 0, 0x2a, 0xff, 3, 0x75, 16, 0x95,
    1, 0x81, 0, 0xc0,
];
const RAW_DESC: &[u8] = &[
    0x06, 0x60, 0xff, 0x09, 0x61, 0xa1, 1, 0x09, 0x62, 0x15, 0, 0x26, 0xff, 0, 0x75, 8, 0x95, 32,
    0x81, 2, 0x09, 0x63, 0x95, 32, 0x91, 2, 0xc0,
];
const BG: Rgb888 = Rgb888::new(12, 18, 28);
const ACCENT: Rgb888 = Rgb888::new(65, 210, 220);

struct UsbStatus<'a>(&'a Cell<bool>);
impl embassy_usb::Handler for UsbStatus<'_> {
    fn reset(&mut self) {
        self.0.set(false);
    }
    fn configured(&mut self, value: bool) {
        self.0.set(value);
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
async fn render(
    framebuffer: &mut [u8],
    codes: [u16; 8],
    layer: usize,
    pressed: u8,
    online: bool,
    storage: &str,
    dirty_keys: u8,
    header: bool,
    full: bool,
) {
    let mut draw_us = 0;
    let mut copy_us = 0;
    let mut strip = [0u8; 360 * 16 * 3];
    // Word-aligned row staging guarantees efficient 32-bit PSRAM writes.
    let mut row = [0u32; 360];
    for y in (0..640).step_by(16) {
        let area = Rectangle::new(Point::new(0, y), Size::new(360, 16));
        let keys = (0..8).fold(0u8, |mask, k| {
            if key_rect(k).intersection(&area).size.height > 0 {
                mask | (1 << k)
            } else {
                mask
            }
        });
        if !full && keys & dirty_keys == 0 && !(header && (y < 125 || y + 16 > 590)) {
            continue;
        }
        let draw_start = Instant::now();
        for pixel in strip.chunks_exact_mut(3) {
            pixel.copy_from_slice(&[BG.r(), BG.g(), BG.b()]);
        }
        let mut canvas = Canvas(&mut strip, y);
        if y < 125 || y + 16 > 590 {
            draw_header(&mut canvas, layer, online, storage);
        }
        for k in 0..8 {
            if keys & (1 << k) != 0 {
                draw_key(&mut canvas, k, codes[k], pressed & (1 << k) != 0);
            }
        }
        draw_us += draw_start.elapsed().as_micros();
        let copy_start = Instant::now();
        for line in 0..16 {
            // This bridge/panel path needs B in bits 15:11 and R in bits 4:0
            // for its RGB565 input; RGB888's byte order is not interchangeable.
            // Duplicate the packed pixel to produce 2x horizontal scaling.
            for x in 0..360 {
                let i = (line * 360 + x) * 3;
                let color = ((strip[i + 2] as u16 >> 3) << 11)
                    | ((strip[i + 1] as u16 >> 2) << 5)
                    | (strip[i] as u16 >> 3);
                row[x] = color as u32 | ((color as u32) << 16);
            }
            for dy in 0..2 {
                let offset = ((y as usize + line) * 2 + dy) * 1440;
                // PSRAM allocation and row stride are 4-byte aligned; all 360
                // destination words lie inside this exclusive framebuffer slice.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        row.as_ptr(),
                        framebuffer.as_mut_ptr().add(offset).cast::<u32>(),
                        360,
                    );
                }
            }
        }
        // USB must keep draining Raw HID even during a complete layer redraw.
        copy_us += copy_start.elapsed().as_micros();
        yield_now().await;
    }
    esp_println::println!("keyboard: draw={} us copy={} us", draw_us, copy_us);
}
fn key_rect(key: usize) -> Rectangle {
    Rectangle::new(
        Point::new(16 + (key % 2) as i32 * 172, 138 + (key / 2) as i32 * 110),
        Size::new(156, 99),
    )
}
fn draw_key(canvas: &mut Canvas<'_>, key: usize, code: u16, pressed: bool) {
    let r = key_rect(key);
    let color = if pressed {
        ACCENT
    } else {
        Rgb888::new(28, 42, 61)
    };
    RoundedRectangle::with_equal_corners(r, Size::new(14, 14))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(canvas)
        .unwrap();
    let name = keyboard::label(code);
    let text_color = if pressed { BG } else { Rgb888::WHITE };
    Text::with_alignment(
        &name,
        r.top_left + Point::new(78, 57),
        MonoTextStyle::new(&FONT_10X20, text_color),
        Alignment::Center,
    )
    .draw(canvas)
    .unwrap();
}
fn draw_header(canvas: &mut Canvas<'_>, layer: usize, online: bool, storage: &str) {
    Rectangle::new(Point::zero(), Size::new(360, 125))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(canvas)
        .unwrap();
    Text::new(
        "P4 / MACROPAD",
        Point::new(16, 35),
        MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE),
    )
    .draw(canvas)
    .unwrap();
    for i in 0..4 {
        let r = Rectangle::new(Point::new(16 + i * 86, 66), Size::new(70, 42));
        RoundedRectangle::with_equal_corners(r, Size::new(8, 8))
            .into_styled(PrimitiveStyle::with_fill(if i as usize == layer {
                ACCENT
            } else {
                Rgb888::new(28, 42, 61)
            }))
            .draw(canvas)
            .unwrap();
        Text::with_alignment(
            ["L0", "L1", "L2", "L3"][i as usize],
            r.top_left + Point::new(35, 28),
            MonoTextStyle::new(
                &FONT_10X20,
                if i as usize == layer {
                    BG
                } else {
                    Rgb888::WHITE
                },
            ),
            Alignment::Center,
        )
        .draw(canvas)
        .unwrap();
    }
    Rectangle::new(Point::new(0, 590), Size::new(360, 50))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(canvas)
        .unwrap();
    Text::new(
        if online { "USB READY" } else { "USB OFFLINE" },
        Point::new(16, 615),
        MonoTextStyle::new(&FONT_10X20, ACCENT),
    )
    .draw(canvas)
    .unwrap();
    Text::with_alignment(
        storage,
        Point::new(344, 615),
        MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE),
        Alignment::Right,
    )
    .draw(canvas)
    .unwrap();
}

#[entry]
fn main() -> ! {
    // The default P4 preset is 100 MHz; use the rev3.1 400 MHz preset so
    // display DMA and the USB/UI tasks have sufficient CPU and memory bandwidth.
    let p =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    let mut flash = FlashStorage::new(p.FLASH);
    let mut table = [0; 0xc00];
    // Never write into an app partition when flashed without our dedicated table.
    let writable = flash.read(0x8000, &mut table).is_ok()
        && table.chunks_exact(32).any(|e| {
            e[..4] == [0xaa, 0x50, 1, 2]
                && e[4..8] == 0xff0000u32.to_le_bytes()
                && e[8..12] == 0x2000u32.to_le_bytes()
                && &e[12..19] == b"kbdmap\0"
        });
    let mut initial = Keymap::default();
    let mut generation = 0u32;
    let mut active_slot = 1;
    let mut restored = false;
    if writable {
        for slot in 0..2 {
            let mut record = [0; RECORD];
            if flash.read(0xff0000 + slot * 4096, &mut record).is_ok() {
                if let Some((map, seq)) = Keymap::restore(&record) {
                    if !restored || (seq.wrapping_sub(generation) as i32) > 0 {
                        initial = map;
                        generation = seq;
                        active_slot = slot;
                        restored = true;
                    }
                }
            }
        }
    }
    let psram = Psram::new(p.PSRAM, PsramConfig::default());
    let (ptr, len) = psram.raw_parts();
    const FRAME_SIZE: usize = 720 * 1280 * 2;
    assert!(len >= FRAME_SIZE * 2);
    let framebuffer = unsafe { core::slice::from_raw_parts_mut(ptr, FRAME_SIZE * 2) };
    block_on(render(
        &mut framebuffer[..FRAME_SIZE],
        core::array::from_fn(|k| initial.resolved(0, k)),
        0,
        0,
        false,
        if writable { "SAVED" } else { "NO SAVE" },
        255,
        true,
        true,
    ));
    // Both buffers start with identical content. Subsequent flips repair only
    // the regions changed since that particular back buffer was last displayed.
    let (front, back) = framebuffer.split_at_mut(FRAME_SIZE);
    back.copy_from_slice(front);
    let i2c = I2c::new(p.I2C1, I2cConfig::default())
        .unwrap()
        .with_sda(p.GPIO7)
        .with_scl(p.GPIO8);
    let (mut dpi, mut i2c) = display::init(
        p.MIPI_DSI,
        p.VDMA_CH0,
        i2c,
        framebuffer,
        esp_hal::mipi_dsi::dpi::DpiConfig {
            in_color_format: esp_hal::mipi_dsi::dpi::ColorFormat::Rgb565,
            // Keep the proven horizontal timing; extra vertical blanking reduces
            // average PSRAM scanout traffic. Host VFP is a 10-bit field.
            timing: esp_hal::mipi_dsi::dpi::FrameTiming {
                vfp: if cfg!(feature = "keyboard-low-refresh") {
                    1023
                } else {
                    24
                },
                ..display::DEFAULT_DPI_CONFIG.timing
            },
            ..display::DEFAULT_DPI_CONFIG
        },
    );
    esp_println::println!(
        "keyboard: RGB565 framebuffer, RGB888 wire, low_refresh={}",
        cfg!(feature = "keyboard-low-refresh")
    );
    // Measure frame spacing instead of assuming the requested timing was applied.
    unsafe {
        riscv::interrupt::enable();
    }
    let measured = block_on(embassy_futures::select::select(
        async {
            dpi.wait_for_vsync_async().await;
            let start = Instant::now();
            for _ in 0..16 {
                dpi.wait_for_vsync_async().await;
            }
            start.elapsed().as_micros() / 16
        },
        async {
            let start = Instant::now();
            while start.elapsed().as_millis() < 2000 {
                yield_now().await;
            }
        },
    ));
    match measured {
        embassy_futures::select::Either::First(us) => {
            esp_println::println!("keyboard: measured frame={} us", us)
        }
        _ => esp_println::println!("keyboard: VSYNC measurement timed out"),
    }
    let mut touch = Gt911::probe(&mut i2c);
    // Faster RGB565 startup may reach the controller before its power-on boot
    // completes. Retry boundedly instead of permanently disabling touch.
    for _ in 0..10 {
        if touch.is_some() {
            break;
        }
        esp_hal::delay::Delay::new().delay_millis(100);
        touch = Gt911::probe(&mut i2c);
    }
    esp_println::println!(
        "keyboard: 8 keys; GT911 {}; storage={} restored={} generation={}",
        if touch.is_some() {
            "ready"
        } else {
            "not found"
        },
        writable,
        restored,
        generation
    );

    let map = RefCell::new(initial);
    let pressed = Cell::new(0u8);
    let online = Cell::new(false);
    let desired = Cell::new([0; 10]);
    let queue = RefCell::new(heapless::Deque::<[u8; 10], 32>::new());
    let mut status = UsbStatus(&online);
    let mut ep_buffer = [0; 256];
    let mut driver_config = DriverConfig::default();
    driver_config.force_full_speed = true;
    let driver = Driver::new(Usb::new_hs(p.USB_HS), &mut ep_buffer, driver_config);
    let mut config = embassy_usb::Config::new(0xcafe, 0x4013);
    config.manufacturer = Some("P4 Lab");
    config.product = Some("P4 Touch Macropad");
    config.serial_number = Some("P4-KEYS-001");
    config.self_powered = true;
    config.max_power = 0;
    let mut descriptors = [0; 512];
    let mut bos = [0; 64];
    let mut msos = [0; 64];
    let mut control = [0; 64];
    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        &mut descriptors,
        &mut bos,
        &mut msos,
        &mut control,
    );
    builder.handler(&mut status);
    let mut keyboard_state = State::new();
    let mut consumer_state = State::new();
    let mut raw_state = State::new();
    let mut keyboard = HidWriter::<_, 8>::new(
        &mut builder,
        &mut keyboard_state,
        HidConfig {
            report_descriptor: KEYBOARD_DESC,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 8,
        },
    );
    let mut consumer = HidWriter::<_, 2>::new(
        &mut builder,
        &mut consumer_state,
        HidConfig {
            report_descriptor: CONSUMER_DESC,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 2,
        },
    );
    let (mut raw_in, mut raw_out) = HidReaderWriter::<_, 32, 32>::new(
        &mut builder,
        &mut raw_state,
        HidConfig {
            report_descriptor: RAW_DESC,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 32,
        },
    )
    .split();
    let mut usb = builder.build();
    let via = async {
        let mut packet = [0; 32];
        loop {
            raw_in.ready().await;
            match raw_in.read(&mut packet).await {
                Ok(32) => {
                    map.borrow_mut().via(
                        &mut packet,
                        Instant::now().duration_since_epoch().as_millis() as u32,
                        pressed.get(),
                    );
                    let _ = raw_out.write(&packet).await;
                }
                _ => {
                    yield_now().await;
                }
            }
        }
    };
    let reports = async {
        loop {
            keyboard.ready().await;
            consumer.ready().await;
            queue.borrow_mut().clear();
            let mut current = desired.get();
            loop {
                if keyboard.write(&current[..8]).await.is_err() {
                    break;
                }
                if consumer.write(&current[8..]).await.is_err() {
                    break;
                }
                loop {
                    let next = queue.borrow_mut().pop_front();
                    if let Some(next) = next {
                        current = next;
                        break;
                    }
                    if !online.get() {
                        break;
                    }
                    yield_now().await;
                }
                if !online.get() {
                    break;
                }
            }
        }
    };
    let ui = async {
        let mut held = [0u16; 8];
        let mut old_pressed = 0u8;
        let mut base_layer = 0usize;
        let mut shown_layer = 0usize;
        let mut tab_down = false;
        let mut macro_player = MacroPlayer::default();
        let mut macro_deadline = 0u64;
        let mut observed_revision = 0;
        let mut saved_revision = 0;
        let mut last_change = 0u64;
        let mut save_error = false;
        let mut shown_online = false;
        let mut shown_dirty = false;
        let mut shown_error = false;
        let mut shown_codes = core::array::from_fn::<_, 8, _>(|k| map.borrow().resolved(0, k));
        let mut repair_keys = 0u8;
        let mut repair_header = false;
        let mut last_poll = 0u64;
        let mut last_probe = 0u64;
        let mut last_touch = 0u64;
        loop {
            yield_now().await;
            let now = Instant::now().duration_since_epoch().as_millis();
            if now - last_poll < 8 {
                continue;
            }
            last_poll = now;
            // A controller that was absent at boot must be able to recover.
            if touch.is_none() && now - last_probe >= 1000 {
                last_probe = now;
                touch = Gt911::probe(&mut i2c);
                if touch.is_some() {
                    esp_println::println!("keyboard: GT911 recovered");
                } else if now < 15000 {
                    for address in [0x14, 0x5d] {
                        let mut id = [0u8; 4];
                        let result = i2c.write_read(address, &[0x81, 0x40], &mut id);
                        esp_println::println!(
                            "keyboard: touch probe {:02x} {:?} id={:?}",
                            address,
                            result,
                            id
                        );
                    }
                }
            }
            let mut mask = old_pressed;
            if let Some(points) = touch.as_mut().and_then(|t| t.poll(&mut i2c)) {
                last_touch = now;
                mask = 0;
                let mut tab = None;
                for point in points {
                    let p = Point::new(point.position.x / 2, point.position.y / 2);
                    for k in 0..8 {
                        if key_rect(k).contains(p) {
                            mask |= 1 << k;
                        }
                    }
                    for i in 0..4 {
                        if Rectangle::new(Point::new(16 + i * 86, 66), Size::new(70, 42))
                            .contains(p)
                        {
                            tab = Some(i as usize);
                        }
                    }
                }
                if !tab_down {
                    if let Some(layer) = tab {
                        base_layer = layer;
                        macro_player.active = false;
                    }
                }
                tab_down = tab.is_some();
            } else if now - last_touch > 250 {
                mask = 0;
                tab_down = false;
            }
            for k in 0..8 {
                if mask & (1 << k) == 0 {
                    held[k] = 0;
                }
            }
            let mut layer = base_layer;
            for &code in &held {
                if (0x5220..=0x5223).contains(&code) {
                    layer = layer.max((code & 3) as usize);
                }
            }
            let rising = mask & !old_pressed;
            if rising != 0 {
                macro_player.active = false;
            }
            for k in 0..8 {
                if rising & (1 << k) == 0 {
                    continue;
                }
                let code = map.borrow().resolved(layer, k);
                held[k] = code;
                match code {
                    0x5220..=0x5223 => {
                        layer = layer.max((code & 3) as usize);
                    }
                    0x5200..=0x5203 | 0x5240..=0x5243 => {
                        base_layer = (code & 3) as usize;
                        layer = base_layer;
                    }
                    0x5260..=0x5263 => {
                        base_layer = if base_layer == (code & 3) as usize {
                            0
                        } else {
                            (code & 3) as usize
                        };
                        layer = base_layer;
                    }
                    0x7700..=0x7707 => {
                        macro_player.start(&map.borrow(), (code - 0x7700) as usize);
                        macro_deadline = now;
                    }
                    _ => {}
                }
                esp_println::println!(
                    "keyboard: press key={} layer={} code={:04x}",
                    k,
                    layer,
                    code
                );
            }
            pressed.set(mask);
            let next = if macro_player.active {
                if now >= macro_deadline {
                    match macro_player.next_step() {
                        MacroStep::Report(r) => {
                            macro_deadline = now + 8;
                            Some(r)
                        }
                        MacroStep::Delay(ms) => {
                            macro_deadline = now + ms as u64;
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                Some(keyboard::report(&held))
            };
            if let Some(next) = next {
                if desired.get() != next {
                    desired.set(next);
                    if online.get() {
                        let mut q = queue.borrow_mut();
                        // Overflow recovery always releases old keys before applying current state.
                        if q.is_full() {
                            q.clear();
                            q.push_back([0; 10]).unwrap();
                        }
                        q.push_back(next).unwrap();
                    }
                }
            }
            let revision = map.borrow().revision;
            let changed = revision != observed_revision;
            if changed {
                observed_revision = revision;
                last_change = now;
            }
            if writable
                && revision != saved_revision
                && map.borrow().bytes[keyboard::PAYLOAD - 1] == 0
                && now - last_change > 1500
                && mask == 0
                && !macro_player.active
            {
                let record = map.borrow().record(generation.wrapping_add(1));
                let slot = 1 - active_slot;
                let mut verify = [0; RECORD];
                save_error = flash.write(0xff0000 + slot * 4096, &record).is_err()
                    || flash.read(0xff0000 + slot * 4096, &mut verify).is_err()
                    || verify != record;
                if !save_error {
                    generation = generation.wrapping_add(1);
                    active_slot = slot;
                    saved_revision = revision;
                }
                last_change = now;
                esp_println::println!(
                    "keyboard: save {} generation={}",
                    if save_error { "failed" } else { "ok" },
                    generation
                );
            }
            let dirty = revision != saved_revision;
            let header_changed = layer != shown_layer
                || online.get() != shown_online
                || dirty != shown_dirty
                || save_error != shown_error;
            if header_changed || changed || mask != old_pressed {
                let render_start = Instant::now();
                let codes = core::array::from_fn(|k| map.borrow().resolved(layer, k));
                let dirty_keys = (0..8).fold(mask ^ old_pressed, |bits, k| {
                    if codes[k] != shown_codes[k] {
                        bits | (1 << k)
                    } else {
                        bits
                    }
                });
                render(
                    dpi.framebuffer_mut(),
                    codes,
                    layer,
                    mask,
                    online.get(),
                    if !writable {
                        "NO SAVE"
                    } else if save_error {
                        "ERROR"
                    } else if dirty {
                        "EDITED"
                    } else {
                        "SAVED"
                    },
                    dirty_keys | repair_keys,
                    header_changed || repair_header,
                    false,
                )
                .await;
                shown_codes = codes;
                dpi.commit();
                // HAL latches a source at each frame block, and flushes LLI
                // changes from its block-done ISR. Allow two boundaries for an
                // already-prefetched old descriptor before reusing the old frame.
                // The first wait may only consume a VSYNC latched during drawing.
                dpi.wait_for_vsync_async().await;
                dpi.wait_for_vsync_async().await;
                dpi.wait_for_vsync_async().await;
                repair_keys = dirty_keys;
                repair_header = header_changed;
                esp_println::println!("keyboard: render {} ms", render_start.elapsed().as_millis());
            }
            shown_layer = layer;
            shown_online = online.get();
            shown_dirty = dirty;
            shown_error = save_error;
            old_pressed = mask;
        }
    };
    unsafe {
        riscv::interrupt::enable();
    }
    block_on(join(usb.run(), join(via, join(reports, ui))));
    loop {
        core::hint::spin_loop();
    }
}
