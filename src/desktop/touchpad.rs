//! Relative HID touchpad gestures; independent of rendering and USB transport.
use crate::driver::touch::Contact;
use embedded_graphics::{
    geometry::{Point, Size},
    primitives::Rectangle,
};

pub const PAGE: u8 = 11;
// Logical screen coordinates; GT911 coordinates are divided by two at the boundary.
pub const PAD: Rectangle = Rectangle::new(Point::new(12, 90), Size::new(336, 410));
pub const LEFT: Rectangle = Rectangle::new(Point::new(12, 520), Size::new(160, 60));
pub const RIGHT: Rectangle = Rectangle::new(Point::new(188, 520), Size::new(160, 60));
/// Buttons, signed little-endian X/Y (16 bit), signed vertical wheel (8 bit).
pub const DESCRIPTOR: &[u8] = &[
    0x05, 1, 0x09, 2, 0xa1, 1, 0x09, 1, 0xa1, 0, 0x05, 9, 0x19, 1, 0x29, 3, 0x15, 0, 0x25, 1, 0x75,
    1, 0x95, 3, 0x81, 2, 0x75, 5, 0x95, 1, 0x81, 1, 0x05, 1, 0x09, 0x30, 0x09, 0x31, 0x16, 1, 0x80,
    0x26, 0xff, 0x7f, 0x75, 16, 0x95, 2, 0x81, 6, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7f, 0x75, 8,
    0x95, 1, 0x81, 6, 0xc0, 0xc0,
];
#[derive(Default)]
pub struct Touchpad {
    ids: u16,
    count: usize,
    previous: Point,
    started: u64,
    distance: i32,
    fingers: usize,
    wheel: i32,
    buttons: u8,
    tapping: bool,
}
impl Touchpad {
    /// Cancellation never synthesizes a tap; used on timeout, page exit and USB loss.
    pub fn cancel(&mut self, mut emit: impl FnMut([u8; 6])) {
        if self.buttons != 0 {
            emit([0; 6]);
        }
        *self = Self::default();
    }
    pub fn update(&mut self, contacts: &[Contact], now: u64, mut emit: impl FnMut([u8; 6])) {
        let mut ids = 0u16;
        let mut count = 0;
        let mut sum = Point::zero();
        let mut buttons = 0;
        for contact in contacts {
            let p = contact.position / 2;
            if PAD.contains(p) {
                ids |= 1u16 << (contact.id & 15);
                count += 1;
                sum += contact.position;
            } else if LEFT.contains(p) {
                buttons |= 1;
            } else if RIGHT.contains(p) {
                buttons |= 2;
            }
        }
        let mut dx = 0i16;
        let mut dy = 0i16;
        let mut wheel = 0i8;
        if count == 0 {
            // A pad tap is recognized only when all contacts lift, never when
            // crossing into a button or leaving the pad while still touching.
            if self.count != 0
                && contacts.is_empty()
                && self.tapping
                && now.saturating_sub(self.started) <= 250
                && self.distance <= 12
            {
                emit([if self.fingers == 1 { 1 } else { 2 }, 0, 0, 0, 0, 0]);
                emit([0; 6]);
            }
            self.tapping = false;
            self.wheel = 0;
        } else {
            let center = sum / count as i32;
            if self.count == 0 {
                self.started = now;
                self.distance = 0;
                self.fingers = count;
                self.tapping = buttons == 0 && count <= 2;
            } else if self.ids == ids && self.count == count {
                let delta = center - self.previous;
                self.distance = self.distance.saturating_add(delta.x.abs() + delta.y.abs());
                if count == 1 && self.fingers == 1 {
                    dx = delta.x.clamp(-32767, 32767) as i16;
                    dy = delta.y.clamp(-32767, 32767) as i16;
                } else if count == 2 {
                    self.wheel += delta.y;
                    wheel = (-self.wheel / 32).clamp(-127, 127) as i8;
                    self.wheel += i32::from(wheel) * 32;
                }
            } else {
                // Rebase on contact changes; never turn a second finger into a cursor jump.
                self.wheel = 0;
                self.fingers = self.fingers.max(count);
                if count > 2 || (self.count == count && self.ids != ids) {
                    self.tapping = false;
                }
            }
            if buttons != 0 {
                self.tapping = false;
            }
            self.previous = center;
        }
        if buttons != self.buttons || dx != 0 || dy != 0 || wheel != 0 {
            let x = dx.to_le_bytes();
            let y = dy.to_le_bytes();
            emit([buttons, x[0], x[1], y[0], y[1], wheel as u8]);
        }
        self.buttons = buttons;
        self.ids = ids;
        self.count = count;
    }
}
