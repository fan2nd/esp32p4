//! Allocation-free macropad control shared by both desktop UI backends.
//!
//! This consumes the complete touch mask, not individual click callbacks, so
//! simultaneous chords and VIA macros have the same semantics on either UI.
use crate::keyboard::{self, Keymap, MacroPlayer, MacroStep};

#[derive(Default)]
pub struct MacroController {
    held: [u16; 8],
    pressed: u8,
    last_report: [u8; 10],
    player: MacroPlayer,
    macro_due: u64,
}

impl MacroController {
    /// Current effective touch mask. It is always zero outside the macropad.
    pub fn pressed(&self) -> u8 {
        self.pressed
    }

    /// Advance touch chords and macro playback. Returns only changed HID
    /// reports; callers enqueue them through `desktop::Shared::report`.
    ///
    /// `mask` must be cleared on touch timeout/cancellation by the input driver.
    /// Key mappings are latched at each press edge, matching the Slint desktop.
    /// The screen intentionally exposes VIA layer 0 only.
    pub fn update(
        &mut self,
        page_is_macro: bool,
        mask: u8,
        map: &Keymap,
        now: u64,
    ) -> Option<[u8; 10]> {
        let mask = if page_is_macro { mask } else { 0 };
        if !page_is_macro {
            self.player.active = false;
        }
        for k in 0..8 {
            if mask & (1 << k) == 0 {
                self.held[k] = 0;
            }
        }
        for k in 0..8 {
            if mask & !self.pressed & (1 << k) == 0 {
                continue;
            }
            let code = map.resolved(0, k);
            self.held[k] = code;
            self.player.active = false;
            if (0x7700..=0x7707).contains(&code) {
                self.player.start(map, (code - 0x7700) as usize);
                self.macro_due = now;
            }
        }
        self.pressed = mask;
        let report = if self.player.active {
            if now >= self.macro_due {
                match self.player.next_step() {
                    MacroStep::Report(r) => {
                        self.macro_due = now.saturating_add(8);
                        Some(r)
                    }
                    MacroStep::Delay(ms) => {
                        self.macro_due = now.saturating_add(u64::from(ms));
                        None
                    }
                }
            } else {
                None
            }
        } else {
            Some(keyboard::report(&self.held))
        };
        if let Some(report) = report {
            if report != self.last_report {
                self.last_report = report;
                return Some(report);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_release_and_suppress_duplicate_reports() {
        let mut c = MacroController::default();
        let mut map = Keymap::default();
        map.set(0, 0, 4);
        map.set(0, 1, 5);
        assert_eq!(c.update(true, 3, &map, 0), Some(keyboard::report(&[4, 5])));
        assert_eq!(c.pressed(), 3);
        assert_eq!(c.update(true, 3, &map, 5), None);
        assert_eq!(c.update(true, 2, &map, 6), Some(keyboard::report(&[5])));
        assert_eq!(c.update(true, 0, &map, 7), Some([0; 10]));
        assert_eq!(c.update(true, 0, &map, 8), None);
    }

    #[test]
    fn mapping_is_latched_on_press_and_layer_zero_only() {
        let mut c = MacroController::default();
        let mut map = Keymap::default();
        map.set(0, 0, 4);
        map.set(1, 0, 6);
        assert_eq!(c.update(true, 1, &map, 0), Some(keyboard::report(&[4])));
        map.set(0, 0, 5);
        assert_eq!(c.update(true, 1, &map, 1), None);
        assert_eq!(c.update(true, 0, &map, 2), Some([0; 10]));
        assert_eq!(c.update(true, 1, &map, 3), Some(keyboard::report(&[5])));
    }

    #[test]
    fn leaving_page_cancels_and_releases_even_with_contacts() {
        let mut c = MacroController::default();
        let map = Keymap::default();
        assert!(c.update(true, 1, &map, 0).is_some());
        assert_eq!(c.update(false, 1, &map, 1), Some([0; 10]));
        assert_eq!(c.pressed(), 0);
        assert_eq!(c.update(false, 255, &map, 2), None);
    }

    #[test]
    fn macro_tap_uses_eight_ms_spacing_and_page_exit_cancels() {
        let mut c = MacroController::default();
        let mut map = Keymap::default();
        map.set(0, 0, 0x7700);
        map.bytes[keyboard::MAP_BYTES..keyboard::MAP_BYTES + 3].copy_from_slice(b"ab\0");
        assert_eq!(c.update(true, 1, &map, 100), Some(keyboard::report(&[4])));
        assert_eq!(c.update(true, 1, &map, 107), None);
        assert_eq!(c.update(true, 1, &map, 108), Some([0; 10]));
        assert_eq!(c.update(true, 1, &map, 116), Some(keyboard::report(&[5])));
        assert_eq!(c.update(false, 1, &map, 117), Some([0; 10]));
        assert_eq!(c.update(false, 0, &map, 200), None);
    }

    #[test]
    fn macro_delay_is_respected() {
        let mut c = MacroController::default();
        let mut map = Keymap::default();
        map.set(0, 0, 0x7700);
        let bytes = [1, 4, b'2', b'0', b'|', b'a', 0];
        map.bytes[keyboard::MAP_BYTES..keyboard::MAP_BYTES + bytes.len()].copy_from_slice(&bytes);
        assert_eq!(c.update(true, 1, &map, 10), None);
        assert_eq!(c.update(true, 1, &map, 29), None);
        assert_eq!(c.update(true, 1, &map, 30), Some(keyboard::report(&[4])));
    }
}
