//! Portable VIA protocol, keymap and macro logic. No hardware or C dependencies.
use core::fmt::Write;
pub const LAYERS: usize = 4;
pub const KEYS: usize = 8;
pub const MAP_BYTES: usize = LAYERS * KEYS * 2;
pub const MACRO_BYTES: usize = 512;
pub const PAYLOAD: usize = MAP_BYTES + MACRO_BYTES;
pub const RECORD: usize = PAYLOAD + 16;

#[derive(Clone)]
pub struct Keymap {
    pub bytes: [u8; PAYLOAD],
    pub revision: u32,
}
impl Default for Keymap {
    fn default() -> Self {
        let mut map = Self {
            bytes: [0; PAYLOAD],
            revision: 0,
        };
        let rows = [
            [
                0x0106, 0x0119, 0x011d, 0x011c, 0x0029, 0x0028, 0x5221, 0x002c,
            ],
            [0x003a, 0x003b, 0x003c, 0x003d, 0x00a9, 0x00aa, 1, 0x00ae],
            [
                0x7700, 0x7701, 0x7702, 0x7703, 0x7704, 0x7705, 0x7706, 0x7707,
            ],
            [
                0x0050, 0x004f, 0x0052, 0x0051, 0x004a, 0x004d, 0x004b, 0x004e,
            ],
        ];
        for (layer, keys) in rows.iter().enumerate() {
            for (key, code) in keys.iter().enumerate() {
                map.set(layer, key, *code);
            }
        }
        map.revision = 0;
        map
    }
}
impl Keymap {
    pub fn get(&self, layer: usize, key: usize) -> u16 {
        let offset = (layer * KEYS + key) * 2;
        u16::from_be_bytes([self.bytes[offset], self.bytes[offset + 1]])
    }
    pub fn resolved(&self, layer: usize, key: usize) -> u16 {
        for l in (0..=layer.min(LAYERS - 1)).rev() {
            let code = self.get(l, key);
            if code != 1 {
                return code;
            }
        }
        0
    }
    pub fn set(&mut self, layer: usize, key: usize, code: u16) {
        let offset = (layer * KEYS + key) * 2;
        self.bytes[offset..offset + 2].copy_from_slice(&code.to_be_bytes());
        self.revision = self.revision.wrapping_add(1);
    }
    /// VIA packets are always 32 bytes; numeric keycodes and buffer offsets are big-endian.
    /// Invalid requests receive id_unhandled and never partially modify the keymap.
    pub fn via(&mut self, p: &mut [u8; 32], uptime: u32, pressed: u8) {
        match p[0] {
            1 => p[1..3].copy_from_slice(&13u16.to_be_bytes()),
            2 => match p[1] {
                1 => p[2..6].copy_from_slice(&uptime.to_be_bytes()),
                2 => p[2..6].fill(0),
                3 => {
                    let start = p[2] as usize;
                    p[3..].fill(0);
                    for row in start..4 {
                        p[3 + row - start] = (pressed >> (row * 2)) & 3;
                    }
                }
                4 => p[2..6].copy_from_slice(&1u32.to_be_bytes()),
                6 => p[2..6].copy_from_slice(&9u32.to_be_bytes()),
                _ => p[0] = 0xff,
            },
            4 | 5 if p[1] < 4 && p[2] < 4 && p[3] < 2 => {
                let key = p[2] as usize * 2 + p[3] as usize;
                if p[0] == 4 {
                    let code = self.get(p[1] as usize, key);
                    p[4..6].copy_from_slice(&code.to_be_bytes());
                } else {
                    self.set(p[1] as usize, key, u16::from_be_bytes([p[4], p[5]]));
                }
            }
            6 | 10 => {
                let revision = self.revision.wrapping_add(1);
                if p[0] == 6 {
                    self.bytes[..MAP_BYTES].copy_from_slice(&Self::default().bytes[..MAP_BYTES]);
                } else {
                    *self = Self::default();
                }
                self.revision = revision;
            }
            12 => p[1] = 8,
            13 => p[1..3].copy_from_slice(&(MACRO_BYTES as u16).to_be_bytes()),
            14 | 15 | 18 | 19 => {
                let offset = u16::from_be_bytes([p[1], p[2]]) as usize;
                let count = p[3] as usize;
                let macro_buffer = p[0] == 14 || p[0] == 15;
                let size = if macro_buffer { MACRO_BYTES } else { MAP_BYTES };
                if count > 28 || offset + count > size {
                    p[0] = 0xff;
                    return;
                }
                let start = offset + if macro_buffer { MAP_BYTES } else { 0 };
                if p[0] == 14 || p[0] == 18 {
                    p[4..4 + count].copy_from_slice(&self.bytes[start..start + count]);
                } else {
                    self.bytes[start..start + count].copy_from_slice(&p[4..4 + count]);
                    self.revision = self.revision.wrapping_add(1);
                }
            }
            16 => {
                self.bytes[MAP_BYTES..].fill(0);
                self.revision = self.revision.wrapping_add(1);
            }
            17 => p[1] = LAYERS as u8,
            _ => p[0] = 0xff,
        }
    }
    pub fn record(&self, generation: u32) -> [u8; RECORD] {
        let mut out = [0u8; RECORD];
        out[..8].copy_from_slice(b"P4KEY001");
        out[8..12].copy_from_slice(&generation.to_le_bytes());
        out[16..].copy_from_slice(&self.bytes);
        let checksum = crc(&out[16..]) ^ generation;
        out[12..16].copy_from_slice(&checksum.to_le_bytes());
        out
    }
    pub fn restore(record: &[u8; RECORD]) -> Option<(Self, u32)> {
        if &record[..8] != b"P4KEY001" {
            return None;
        }
        let generation = u32::from_le_bytes(record[8..12].try_into().ok()?);
        if u32::from_le_bytes(record[12..16].try_into().ok()?) != crc(&record[16..]) ^ generation {
            return None;
        }
        let mut map = Self::default();
        map.bytes.copy_from_slice(&record[16..]);
        Some((map, generation))
    }
}

fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for b in bytes {
        value ^= *b as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(value & 1)));
        }
    }
    !value
}

/// Eight keyboard bytes followed by a two-byte Consumer Control usage.
pub fn report(codes: &[u16]) -> [u8; 10] {
    let mut out = [0; 10];
    let mut count = 0;
    for &code in codes {
        let key = code as u8;
        let mods = ((code >> 8) & 0x1f) as u8;
        if code <= 0x1fff {
            out[0] |= if mods & 0x10 != 0 {
                (mods & 15) << 4
            } else {
                mods
            };
            if (0xe0..=0xe7).contains(&key) {
                out[0] |= 1 << (key - 0xe0);
            } else if (4..=0xa4).contains(&key) && !out[2..8].contains(&key) {
                if count < 6 {
                    out[2 + count] = key;
                    count += 1;
                } else {
                    out[2..8].fill(1);
                }
            } else if (0xa8..=0xae).contains(&key) {
                let usage: u16 = [0xe2, 0xe9, 0xea, 0xb5, 0xb6, 0xb7, 0xcd][(key - 0xa8) as usize];
                out[8..].copy_from_slice(&usage.to_le_bytes());
            }
        }
    }
    out
}

pub fn label(code: u16) -> heapless::String<24> {
    let mut out = heapless::String::new();
    match code {
        0x0106 => out.push_str("COPY").unwrap(),
        0x0119 => out.push_str("PASTE").unwrap(),
        0x011d => out.push_str("UNDO").unwrap(),
        0x011c => out.push_str("REDO").unwrap(),
        0x5220..=0x5223 => {
            let _ = write!(out, "HOLD L{}", code & 3);
        }
        0x5260..=0x5263 | 0x5200..=0x5203 | 0x5240..=0x5243 => {
            let _ = write!(out, "LAYER {}", code & 3);
        }
        0x7700..=0x7707 => {
            let _ = write!(out, "MACRO {}", code - 0x7700 + 1);
        }
        0..=0xff => match code {
            0 => out.push_str("NONE").unwrap(),
            1 => out.push_str("TRANS").unwrap(),
            4..=29 => {
                let _ = out.push((b'A' + code as u8 - 4) as char);
            }
            30..=38 => {
                let _ = out.push((b'1' + code as u8 - 30) as char);
            }
            39 => out.push_str("0").unwrap(),
            0x3a..=0x45 => {
                let _ = write!(out, "F{}", code - 0x39);
            }
            _ => {
                let _ = out.push_str(match code {
                    0x28 => "ENTER",
                    0x29 => "ESC",
                    0x2a => "BKSP",
                    0x2b => "TAB",
                    0x2c => "SPACE",
                    0x4f => "RIGHT",
                    0x50 => "LEFT",
                    0x51 => "DOWN",
                    0x52 => "UP",
                    0xa8 => "MUTE",
                    0xa9 => "VOL +",
                    0xaa => "VOL -",
                    0xab => "NEXT",
                    0xac => "PREV",
                    0xad => "STOP",
                    0xae => "PLAY",
                    0xe0 => "CTRL",
                    0xe1 => "SHIFT",
                    0xe2 => "ALT",
                    0xe3 => "WIN",
                    _ => "",
                });
                if out.is_empty() {
                    let _ = write!(out, "0x{code:04X}");
                }
            }
        },
        0x100..=0x1fff => {
            let _ = write!(out, "M{:02X}+{}", code >> 8, label(code & 255));
        }
        _ => {
            let _ = write!(out, "0x{code:04X}");
        }
    }
    out
}

