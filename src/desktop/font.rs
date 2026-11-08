//! Inter A8 atlas shared with PocketJS; no font parser or heap allocation on device.
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
const DATA: &[u8] = include_bytes!("../../assets/ui/pocket/inter16.bin");
const ATLAS: usize = 16 + 96 * 8;

pub fn width(text: &str, size: u32) -> u32 {
    text.chars()
        .map(|c| {
            let index = if c == '°' {
                95
            } else if (' '..='~').contains(&c) {
                c as usize - 32
            } else {
                31
            };
            u32::from(DATA[16 + index * 8 + 6])
        })
        .sum::<u32>()
        * size
        / 16
}

pub fn draw<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    text: &str,
    origin: Point,
    size: u32,
    foreground: Rgb565,
    background: Rgb565,
) -> Result<(), D::Error> {
    if size == 0 {
        return Ok(());
    }
    let mut advance = 0u32;
    for c in text.chars() {
        let index = if c == '°' {
            95
        } else if (' '..='~').contains(&c) {
            c as usize - 32
        } else {
            31
        };
        let cell = &DATA[ATLAS + index * 1920..ATLAS + (index + 1) * 1920];
        let offset = origin + Point::new((advance * size / 16) as i32, 0);
        let w = 20 * size / 16;
        let h = 24 * size / 16;
        // Bilinear filtering retains the atlas coverage at enlarged numeric sizes.
        for y in 0..h {
            for x in 0..w {
                let sx = x * 32 * 256 / size;
                let sy = y * 32 * 256 / size;
                let ix = (sx / 256).min(39) as usize;
                let iy = (sy / 256).min(47) as usize;
                let fx = sx % 256;
                let fy = sy % 256;
                let sample = |dx: usize, dy: usize| {
                    u32::from(cell[(iy + dy).min(47) * 40 + (ix + dx).min(39)])
                };
                let alpha = ((sample(0, 0) * (256 - fx) + sample(1, 0) * fx) * (256 - fy)
                    + (sample(0, 1) * (256 - fx) + sample(1, 1) * fx) * fy
                    + 32768)
                    / 65536;
                if alpha == 0 {
                    continue;
                }
                let mix = |a: u8, b: u8| {
                    ((u32::from(a) * (255 - alpha) + u32::from(b) * alpha + 127) / 255) as u8
                };
                let color = Rgb565::new(
                    mix(background.r(), foreground.r()),
                    mix(background.g(), foreground.g()),
                    mix(background.b(), foreground.b()),
                );
                target.draw_iter([Pixel(offset + Point::new(x as i32, y as i32), color)])?;
            }
        }
        advance += u32::from(DATA[16 + index * 8 + 6]);
    }
    Ok(())
}
