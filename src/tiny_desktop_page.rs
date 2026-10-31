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
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use heapless::{String, Vec};
use tiny_ui::{
    ActionKind, AlphaIcon, Button, Event, ListView, Panel, Scene, Slider, Style, Widget,
};
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
const WIFI_TOGGLE: u16 = 38;
const BLE_TOGGLE: u16 = 39;
const SAVED_CONNECT: u16 = 40;
const EDIT_PASSWORD: u16 = 41;
const FORGET: u16 = 42;
const TIME_SETTINGS: u16 = 43;
const SYNC_TIME: u16 = 44;
const TYPING: u16 = 100;
const WIFI_SETTINGS: u16 = 30;
const BLE_SETTINGS: u16 = 31;
const BRIGHTNESS_SETTINGS: u16 = 32;
const VOLUME_SETTINGS: u16 = 33;
const SETTING_SLIDER: u16 = 34;
const RESET_SETTINGS: u16 = 35;
const CONFIRM_RESET: u16 = 36;
const CANCEL_RESET: u16 = 37;
// Keep existing page IDs stable for macro-key dispatch; 4 is the settings root.

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
        border: rgb(0x75, 0xa6, 0xba),
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
    pub saved: bool,
    pub connected: bool,
    pub available: bool,
}
pub enum Action {
    Reset,
    SyncTime,
    WifiEnabled(bool),
    BleEnabled(bool),
    ConnectSaved(String<32>),
    Forget(String<32>),
    Brightness(u8),
    Volume(u8),
    BleName(String<24>),
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
    wifi_enabled: bool,
    ble_enabled: bool,
    wifi: bool,
    ble: bool,
    usb: bool,
    macro_labels: [String<24>; 8],
    pressed: u8,
    brightness: u8,
    volume: u8,
    ble_name: String<24>,
    settings_message: String<96>,
    ble_status_seen: String<96>,
    save_status: String<96>,
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
            wifi_enabled: true,
            ble_enabled: false,
            wifi: false,
            ble: false,
            usb: false,
            macro_labels: core::array::from_fn(|_| String::new()),
            pressed: 0,
            brightness: 100,
            volume: 100,
            ble_name: String::try_from("P4 Desktop").unwrap(),
            settings_message: String::new(),
            ble_status_seen: String::new(),
            save_status: String::new(),
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
                self.button(
                    WIFI_TOGGLE,
                    rect(12, 60, 160, 44),
                    if self.wifi_enabled {
                        "Wi-Fi ON"
                    } else {
                        "Wi-Fi OFF"
                    },
                );
                self.button(SCAN, rect(184, 60, 164, 44), "SCAN");
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
            9 => {
                self.button(SAVED_CONNECT, rect(24, 180, 312, 60), "CONNECT SAVED");
                self.button(EDIT_PASSWORD, rect(24, 270, 312, 60), "EDIT PASSWORD");
                self.button(FORGET, rect(24, 360, 312, 60), "FORGET NETWORK");
            }
            2 | 5 => {
                if self.page == 5 {
                    self.button(
                        BLE_TOGGLE,
                        rect(12, 60, 336, 40),
                        if self.ble_enabled {
                            "BLE ON - tap to turn OFF"
                        } else {
                            "BLE OFF - tap to turn ON"
                        },
                    );
                }
                let mut panel = Panel::new(8, rect(12, 112, 336, 52));
                panel.base.style = style();
                self.scene.add(panel).unwrap();
                for (i, k) in password_keyboard::layout().iter().enumerate() {
                    self.button(TYPING + i as u16, typing_bounds(k), "");
                }
                self.button(
                    CONNECT,
                    rect(12, 542, 336, 50),
                    if self.page == 5 {
                        "APPLY NAME"
                    } else {
                        "CONNECT"
                    },
                );
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
            4 => {
                for (id, y, title) in [
                    (WIFI_SETTINGS, 60, "Wi-Fi connection"),
                    (BLE_SETTINGS, 144, "Bluetooth name"),
                    (TIME_SETTINGS, 228, "Time synchronization"),
                    (BRIGHTNESS_SETTINGS, 312, "Screen brightness"),
                    (VOLUME_SETTINGS, 396, "Playback volume"),
                    (RESET_SETTINGS, 480, "Reset settings"),
                ] {
                    self.button(id, rect(16, y, 328, 72), title);
                }
            }
            10 => {
                self.button(SYNC_TIME, rect(24, 210, 312, 64), "SYNC NOW");
            }
            8 => {
                self.button(CONFIRM_RESET, rect(24, 330, 312, 64), "RESET AND RESTART");
                self.button(CANCEL_RESET, rect(24, 420, 312, 64), "Cancel");
            }
            6 | 7 => {
                let (min, value) = if self.page == 6 {
                    (5, self.brightness)
                } else {
                    (0, self.volume)
                };
                let mut slider = Slider::new(
                    SETTING_SLIDER,
                    rect(24, 210, 312, 64),
                    min,
                    100,
                    1,
                    i32::from(value),
                );
                slider.base.style = style();
                self.scene.add(slider).unwrap();
            }
            _ => {}
        }
    }
    pub fn navigate(&mut self, page: u8) -> Option<Action> {
        if page > 10 {
            return None;
        }
        self.password.clear();
        self.busy = false;
        self.page = page;
        self.settings_message.clear();
        if page == 5 {
            let _ = self.password.text.push_str(&self.ble_name);
            self.ble_status_seen.clear();
        }
        self.rebuild();
        if page == 1 && self.wifi_enabled {
            Some(Action::Scan)
        } else {
            None
        }
    }
    pub fn handle(&mut self, event: Event) -> Option<Action> {
        if event == Event::KeyDown(tiny_ui::Key::Escape) {
            return self.navigate(self.parent_page());
        }
        let action = self.scene.handle(event)?;
        match (action.id, action.kind) {
            (BACK, ActionKind::Clicked) => self.navigate(self.parent_page()),
            (SETTINGS, ActionKind::Clicked) => self.navigate(4),
            (WIFI_SETTINGS, ActionKind::Clicked) => self.navigate(1),
            (BLE_SETTINGS, ActionKind::Clicked) => self.navigate(5),
            (BRIGHTNESS_SETTINGS, ActionKind::Clicked) => self.navigate(6),
            (VOLUME_SETTINGS, ActionKind::Clicked) => self.navigate(7),
            (TIME_SETTINGS, ActionKind::Clicked) => self.navigate(10),
            (SYNC_TIME, ActionKind::Clicked) => Some(Action::SyncTime),
            (RESET_SETTINGS, ActionKind::Clicked) => self.navigate(8),
            (CONFIRM_RESET, ActionKind::Clicked) => Some(Action::Reset),
            (CANCEL_RESET, ActionKind::Clicked) => self.navigate(4),
            (SETTING_SLIDER, ActionKind::ValueChanged(value)) => {
                self.invalidate(rect(24, 140, 312, 50));
                if self.page == 6 {
                    Some(Action::Brightness(value.clamp(5, 100) as u8))
                } else {
                    self.volume = value.clamp(0, 100) as u8;
                    Some(Action::Volume(self.volume))
                }
            }
            (KEYBOARD, ActionKind::Clicked) => self.navigate(3),
            (WIFI_TOGGLE, ActionKind::Clicked) => Some(Action::WifiEnabled(!self.wifi_enabled)),
            (BLE_TOGGLE, ActionKind::Clicked) => Some(Action::BleEnabled(!self.ble_enabled)),
            (SAVED_CONNECT, ActionKind::Clicked) => {
                Some(Action::ConnectSaved(self.selected.clone()))
            }
            (FORGET, ActionKind::Clicked) => {
                let ssid = self.selected.clone();
                self.navigate(1);
                Some(Action::Forget(ssid))
            }
            (EDIT_PASSWORD, ActionKind::Clicked) => self.navigate(2),
            (SCAN, ActionKind::Clicked) => Some(Action::Scan),
            (DISCONNECT, ActionKind::Clicked) => Some(Action::Disconnect),
            (CONNECT, ActionKind::Clicked) => self.connect(),
            (NETWORKS, ActionKind::SelectionChanged(index)) => {
                if let Some(ap) = self.networks.get(index) {
                    self.selected = ap.ssid.clone();
                    self.password.clear();
                    self.busy = false;
                    self.page = if ap.saved { 9 } else { 2 };
                    self.rebuild();
                }
                None
            }
            (id, ActionKind::Clicked) if id >= TYPING => {
                let layout = password_keyboard::layout();
                if let Some(k) = layout.get((id - TYPING) as usize) {
                    let inserting = matches!(
                        k.action,
                        password_keyboard::Action::Character(_, _)
                            | password_keyboard::Action::Space
                    );
                    if self.page == 5 && inserting && self.password.text.len() >= 24 {
                        self.set_setting_message("Name limit: 24 ASCII bytes");
                        return None;
                    }
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
    fn parent_page(&self) -> u8 {
        match self.page {
            2 | 9 => 1,
            1 | 5 | 6 | 7 | 8 | 10 => 4,
            _ => 0,
        }
    }
    fn connect(&mut self) -> Option<Action> {
        if self.page == 5 {
            if self.password.text.trim().is_empty() {
                self.set_setting_message("Enter a non-empty name");
                return None;
            }
            return String::<24>::try_from(self.password.text.as_str())
                .ok()
                .map(Action::BleName);
        }
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
            if self.page == 9 {
                self.invalidate(rect(20, 460, 320, 30));
            }
        }
    }
    pub fn set_setting_message(&mut self, message: &str) {
        if self.settings_message.as_str() != message {
            copy_text(&mut self.settings_message, message);
            self.invalidate(rect(12, 178, 336, 48));
            self.invalidate(rect(12, 310, 336, 100));
        }
    }
    pub fn sync_radio_switches(&mut self, wifi: bool, ble: bool) {
        if self.wifi_enabled == wifi && self.ble_enabled == ble {
            return;
        }
        self.wifi_enabled = wifi;
        self.ble_enabled = ble;
        self.scene.update(WIFI_TOGGLE, |w| {
            if let Widget::Button(b) = w {
                b.text = if wifi { "Wi-Fi ON" } else { "Wi-Fi OFF" };
            }
        });
        self.scene.update(BLE_TOGGLE, |w| {
            if let Widget::Button(b) = w {
                b.text = if ble {
                    "BLE ON - tap to turn OFF"
                } else {
                    "BLE OFF - tap to turn ON"
                };
            }
        });
    }
    pub fn sync_settings(&mut self, brightness: u8, volume: u8, status: &str) {
        if self.brightness != brightness || self.volume != volume {
            self.brightness = brightness;
            self.volume = volume;
            if self.page == 6 || self.page == 7 {
                let value = if self.page == 6 { brightness } else { volume };
                self.scene.update(SETTING_SLIDER, |w| {
                    if let Widget::Slider(v) = w {
                        v.value = i32::from(value);
                    }
                });
                self.invalidate(rect(24, 140, 312, 50));
            }
        }
        if self.save_status.as_str() != status {
            copy_text(&mut self.save_status, status);
            self.invalidate(rect(12, 560, 336, 80));
        }
    }
    pub fn brightness_result(&mut self, requested: u8, ok: bool) {
        if ok {
            self.brightness = requested.clamp(5, 100);
        }
        let value = self.brightness;
        self.scene.update(SETTING_SLIDER, |w| {
            if let Widget::Slider(slider) = w {
                slider.value = i32::from(value);
            }
        });
        self.invalidate(rect(24, 140, 312, 50));
        self.set_setting_message(if ok {
            "Brightness applied"
        } else {
            "Backlight write failed"
        });
    }
    pub fn sync_ble_name(&mut self, name: &str, status: &str) {
        copy_text(&mut self.ble_name, name);
        if self.page == 5 && self.ble_status_seen.as_str() != status {
            copy_text(&mut self.ble_status_seen, status);
            self.set_setting_message(status);
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
        if self.page == 10 {
            if clock_changed {
                self.invalidate(rect(12, 110, 336, 44));
            }
            if message_changed {
                self.invalidate(rect(12, 310, 336, 70));
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
                text(target, "WI-FI", Point::new(105, 16), ink())?;
                text(
                    &mut target.clipped(&rect(12, 118, 336, 28)),
                    &self.message,
                    Point::new(12, 118),
                    accent(),
                )?;
                let (scroll, interior) = match self.scene.get(NETWORKS) {
                    Some(Widget::ListView(l)) => (l.scroll.min(l.max_scroll()), l.content_bounds()),
                    _ => (0, rect(16, 154, 328, 366)),
                };
                // Scene has already drawn the list focus ring. Keep overlays
                // inside its rounded, radius-8 perimeter, including when a
                // scrolled separator crosses the top or bottom edge.
                let mut list = target.clipped(&interior);
                for (i, n) in self.networks.iter().enumerate() {
                    let y = 150 + i as i32 * 70 - scroll as i32;
                    if y + 70 < 150 || y >= 524 {
                        continue;
                    }
                    ssid(&mut list, &n.ssid, Point::new(22, y + 10), ink())?;
                    let mut detail = String::<48>::new();
                    let tag = if n.connected {
                        "CONNECTED"
                    } else if n.saved {
                        "SAVED"
                    } else if n.secure {
                        "secured"
                    } else {
                        "open"
                    };
                    if n.available {
                        write!(&mut detail, "{} / {} dBm", tag, n.rssi).ok();
                    } else {
                        write!(&mut detail, "{} / not in scan", tag).ok();
                    }
                    text(&mut list, &detail, Point::new(22, y + 40), accent())?;
                    list.fill_solid(&rect(16, y + 68, 328, 2), background())?;
                }
                text(target, &self.save_status, Point::new(12, 604), accent())?;
            }
            2 | 5 => {
                text(
                    target,
                    if self.page == 5 {
                        "BLUETOOTH NAME"
                    } else {
                        "WIFI PASSWORD"
                    },
                    Point::new(100, 16),
                    ink(),
                )?;
                if self.page != 5 {
                    ssid(target, &self.selected, Point::new(12, 68), ink())?;
                }
                let mut mask = String::<64>::new();
                if self.page == 5 {
                    mask.push_str(&self.password.text).ok();
                } else {
                    for _ in 0..self.password.text.len() {
                        mask.push('*').ok();
                    }
                }
                text(
                    &mut target.clipped(&rect(22, 112, 316, 52)),
                    &mask,
                    Point::new(22, 128),
                    ink(),
                )?;
                let mut hint = String::<96>::new();
                if self.page == 5 && !self.settings_message.is_empty() {
                    hint.push_str(&self.settings_message).ok();
                } else if self.busy {
                    hint.push_str("Wi-Fi off or request pending").ok();
                } else {
                    write!(
                        &mut hint,
                        "{} / {}  Shift:{} Caps:{}",
                        self.password.text.len(),
                        if self.page == 5 { 24 } else { 64 },
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
            4 => {
                text(target, "SETTINGS", Point::new(105, 16), ink())?;
                text(target, &self.save_status, Point::new(20, 570), accent())?;
            }
            6 | 7 => {
                text(
                    target,
                    if self.page == 6 {
                        "BRIGHTNESS"
                    } else {
                        "VOLUME"
                    },
                    Point::new(105, 16),
                    ink(),
                )?;
                let value = if self.page == 6 {
                    self.brightness
                } else {
                    self.volume
                };
                let mut label = String::<32>::new();
                write!(&mut label, "{}%", value).ok();
                text(target, &label, Point::new(24, 152), ink())?;
                text(
                    target,
                    if self.page == 6 {
                        "Minimum 5% keeps touch usable"
                    } else {
                        "Playback only; 100% = 0 dB"
                    },
                    Point::new(12, 310),
                    accent(),
                )?;
                text(
                    &mut target.clipped(&rect(12, 350, 336, 40)),
                    &self.settings_message,
                    Point::new(12, 350),
                    accent(),
                )?;
                text(target, &self.save_status, Point::new(20, 570), accent())?;
            }
            10 => {
                text(target, "TIME SYNC", Point::new(105, 16), ink())?;
                text(target, &self.clock, Point::new(24, 118), ink())?;
                text(
                    target,
                    "Beijing time (UTC+8)",
                    Point::new(24, 158),
                    accent(),
                )?;
                text(target, &self.message, Point::new(12, 310), accent())?;
                text(
                    target,
                    "Manual only; requires Wi-Fi",
                    Point::new(12, 390),
                    ink(),
                )?;
            }
            9 => {
                text(target, "SAVED NETWORK", Point::new(105, 16), ink())?;
                ssid(target, &self.selected, Point::new(20, 90), ink())?;
                text(
                    target,
                    "Connect without typing password",
                    Point::new(20, 130),
                    accent(),
                )?;
                if self.busy {
                    text(
                        target,
                        "Wi-Fi off or request pending",
                        Point::new(20, 460),
                        accent(),
                    )?;
                }
                text(target, &self.save_status, Point::new(20, 570), accent())?;
            }
            8 => {
                text(target, "RESET SETTINGS", Point::new(105, 16), ink())?;
                for (y, message) in [
                    (130, "Forget Wi-Fi and password"),
                    (170, "Reset Bluetooth name"),
                    (210, "Reset brightness and volume"),
                    (250, "Reset VIA keys and macros"),
                ] {
                    text(target, message, Point::new(20, y), ink())?;
                }
                text(target, &self.save_status, Point::new(20, 570), accent())?;
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
    let mask: &[u8] = match kind {
        0 => include_bytes!("../assets/material-icons/wifi.a8"),
        1 => include_bytes!("../assets/material-icons/bluetooth.a8"),
        _ => include_bytes!("../assets/material-icons/usb.a8"),
    };
    AlphaIcon::new(mask, Size::new(30, 30)).draw(d, p, c, background())
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
