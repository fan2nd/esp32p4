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
