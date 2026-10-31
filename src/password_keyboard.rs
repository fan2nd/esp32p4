//! ASCII password entry with PC keyboard rows. No password reaches the logger.
use heapless::{String, Vec};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Character(u8, u8),
    Shift,
    Caps,
    Backspace,
    Enter,
    Clear,
    Space,
}
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub x: f32,
    pub width: f32,
    pub row: usize,
    pub action: Action,
}

/// Positions are in key units, including the PC keyboard's stagger and wider
/// modifiers. Both drawing and click dispatch use this single ordered layout.
pub fn layout() -> Vec<Key, 64> {
    let mut keys = Vec::new();
    let mut push = |row, x, width, action| {
        keys.push(Key {
            row,
            x,
            width,
            action,
        })
        .unwrap()
    };
    for (row, normal, shifted, offset) in [
        (0, &b"`1234567890-="[..], &b"~!@#$%^&*()_+"[..], 0.),
        (1, &b"qwertyuiop[]\\"[..], &b"QWERTYUIOP{}|"[..], 1.5),
        (2, &b"asdfghjkl;'"[..], &b"ASDFGHJKL:\""[..], 1.75),
        (3, &b"zxcvbnm,./"[..], &b"ZXCVBNM<>?"[..], 2.25),
    ] {
        for (i, (&a, &b)) in normal.iter().zip(shifted).enumerate() {
            push(row, offset + i as f32, 1., Action::Character(a, b));
        }
    }
    push(0, 13., 1.5, Action::Backspace);
    // Passwords cannot contain a Tab; this position provides a second backspace.
    push(1, 0., 1.5, Action::Backspace);
    push(2, 0., 1.75, Action::Caps);
    push(2, 12.75, 1.75, Action::Enter);
    push(3, 0., 2.25, Action::Shift);
    push(3, 12.25, 2.25, Action::Shift);
    push(4, 0., 2.5, Action::Clear);
    push(4, 2.5, 9.5, Action::Space);
    push(4, 12., 2.5, Action::Backspace);
    keys
}

#[derive(Default)]
pub struct Password {
    pub text: String<64>,
    pub shift: bool,
    pub caps: bool,
}
impl Password {
    pub fn character(&self, normal: u8, shifted: u8) -> char {
        let upper = self.shift ^ (self.caps && normal.is_ascii_alphabetic());
        if upper {
            shifted as char
        } else {
            normal as char
        }
    }
    /// Return true for Enter. Shift is sticky for one character, Caps persists.
    pub fn press(&mut self, action: Action) -> bool {
        match action {
            Action::Character(a, b) => {
                let _ = self.text.push(self.character(a, b));
                self.shift = false;
            }
            Action::Shift => self.shift = !self.shift,
            Action::Caps => self.caps = !self.caps,
            Action::Backspace => {
                self.text.pop();
            }
            Action::Clear => self.clear(),
            Action::Space => {
                let _ = self.text.push(' ');
                self.shift = false;
            }
            Action::Enter => return true,
        }
        false
    }
    pub fn clear(&mut self) {
        // Erase the live bytes before shrinking the fixed-capacity string.
        unsafe {
            self.text.as_bytes_mut().fill(0);
        }
        self.text.clear();
        self.shift = false;
        self.caps = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pc_shift_caps_and_symbols() {
        let mut p = Password::default();
        p.press(Action::Shift);
        p.press(Action::Character(b'1', b'!'));
        p.press(Action::Caps);
        p.press(Action::Character(b'a', b'A'));
        p.press(Action::Shift);
        p.press(Action::Character(b'b', b'B'));
        p.press(Action::Character(b'-', b'_'));
        assert_eq!(p.text.as_str(), "!Ab-");
        p.press(Action::Backspace);
        assert_eq!(p.text.as_str(), "!Ab");
    }
    #[test]
    fn layout_covers_printable_ascii_and_stays_on_screen() {
        let keys = layout();
        for k in &keys {
            assert!(k.x >= 0. && k.x + k.width <= 14.5);
        }
        for c in 33u8..=126 {
            assert!(
                keys.iter()
                    .any(|k| matches!(k.action, Action::Character(a,b) if c == a || c == b)),
                "missing {c}"
            );
        }
    }
    #[test]
    fn password_limit_and_clear() {
        let mut p = Password::default();
        for _ in 0..80 {
            p.press(Action::Character(b'a', b'A'));
        }
        assert_eq!(p.text.len(), 64);
        assert!(p.press(Action::Enter));
        p.clear();
        assert!(p.text.is_empty());
        assert!(!p.shift && !p.caps);
    }
}
