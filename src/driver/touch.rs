use embedded_graphics::geometry::Point;
use esp_hal::Blocking;
use esp_hal::i2c::master::I2c;

pub const WIDTH: u16 = 720;
pub const HEIGHT: u16 = 1280;
pub const MAX_TOUCHES: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contact {
    pub id: u8,
    pub position: Point,
}

pub struct Gt911 {
    address: u8,
}

impl Gt911 {
    pub fn probe(i2c: &mut I2c<'static, Blocking>) -> Option<Self> {
        for address in [0x14, 0x5d] {
            let mut id = [0u8; 3];
            match i2c.write_read(address, &[0x81, 0x40], &mut id) {
                Ok(()) => {
                    if &id != b"911" {
                        continue;
                    }
                    return Some(Self { address });
                }
                Err(_) => {}
            }
        }
        None
    }

    /// None means no usable report; an empty report means all contacts released.
    pub fn poll(
        &mut self,
        i2c: &mut I2c<'static, Blocking>,
    ) -> Option<heapless::Vec<Contact, MAX_TOUCHES>> {
        let mut status = [0u8; 1];
        if i2c
            .write_read(self.address, &[0x81, 0x4e], &mut status)
            .is_err()
        {
            return None;
        }
        let count = status[0] & 0x0f;
        // Only ready reports contain valid coordinates. Every ready report,
        // including release (0x80), must be acknowledged to allow the next one.
        if status[0] & 0x80 == 0 {
            return None;
        }
        // Each contact occupies 8 bytes starting at 0x814F:
        // track ID, X LE16, Y LE16, size LE16, reserved.
        let mut raw = [0u8; MAX_TOUCHES * 8];
        let valid = if count == 0 {
            true
        } else if count as usize <= MAX_TOUCHES {
            match i2c.write_read(self.address, &[0x81, 0x4f], &mut raw[..count as usize * 8]) {
                Ok(()) => true,
                Err(_) => false,
            }
        } else {
            false
        };
        if i2c.write(self.address, &[0x81, 0x4e, 0]).is_err() {
            return None;
        }
        if !valid {
            return None;
        }
        let mut points = heapless::Vec::new();
        for contact in raw[..count as usize * 8].chunks_exact(8) {
            points
                .push(Contact {
                    id: contact[0],
                    position: Point::new(
                        i32::from(u16::from_le_bytes([contact[1], contact[2]])),
                        i32::from(u16::from_le_bytes([contact[3], contact[4]])),
                    ),
                })
                .unwrap();
        }
        Some(points)
    }
}
