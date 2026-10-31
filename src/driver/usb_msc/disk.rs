//! Immutable FAT12 superfloppy. Only the public VIA definition is exposed.
pub const JSON: &[u8] = include_bytes!("../../../assets/p4-desktop-via.json");
pub const BLOCK_SIZE: usize = 512;
pub const BLOCKS: u32 = 2048;
const FAT_SECTORS: u32 = 6;
const ROOT: u32 = 1 + 2 * FAT_SECTORS;
const DATA: u32 = ROOT + 4;
const FILE_CLUSTERS: usize = JSON.len().div_ceil(BLOCK_SIZE);
const _: () = assert!(FILE_CLUSTERS < (BLOCKS - DATA) as usize);

fn le16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn le32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn fat_entry(cluster: usize) -> u16 {
    match cluster {
        0 => 0xff8,
        1 => 0xfff,
        c if c >= 2 && c < FILE_CLUSTERS + 2 => {
            if c == FILE_CLUSTERS + 1 {
                0xfff
            } else {
                (c + 1) as u16
            }
        }
        _ => 0,
    }
}

pub fn read(lba: u32, b: &mut [u8; BLOCK_SIZE]) {
    b.fill(0);
    match lba {
        0 => {
            b[..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
            b[3..11].copy_from_slice(b"MSDOS5.0");
            le16(b, 11, 512);
            b[13] = 1;
            le16(b, 14, 1);
            b[16] = 2;
            le16(b, 17, 64);
            le16(b, 19, BLOCKS as u16);
            b[21] = 0xf8;
            le16(b, 22, FAT_SECTORS as u16);
            le16(b, 24, 32);
            le16(b, 26, 2);
            b[36] = 0x80;
            b[38] = 0x29;
            le32(b, 39, 0x50345649);
            b[43..54].copy_from_slice(b"P4 VIA     ");
            b[54..62].copy_from_slice(b"FAT12   ");
            b[510..].copy_from_slice(&[0x55, 0xaa]);
        }
        1..ROOT => {
            let offset = ((lba - 1) % FAT_SECTORS) as usize * BLOCK_SIZE;
            for (i, byte) in b.iter_mut().enumerate() {
                let at = offset + i;
                let pair = at / 3;
                let a = fat_entry(pair * 2);
                let c = fat_entry(pair * 2 + 1);
                *byte = match at % 3 {
                    0 => a as u8,
                    1 => ((a >> 8) | (c << 4)) as u8,
                    _ => (c >> 4) as u8,
                };
            }
        }
        ROOT => {
            b[..11].copy_from_slice(b"P4 VIA     ");
            b[11] = 8;
            // VFAT is required: JSON is a four-character extension, not 8.3.
            let alias = b"VIA~1   JSO";
            let checksum = alias
                .iter()
                .fold(0u8, |sum, c| sum.rotate_right(1).wrapping_add(*c));
            let lfn = &mut b[32..64];
            lfn.fill(0xff);
            lfn[0] = 0x41;
            lfn[11] = 0x0f;
            lfn[12] = 0;
            lfn[13] = checksum;
            le16(lfn, 26, 0);
            let positions = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
            for (i, at) in positions.into_iter().enumerate() {
                let value = if i < 8 {
                    u16::from(b"VIA.json"[i])
                } else if i == 8 {
                    0
                } else {
                    0xffff
                };
                le16(lfn, at, value);
            }
            let entry = &mut b[64..96];
            entry[..11].copy_from_slice(alias);
            entry[11] = 0x21;
            le16(entry, 16, 0x5821);
            le16(entry, 18, 0x5821);
            le16(entry, 24, 0x5821);
            le16(entry, 26, 2);
            le32(entry, 28, JSON.len() as u32);
        }
        n if n >= DATA => {
            let offset = (n - DATA) as usize * BLOCK_SIZE;
            if offset < JSON.len() {
                let count = (JSON.len() - offset).min(BLOCK_SIZE);
                b[..count].copy_from_slice(&JSON[offset..offset + count]);
            }
        }
        _ => {}
    }
}
