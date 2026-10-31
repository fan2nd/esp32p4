//! Allocation-free desktop page composition. Hardware and commands stay outside the UI.
use crate::password_keyboard::{self, Password};
use core::fmt::Write;
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use heapless::{String, Vec};
use tiny_ui::{ActionKind, Button, Event, ListView, Panel, Scene, Style, Widget};
pub const WIDTH: u32 = 360;
pub const HEIGHT: u32 = 640;
const WHALE: &[u8] = include_bytes!("../assets/whale-girl-wave-large.rgb");
const CJK: &[u8] = include_bytes!("../assets/cjk16.bin");
const EMPTY_ROWS: [&str; 64] = [""; 64];
const BACK: u16 = 1;
const SETTINGS: u16 = 2;
const KEYBOARD: u16 = 3;
const SCAN: u16 = 4;
const DISCONNECT: u16 = 5;
const CONNECT: u16 = 6;
const NETWORKS: u16 = 7;
const TYPING: u16 = 100;
fn rect(x: i32, y: i32, w: u32, h: u32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(w, h))
}
fn rgb(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}
fn background() -> Rgb565 {
    rgb(0xe8, 0xf7, 0xff)
}
fn ink() -> Rgb565 {
    rgb(0x18, 0x2d, 0x44)
}
fn accent() -> Rgb565 {
    rgb(0x26, 0x7b, 0x9a)
}
fn style() -> Style {
    Style {
        background: Rgb565::WHITE,
        foreground: ink(),
        accent: accent(),
        border: rgb(0xcd, 0xe4, 0xef),
        pressed: rgb(0xc1, 0xea, 0xfa),
        radius: 8,
        padding: 4,
        align: Alignment::Center,
        ..Style::default()
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Network {
    pub ssid: String<32>,
    pub rssi: i32,
    pub secure: bool,
}
pub enum Action {
    Scan,
    Disconnect,
    Connect {
        ssid: String<32>,
        password: String<64>,
    },
}
pub struct Ui {
    scene: Scene<72>,
    page: u8,
    networks: Vec<Network, 64>,
    password: Password,
    selected: String<32>,
    busy: bool,
    clock: String<16>,
    ip: String<32>,
    message: String<96>,
    wifi: bool,
    ble: bool,
    usb: bool,
    macro_labels: [String<24>; 8],
    pressed: u8,
}
impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
impl Ui {
    pub fn new() -> Self {
        let mut v = Self {
            scene: Scene::new(rect(0, 0, WIDTH, HEIGHT), background()),
            page: 0,
            networks: Vec::new(),
            password: Password::default(),
            selected: String::new(),
            busy: false,
            clock: String::try_from("--:--:--").unwrap(),
            ip: String::new(),
            message: String::new(),
            wifi: false,
            ble: false,
            usb: false,
            macro_labels: core::array::from_fn(|_| String::new()),
            pressed: 0,
        };
        v.rebuild();
        v
    }
    pub fn page(&self) -> u8 {
        self.page
    }
    pub fn dirty(&self) -> Option<Rectangle> {
        self.scene.damage()
    }
    fn invalidate(&mut self, r: Rectangle) {
        self.scene.invalidate(Some(r));
    }
    fn button(&mut self, id: u16, r: Rectangle, label: &'static str) {
        let mut b = Button::new(id, r, label);
        b.base.style = style();
        self.scene.add(b).unwrap();
    }
    fn rebuild(&mut self) {
        // Preserve the fixed storage: a replacement Scene temporarily duplicates
        // its 72 inline widgets and overflows the 64 KiB core-1 startup stack.
        self.scene.clear();
        if self.page != 0 {
            self.button(BACK, rect(10, 8, 74, 36), "BACK");
        }
        match self.page {
            0 => {
                self.button(SETTINGS, rect(296, 8, 54, 36), "SET");
                self.button(KEYBOARD, rect(70, 566, 220, 50), "KEYBOARD");
            }
            1 => {
                self.button(SCAN, rect(12, 60, 336, 44), "SCAN WIFI");
                let mut list = ListView::new(
                    NETWORKS,
                    rect(12, 150, 336, 374),
                    &EMPTY_ROWS[..self.networks.len()],
                );
                list.row_height = 70;
                list.selected = usize::MAX;
                list.base.style = Style {
                    background: Rgb565::WHITE,
                    alternate: Rgb565::WHITE,
                    padding: 0,
                    ..style()
                };
                self.scene.add(list).unwrap();
                self.button(DISCONNECT, rect(12, 542, 336, 44), "DISCONNECT");
            }
            2 => {
                let mut panel = Panel::new(8, rect(12, 112, 336, 52));
                panel.base.style = style();
                self.scene.add(panel).unwrap();
                for (i, k) in password_keyboard::layout().iter().enumerate() {
                    self.button(TYPING + i as u16, typing_bounds(k), "");
                }
                self.button(CONNECT, rect(12, 542, 336, 50), "CONNECT");
            }
            3 => {
                for k in 0..8 {
                    let mut p = Panel::new(20 + k as u16, macro_bounds(k));
                    p.base.style = Style {
                        background: if self.pressed & (1 << k) != 0 {
                            rgb(0x26, 0x91, 0xb5)
                        } else {
                            rgb(0xcd, 0xe4, 0xef)
                        },
                        ..style()
                    };
                    self.scene.add(p).unwrap();
                }
            }
            _ => {}
        }
    }
    pub fn navigate(&mut self, page: u8) -> Option<Action> {
        if page > 3 {
            return None;
        }
        self.password.clear();
        self.busy = false;
        self.page = page;
        self.rebuild();
        if page == 1 { Some(Action::Scan) } else { None }
    }
    pub fn handle(&mut self, event: Event) -> Option<Action> {
        if event == Event::KeyDown(tiny_ui::Key::Escape) {
            return self.navigate(if self.page == 2 { 1 } else { 0 });
        }
        let action = self.scene.handle(event)?;
        match (action.id, action.kind) {
            (BACK, ActionKind::Clicked) => self.navigate(if self.page == 2 { 1 } else { 0 }),
            (SETTINGS, ActionKind::Clicked) => self.navigate(1),
            (KEYBOARD, ActionKind::Clicked) => self.navigate(3),
            (SCAN, ActionKind::Clicked) => Some(Action::Scan),
            (DISCONNECT, ActionKind::Clicked) => Some(Action::Disconnect),
            (CONNECT, ActionKind::Clicked) => self.connect(),
            (NETWORKS, ActionKind::SelectionChanged(index)) => {
                if let Some(ap) = self.networks.get(index) {
                    self.selected = ap.ssid.clone();
                    self.password.clear();
                    self.busy = false;
                    self.page = 2;
                    self.rebuild();
                }
                None
            }
            (id, ActionKind::Clicked) if id >= TYPING => {
                let layout = password_keyboard::layout();
                if let Some(k) = layout.get((id - TYPING) as usize) {
                    let enter = self.password.press(k.action);
                    self.busy = false;
                    self.invalidate(rect(12, 112, 336, 100));
                    self.invalidate(rect(8, 238, 344, 264));
                    if enter {
                        return self.connect();
                    }
                }
                None
            }
            _ => None,
        }
    }
    fn connect(&self) -> Option<Action> {
        if self.selected.is_empty() {
            None
        } else {
            Some(Action::Connect {
                ssid: self.selected.clone(),
                password: self.password.text.clone(),
            })
        }
    }
    pub fn connect_result(&mut self, accepted: bool) {
        if accepted {
            self.password.clear();
            self.busy = false;
            self.page = 1;
            self.rebuild();
        } else {
            self.busy = true;
            self.invalidate(rect(12, 178, 336, 48));
        }
    }
    pub fn sync_status(
        &mut self,
        clock: &str,
        wifi: bool,
        ble: bool,
        usb: bool,
        ip: &str,
        message: &str,
    ) {
        let clock_changed = self.clock.as_str() != clock;
        let status_changed =
            self.wifi != wifi || self.ble != ble || self.usb != usb || self.ip.as_str() != ip;
        let message_changed = self.message.as_str() != message;
        copy_text(&mut self.clock, clock);
        copy_text(&mut self.ip, ip);
        copy_text(&mut self.message, message);
        self.wifi = wifi;
        self.ble = ble;
        self.usb = usb;
        if self.page == 0 {
            if clock_changed {
                self.invalidate(rect(12, 12, 280, 44));
            }
            if status_changed {
                self.invalidate(rect(12, 72, 336, 78));
            }
        }
        if self.page == 1 && message_changed {
            self.invalidate(rect(12, 118, 336, 28));
        }
        if self.page == 3 && status_changed {
            self.invalidate(rect(16, 600, 328, 25));
        }
    }
    pub fn sync_networks(&mut self, networks: &[Network]) {
        let clipped = &networks[..networks.len().min(64)];
        if self.networks.as_slice() == clipped {
            return;
        }
        self.networks.clear();
        for n in clipped {
            self.networks.push(n.clone()).ok();
        }
        if self.page == 1 {
            // A new scan cannot complete a press against a different AP index.
            self.scene.handle(Event::Cancel);
            self.scene.update(NETWORKS, |w| {
                if let Widget::ListView(v) = w {
                    v.items = &EMPTY_ROWS[..clipped.len()];
                    v.scroll = 0;
                    v.selected = usize::MAX;
                }
            });
        }
    }
    pub fn sync_macros(&mut self, labels: &[&str; 8], pressed: u8) {
        for (k, label) in labels.iter().enumerate() {
            let changed =
                self.macro_labels[k].as_str() != *label || (self.pressed ^ pressed) & (1 << k) != 0;
            copy_text(&mut self.macro_labels[k], label);
            if self.page == 3 && changed {
                self.scene.update(20 + k as u16, |w| {
                    w.base_mut().style.background = if pressed & (1 << k) != 0 {
                        rgb(0x26, 0x91, 0xb5)
                    } else {
                        rgb(0xcd, 0xe4, 0xef)
                    }
                });
                self.invalidate(macro_bounds(k));
            }
        }
        self.pressed = pressed;
    }
    pub fn macro_key_at(&self, p: Point) -> Option<usize> {
        if self.page != 3 {
            return None;
        }
        (0..8).find(|&k| macro_bounds(k).contains(p))
    }
    pub fn render_full<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        self.scene.render_full(target)?;
        self.overlays(&mut target.clipped(&rect(0, 0, WIDTH, HEIGHT)))
    }
    pub fn render_with_extra_damage<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        extra: Option<Rectangle>,
    ) -> Result<Option<Rectangle>, D::Error> {
        let damage = self.scene.render_with_extra_damage(target, extra)?;
        if let Some(r) = damage {
            if let Err(e) = self.overlays(&mut target.clipped(&r)) {
                self.invalidate(r);
                return Err(e);
            }
        }
        Ok(damage)
    }
    fn overlays<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
        match self.page {
            0 => {
                let mut doubled = DoubleText {
                    target,
                    offset: Point::new(12, 12),
                };
                text(&mut doubled, &self.clock, Point::zero(), ink())?;
                status_icon(target, Point::new(16, 76), 0, self.wifi)?;
                status_icon(target, Point::new(66, 76), 1, self.ble)?;
                status_icon(target, Point::new(116, 76), 2, self.usb)?;
                text(
                    &mut target.clipped(&rect(12, 122, 336, 28)),
                    &self.ip,
                    Point::new(12, 122),
                    ink(),
                )?;
                let image_clip = target.bounding_box().intersection(&rect(5, 170, 350, 326));
                for pixel in image_clip.points() {
                    let x = (pixel.x - 5) as usize;
                    let y = (pixel.y - 170) as usize;
                    let i = (y * 2 * 700 + x * 2) * 3;
                    target.draw_iter([Pixel(pixel, rgb(WHALE[i], WHALE[i + 1], WHALE[i + 2]))])?;
                }
            }
            1 => {
                text(target, "SETTINGS", Point::new(105, 16), ink())?;
                text(
                    &mut target.clipped(&rect(12, 118, 336, 28)),
                    &self.message,
                    Point::new(12, 118),
                    accent(),
                )?;
                let scroll = match self.scene.get(NETWORKS) {
                    Some(Widget::ListView(l)) => l.scroll,
                    _ => 0,
                };
                // Scene has already drawn the list focus ring. Keep overlays
                // inside its rounded, radius-8 perimeter, including when a
                // scrolled separator crosses the top or bottom edge.
                let interior = rect(16, 154, 328, 366);
                let mut list = target.clipped(&interior);
                for (i, n) in self.networks.iter().enumerate() {
                    let y = 150 + i as i32 * 70 - scroll as i32;
                    if y + 70 < 150 || y >= 524 {
                        continue;
                    }
                    ssid(&mut list, &n.ssid, Point::new(22, y + 10), ink())?;
                    let mut detail = String::<48>::new();
                    write!(
                        &mut detail,
                        "{} dBm / {}",
                        n.rssi,
                        if n.secure { "secured" } else { "open" }
                    )
                    .ok();
                    text(&mut list, &detail, Point::new(22, y + 40), accent())?;
                    list.fill_solid(&rect(16, y + 68, 328, 2), background())?;
                }
                text(
                    target,
                    "Wi-Fi credentials stay in RAM",
                    Point::new(12, 604),
                    accent(),
                )?;
            }
            2 => {
                text(target, "WIFI PASSWORD", Point::new(100, 16), ink())?;
                ssid(target, &self.selected, Point::new(12, 68), ink())?;
                let mut mask = String::<64>::new();
                for _ in 0..self.password.text.len() {
                    mask.push('*').ok();
                }
                text(
                    &mut target.clipped(&rect(22, 112, 316, 52)),
                    &mask,
                    Point::new(22, 128),
                    ink(),
                )?;
                let mut hint = String::<96>::new();
                if self.busy {
                    hint.push_str("Wi-Fi busy. Please retry.").ok();
                } else {
                    write!(
                        &mut hint,
                        "{} / 64  Shift:{} Caps:{}",
                        self.password.text.len(),
                        if self.password.shift { "ON" } else { "off" },
                        if self.password.caps { "ON" } else { "off" }
                    )
                    .ok();
                }
                text(
                    &mut target.clipped(&rect(12, 178, 336, 48)),
                    &hint,
                    Point::new(12, 178),
                    accent(),
                )?;
                for k in password_keyboard::layout() {
                    let r = typing_bounds(&k);
                    let mut bytes = [0; 4];
                    let label = match k.action {
                        password_keyboard::Action::Character(a, b) => {
                            self.password.character(a, b).encode_utf8(&mut bytes)
                        }
                        password_keyboard::Action::Shift => "Shift",
                        password_keyboard::Action::Caps => "Caps",
                        password_keyboard::Action::Backspace => "Bksp",
                        password_keyboard::Action::Enter => "Enter",
                        password_keyboard::Action::Clear => "Clear",
                        password_keyboard::Action::Space => "Space",
                    };
                    let active = (k.action == password_keyboard::Action::Shift
                        && self.password.shift)
                        || (k.action == password_keyboard::Action::Caps && self.password.caps);
                    if active {
                        target.fill_solid(&r, rgb(0x86, 0xcd, 0xe8))?;
                    }
                    typing_label(&mut target.clipped(&r), label, r, ink())?;
                }
            }
            3 => {
                text(target, "USB MACROPAD", Point::new(100, 17), ink())?;
                text(
                    target,
                    "Touch keys / VIA remapping",
                    Point::new(16, 76),
                    accent(),
                )?;
                for k in 0..8 {
                    let r = macro_bounds(k);
                    centered(
                        &mut target.clipped(&r),
                        &self.macro_labels[k],
                        r,
                        if self.pressed & (1 << k) != 0 {
                            Rgb565::WHITE
                        } else {
                            ink()
                        },
                    )?;
                }
                text(
                    target,
                    if self.usb {
                        "USB connected"
                    } else {
                        "USB inactive"
                    },
                    Point::new(16, 600),
                    accent(),
                )?;
            }
            _ => {}
        }
        Ok(())
    }
}
fn copy_text<const N: usize>(out: &mut String<N>, s: &str) {
    out.clear();
    for c in s.chars() {
        if out.push(c).is_err() {
            break;
        }
    }
}
fn typing_bounds(k: &password_keyboard::Key) -> Rectangle {
    rect(
        (8. + k.x * 23.7) as i32,
        238 + k.row as i32 * 54,
        (k.width * 23.7 - 2.) as u32,
        48,
    )
}
fn macro_bounds(k: usize) -> Rectangle {
    rect(
        16 + (k % 2) as i32 * 172,
        138 + (k / 2) as i32 * 110,
        156,
        99,
    )
}
fn text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    p: Point,
    c: Rgb565,
) -> Result<(), D::Error> {
    Text::with_text_style(
        s,
        p,
        MonoTextStyle::new(&FONT_10X20, c),
        TextStyleBuilder::new().baseline(Baseline::Top).build(),
    )
    .draw(d)
    .map(|_| ())
}
fn centered<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    r: Rectangle,
    c: Rgb565,
) -> Result<(), D::Error> {
    Text::with_text_style(
        s,
        Point::new(
            r.top_left.x + r.size.width as i32 / 2,
            r.top_left.y + (r.size.height as i32 - 20) / 2,
        ),
        MonoTextStyle::new(&FONT_10X20, c),
        TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Top)
            .build(),
    )
    .draw(d)
    .map(|_| ())
}
fn ssid<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    p: Point,
    c: Rgb565,
) -> Result<(), D::Error> {
    let mut x = p.x;
    for ch in s.chars() {
        if x > p.x + 300 {
            break;
        }
        if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            let offset = (ch as usize - 0x4e00) * 32;
            for y in 0..16 {
                for col in 0..16 {
                    if CJK[offset + y * 2 + col / 8] & (0x80 >> (col % 8)) != 0 {
                        d.draw_iter([Pixel(Point::new(x + col as i32, p.y + y as i32 + 2), c)])?;
                    }
                }
            }
            x += 16;
        } else {
            let mut bytes = [0; 4];
            text(d, ch.encode_utf8(&mut bytes), Point::new(x, p.y), c)?;
            x += 10;
        }
    }
    Ok(())
}
struct DoubleText<'a, D> {
    target: &'a mut D,
    offset: Point,
}
impl<D: DrawTarget<Color = Rgb565>> OriginDimensions for DoubleText<'_, D> {
    fn size(&self) -> Size {
        Size::new(140, 22)
    }
}
impl<D: DrawTarget<Color = Rgb565>> DrawTarget for DoubleText<'_, D> {
    type Color = Rgb565;
    type Error = D::Error;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        iter: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in iter {
            if !self.bounding_box().contains(p) {
                continue;
            }
            self.target.fill_solid(
                &rect(self.offset.x + p.x * 2, self.offset.y + p.y * 2, 2, 2),
                c,
            )?;
        }
        Ok(())
    }
}
fn status_icon<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    p: Point,
    kind: u8,
    ready: bool,
) -> Result<(), D::Error> {
    let c = if ready {
        accent()
    } else {
        rgb(0xb8, 0xcd, 0xd5)
    };
    let st = PrimitiveStyle::with_stroke(c, 3);
    match kind {
        0 => {
            for (a, b) in [
                (Point::new(1, 8), Point::new(14, 1)),
                (Point::new(14, 1), Point::new(27, 8)),
                (Point::new(6, 15), Point::new(14, 10)),
                (Point::new(14, 10), Point::new(22, 15)),
            ] {
                Line::new(p + a, p + b).into_styled(st).draw(d)?;
            }
            Circle::new(p + Point::new(11, 21), 6)
                .into_styled(PrimitiveStyle::with_fill(c))
                .draw(d)?;
        }
        1 => {
            for (a, b) in [
                (Point::new(13, 0), Point::new(13, 29)),
                (Point::new(13, 0), Point::new(24, 9)),
                (Point::new(24, 9), Point::new(3, 23)),
                (Point::new(3, 7), Point::new(24, 21)),
                (Point::new(24, 21), Point::new(13, 29)),
            ] {
                Line::new(p + a, p + b).into_styled(st).draw(d)?;
            }
        }
        _ => {
            Line::new(p + Point::new(15, 1), p + Point::new(15, 27))
                .into_styled(st)
                .draw(d)?;
            Line::new(p + Point::new(4, 10), p + Point::new(15, 19))
                .into_styled(st)
                .draw(d)?;
            Line::new(p + Point::new(26, 7), p + Point::new(15, 15))
                .into_styled(st)
                .draw(d)?;
            Circle::new(p + Point::new(1, 6), 6)
                .into_styled(PrimitiveStyle::with_fill(c))
                .draw(d)?;
            d.fill_solid(&rect(p.x + 23, p.y + 3, 6, 6), c)?;
        }
    }
    Ok(())
}

fn typing_label<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    r: Rectangle,
    c: Rgb565,
) -> Result<(), D::Error> {
    let small = s.len() > 1;
    Text::with_text_style(
        s,
        Point::new(
            r.top_left.x + r.size.width as i32 / 2,
            r.top_left.y + (r.size.height as i32 - if small { 10 } else { 20 }) / 2,
        ),
        MonoTextStyle::new(if small { &FONT_6X10 } else { &FONT_10X20 }, c),
        TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Top)
            .build(),
    )
    .draw(d)
    .map(|_| ())
}