/// US ANSI text conversion used by VIA string macros; non-ASCII is rejected.
pub fn ascii_key(byte: u8) -> Option<u16> {
    Some(match byte {
        b'a'..=b'z' => (byte - b'a' + 4) as u16,
        b'A'..=b'Z' => 0x200 | (byte - b'A' + 4) as u16,
        b'1'..=b'9' => (byte - b'1' + 30) as u16,
        b'0' => 39,
        b' ' => 44,
        b'\n' => 40,
        b'\t' => 43,
        _ => {
            let plain = b"-=[]\\;'/.,`";
            let shifted = b"_+{}|:\"?><~";
            let keys = [45, 46, 47, 48, 49, 51, 52, 56, 55, 54, 53];
            if let Some(i) = plain.iter().position(|b| *b == byte) {
                keys[i]
            } else if let Some(i) = shifted.iter().position(|b| *b == byte) {
                0x200 | keys[i]
            } else if let Some(i) = b"!@#$%^&*()".iter().position(|b| *b == byte) {
                0x200 | if i == 9 { 39 } else { 30 + i as u16 }
            } else {
                return None;
            }
        }
    })
}

/// Snapshot a macro when it starts, so concurrent VIA edits cannot alter held keys.
pub struct MacroPlayer {
    bytes: [u8; MACRO_BYTES],
    cursor: usize,
    held: [u16; 8],
    release_tap: bool,
    pub active: bool,
}
pub enum MacroStep {
    Report([u8; 10]),
    Delay(u32),
}
impl Default for MacroPlayer {
    fn default() -> Self {
        Self {
            bytes: [0; MACRO_BYTES],
            cursor: 0,
            held: [0; 8],
            release_tap: false,
            active: false,
        }
    }
}
impl MacroPlayer {
    pub fn start(&mut self, map: &Keymap, index: usize) {
        *self = Self::default();
        if index >= 8 || map.bytes[PAYLOAD - 1] != 0 {
            return;
        }
        self.bytes.copy_from_slice(&map.bytes[MAP_BYTES..]);
        for _ in 0..index {
            while self.cursor < MACRO_BYTES && self.bytes[self.cursor] != 0 {
                self.cursor += 1;
            }
            self.cursor += 1;
        }
        self.active = self.cursor < MACRO_BYTES;
    }
    pub fn next_step(&mut self) -> MacroStep {
        if self.release_tap {
            self.release_tap = false;
            return MacroStep::Report(report(&self.held));
        }
        if !self.active || self.cursor >= MACRO_BYTES || self.bytes[self.cursor] == 0 {
            self.active = false;
            return MacroStep::Report([0; 10]);
        }
        let byte = self.bytes[self.cursor];
        self.cursor += 1;
        let (command, key) = if byte == 1 {
            if self.cursor + 1 >= MACRO_BYTES {
                self.active = false;
                return MacroStep::Report([0; 10]);
            }
            let command = self.bytes[self.cursor];
            self.cursor += 1;
            if command == 4 {
                let mut delay = 0u32;
                let start = self.cursor;
                while self.cursor < MACRO_BYTES && self.bytes[self.cursor].is_ascii_digit() {
                    delay = (delay * 10 + (self.bytes[self.cursor] - b'0') as u32).min(10_000);
                    self.cursor += 1;
                }
                if self.cursor == start
                    || self.cursor >= MACRO_BYTES
                    || self.bytes[self.cursor] != b'|'
                {
                    self.active = false;
                    return MacroStep::Report([0; 10]);
                }
                self.cursor += 1;
                return MacroStep::Delay(delay);
            }
            let key = self.bytes[self.cursor] as u16;
            self.cursor += 1;
            (command, key)
        } else {
            (1, ascii_key(byte).unwrap_or(0))
        };
        match command {
            1 => {
                let mut codes = [0u16; 9];
                codes[..8].copy_from_slice(&self.held);
                codes[8] = key;
                self.release_tap = true;
                MacroStep::Report(report(&codes))
            }
            2 => {
                if !self.held.contains(&key) {
                    if let Some(slot) = self.held.iter_mut().find(|k| **k == 0) {
                        *slot = key;
                    }
                }
                MacroStep::Report(report(&self.held))
            }
            3 => {
                for k in &mut self.held {
                    if *k == key {
                        *k = 0;
                    }
                }
                MacroStep::Report(report(&self.held))
            }
            _ => {
                self.active = false;
                MacroStep::Report([0; 10])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macros_release_and_delay() {
        let mut map = Keymap::default();
        let bytes = b"A\x01\x04";
        map.bytes[MAP_BYTES..MAP_BYTES + bytes.len()].copy_from_slice(bytes);
        map.bytes[MAP_BYTES + bytes.len()..MAP_BYTES + bytes.len() + 4].copy_from_slice(b"25|b");
        let mut player = MacroPlayer::default();
        player.start(&map, 0);
        assert!(matches!(player.next_step(),MacroStep::Report(r) if r[0]==2 && r[2]==4));
        assert!(matches!(player.next_step(),MacroStep::Report(r) if r==[0;10]));
        assert!(matches!(player.next_step(), MacroStep::Delay(25)));
        assert!(matches!(player.next_step(),MacroStep::Report(r) if r[2]==5));
        player.next_step();
        player.next_step();
        assert!(!player.active);
        map.bytes[PAYLOAD - 1] = 255;
        player.start(&map, 0);
        assert!(!player.active);
    }
    #[test]
    fn via_roundtrip_and_bounds() {
        let mut m = Keymap::default();
        let mut p = [0; 32];
        p[..6].copy_from_slice(&[5, 2, 3, 1, 1, 6]);
        m.via(&mut p, 0, 0);
        p[0] = 4;
        p[4] = 0;
        p[5] = 0;
        m.via(&mut p, 0, 0);
        assert_eq!(&p[4..6], &[1, 6]);
        let before = m.bytes;
        p[..4].copy_from_slice(&[19, 0, 63, 2]);
        m.via(&mut p, 0, 0);
        assert_eq!(p[0], 255);
        assert_eq!(m.bytes, before);
        p[..4].copy_from_slice(&[19, 0, 0, 29]);
        m.via(&mut p, 0, 0);
        assert_eq!(p[0], 255);
    }
    #[test]
    fn persistence_detects_torn_record() {
        let m = Keymap::default();
        let mut r = m.record(7);
        assert_eq!(Keymap::restore(&r).unwrap().0.bytes, m.bytes);
        r[80] ^= 1;
        assert!(Keymap::restore(&r).is_none());
    }
    #[test]
    fn chord_media_and_transparent() {
        assert_eq!(&report(&[0x0106])[..4], &[1, 0, 6, 0]);
        assert_eq!(&report(&[0xa9])[8..], &[0xe9, 0]);
        assert_eq!(Keymap::default().resolved(1, 6), 0x5221);
        assert_eq!(ascii_key(b'A'), Some(0x204));
    }
}
