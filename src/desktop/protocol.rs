//! Hardware-independent validation for the desktop's time service.
pub fn ntp_utc(reply: &[u8], nonce: [u8; 8]) -> Option<u64> {
    if reply.len() < 48
        || reply[0] & 7 != 4
        || !matches!((reply[0] >> 3) & 7, 3 | 4)
        || reply[0] >> 6 == 3
        || !(1..=15).contains(&reply[1])
        || reply[24..32] != nonce
    {
        return None;
    }
    let seconds = u32::from_be_bytes(reply[40..44].try_into().ok()?) as u64;
    // This firmware targets the current NTP era (through February 2036).
    if seconds <= 3_900_000_000 {
        return None;
    }
    Some(seconds - 2_208_988_800)
}

// PCMON1 wire compatibility with rp2350-yz; no heap allocation.
// Newline-delimited USB stream; discard oversized lines to resynchronize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    pub sequence: u32,
    pub cpu: u32, // tenths of a percent, 0..=1000
    pub memory: u32,
    pub rx_kib: u32,
    pub tx_kib: u32,
    pub uptime_s: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Message {
    Hello,
    Metrics(Metrics),
}

fn number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

pub fn parse(line: &[u8]) -> Option<Message> {
    let s = core::str::from_utf8(line).ok()?.trim_end_matches('\r');
    if s == "HELLO" {
        return Some(Message::Hello);
    }
    let mut fields = s.split(',');
    if fields.next()? != "PCMON1" {
        return None;
    }
    let m = Metrics {
        sequence: number(fields.next()?)?,
        cpu: number(fields.next()?)?,
        memory: number(fields.next()?)?,
        rx_kib: number(fields.next()?)?,
        tx_kib: number(fields.next()?)?,
        uptime_s: number(fields.next()?)?,
    };
    if fields.next().is_some() || m.cpu > 1000 || m.memory > 1000 {
        return None;
    }
    Some(Message::Metrics(m))
}

pub struct Decoder {
    bytes: [u8; 128],
    len: usize,
    dropping: bool,
}
impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}
impl Decoder {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
            dropping: false,
        }
    }
    pub fn feed(&mut self, b: u8) -> Option<Message> {
        if b == b'\n' {
            let result = if self.dropping {
                None
            } else {
                parse(&self.bytes[..self.len])
            };
            self.len = 0;
            self.dropping = false;
            return result;
        }
        if self.len == self.bytes.len() {
            self.dropping = true;
        }
        if !self.dropping {
            self.bytes[self.len] = b;
            self.len += 1;
        }
        None
    }
}
