//! Bounded read-only direct-access SCSI commands (SPC/SBC), one LUN.
use super::disk;

pub enum Data {
    None,
    Reply(usize),
    Blocks { lba: u32, bytes: u32 },
}
pub struct Command {
    pub data: Data,
    pub failed: bool,
}
impl Command {
    fn ok(data: Data) -> Self {
        Self {
            data,
            failed: false,
        }
    }
}
pub struct Scsi {
    sense: [u8; 3],
    ejected: bool,
    prevent: bool,
}
impl Scsi {
    pub fn new() -> Self {
        Self {
            sense: [0; 3],
            ejected: false,
            prevent: false,
        }
    }
    pub fn reset(&mut self) {
        self.sense = [0; 3];
        self.prevent = false;
    }
    pub fn unsupported_lun(&mut self) -> Command {
        self.fail(5, 0x25)
    }
    fn fail(&mut self, key: u8, asc: u8) -> Command {
        self.sense = [key, asc, 0];
        Command {
            data: Data::None,
            failed: true,
        }
    }
    pub fn command(&mut self, c: &[u8], b: &mut [u8; 64]) -> Command {
        b.fill(0);
        let needed = match c[0] >> 5 {
            0 => 6,
            1 | 2 => 10,
            4 => 16,
            5 => 12,
            _ => return self.fail(5, 0x20),
        };
        if c.len() < needed {
            return self.fail(5, 0x24);
        }
        match c[0] {
            0x12 => {
                if c[1] & !1 != 0 {
                    return self.fail(5, 0x24);
                }
                let n = if c[1] & 1 == 0 {
                    if c[2] != 0 {
                        return self.fail(5, 0x24);
                    }
                    b[1] = 0x80;
                    b[2] = 4;
                    b[3] = 2;
                    b[4] = 31;
                    b[8..16].copy_from_slice(b"P4 LAB  ");
                    b[16..32].copy_from_slice(b"VIA Definition  ");
                    b[32..36].copy_from_slice(b"0001");
                    36
                } else {
                    match c[2] {
                        0 => {
                            b[3] = 2;
                            b[4] = 0;
                            b[5] = 0x80;
                            6
                        }
                        0x80 => {
                            b[1] = 0x80;
                            b[3] = 8;
                            b[4..12].copy_from_slice(b"P4VIA001");
                            12
                        }
                        _ => return self.fail(5, 0x24),
                    }
                };
                Command::ok(Data::Reply(n.min(c[4] as usize)))
            }
            0x03 => {
                if c[1] & 1 != 0 {
                    return self.fail(5, 0x24);
                }
                b[0] = 0x70;
                b[2] = self.sense[0];
                b[7] = 10;
                b[12] = self.sense[1];
                b[13] = self.sense[2];
                self.sense = [0; 3];
                Command::ok(Data::Reply(18.min(c[4] as usize)))
            }
            0xa0 => {
                b[3] = 8;
                Command::ok(Data::Reply(16.min(be32(c, 6) as usize)))
            }
            0x00 => {
                if self.ejected {
                    self.fail(2, 0x3a)
                } else {
                    Command::ok(Data::None)
                }
            }
            0x1b => {
                if c[4] & !3 != 0 {
                    return self.fail(5, 0x24);
                }
                if c[4] & 2 != 0 {
                    if c[4] & 1 == 0 && self.prevent {
                        return self.fail(5, 0x53);
                    }
                    self.ejected = c[4] & 1 == 0;
                }
                Command::ok(Data::None)
            }
            0x1e => {
                self.prevent = c[4] & 1 != 0;
                Command::ok(Data::None)
            }
            0x1a | 0x5a => {
                let page = c[2] & 0x3f;
                if !matches!(page, 0 | 8 | 0x3f) || c[2] & 0xc0 == 0xc0 || c[3] != 0 {
                    return self.fail(5, 0x24);
                }
                let ten = c[0] == 0x5a;
                let header = if ten { 8 } else { 4 };
                b[if ten { 3 } else { 2 }] = 0x80; // Write protected.
                let desc = if c[1] & 8 == 0 { 8 } else { 0 };
                b[if ten { 7 } else { 3 }] = desc;
                if desc != 0 {
                    b[header + 1..header + 4].copy_from_slice(&disk::BLOCKS.to_be_bytes()[1..]);
                    b[header + 6] = 2;
                }
                let mut n = header + desc as usize;
                if page == 8 || page == 0x3f {
                    b[n] = 8;
                    b[n + 1] = 18;
                    n += 20;
                }
                let allocation = if ten {
                    b[..2].copy_from_slice(&((n - 2) as u16).to_be_bytes());
                    be16(c, 7) as usize
                } else {
                    b[0] = (n - 1) as u8;
                    c[4] as usize
                };
                Command::ok(Data::Reply(n.min(allocation)))
            }
            0x25 | 0x9e | 0x23 => {
                if self.ejected {
                    return self.fail(2, 0x3a);
                }
                match c[0] {
                    0x25 => {
                        b[..4].copy_from_slice(&(disk::BLOCKS - 1).to_be_bytes());
                        b[4..8].copy_from_slice(&512u32.to_be_bytes());
                        Command::ok(Data::Reply(8))
                    }
                    0x9e if c[1] == 0x10 => {
                        b[..8].copy_from_slice(&u64::from(disk::BLOCKS - 1).to_be_bytes());
                        b[8..12].copy_from_slice(&512u32.to_be_bytes());
                        Command::ok(Data::Reply(32.min(be32(c, 10) as usize)))
                    }
                    0x23 => {
                        b[3] = 8;
                        b[4..8].copy_from_slice(&disk::BLOCKS.to_be_bytes());
                        b[8] = 2;
                        b[10] = 2;
                        Command::ok(Data::Reply(12.min(be16(c, 7) as usize)))
                    }
                    _ => self.fail(5, 0x24),
                }
            }
            0x08 | 0x28 | 0xa8 | 0x88 | 0x2f => {
                if self.ejected {
                    return self.fail(2, 0x3a);
                }
                let (lba, count) = match c[0] {
                    0x08 => (
                        u64::from(be32(&[0, c[1] & 0x1f, c[2], c[3]], 0)),
                        if c[4] == 0 { 256 } else { u32::from(c[4]) },
                    ),
                    0x28 | 0x2f => (u64::from(be32(c, 2)), u32::from(be16(c, 7))),
                    0xa8 => (u64::from(be32(c, 2)), be32(c, 6)),
                    _ => (
                        u64::from_be_bytes(c[2..10].try_into().unwrap()),
                        be32(c, 10),
                    ),
                };
                if lba > u64::from(disk::BLOCKS)
                    || u64::from(count) > u64::from(disk::BLOCKS).saturating_sub(lba)
                {
                    return self.fail(5, 0x21);
                }
                if c[0] == 0x2f {
                    return if c[1] & 6 != 0 {
                        self.fail(5, 0x24)
                    } else {
                        Command::ok(Data::None)
                    };
                }
                Command::ok(Data::Blocks {
                    lba: lba as u32,
                    bytes: count * 512,
                })
            }
            0x0a | 0x2a | 0xaa | 0x8a | 0x04 | 0x15 | 0x55 | 0x42 | 0x41 | 0x93 => {
                self.fail(7, 0x27)
            }
            0x35 | 0x91 => Command::ok(Data::None), // No volatile write cache.
            _ => self.fail(5, 0x20),
        }
    }
}
fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(b[at..at + 2].try_into().unwrap())
}
fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}
