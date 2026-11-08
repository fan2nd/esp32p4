//! Homepage PC telemetry cards. USB owns reception; the display owns history.
use super::protocol::Metrics;
use core::fmt::Write;
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Arc, Circle, Line, PrimitiveStyle, Rectangle, RoundedRectangle},
    text::{Baseline, Text},
};
use heapless::String;

pub const AREA: Rectangle = Rectangle::new(Point::new(12, 182), Size::new(336, 368));
const INK: Rgb565 = Rgb565::new(4, 9, 5);
const MUTED: Rgb565 = Rgb565::new(10, 21, 11);
// Use separated blue/orange hues so both series remain distinct on the panel.
const BLUE: Rgb565 = Rgb565::new(2, 22, 27);
const ORANGE: Rgb565 = Rgb565::new(26, 24, 2);
const TEAL: Rgb565 = Rgb565::new(1, 25, 12);

#[derive(Default)]
pub struct Dashboard {
    pub live: bool,
    sample: Metrics,
    received: u64,
    cpu: [u16; 32],
    memory: [u16; 32],
}
impl Dashboard {
    /// USB enumeration alone is not a connection: only fresh valid samples count.
    pub fn update(&mut self, snapshot: Option<(Metrics, u64)>, now: u64) -> bool {
        let sample = snapshot.filter(|(_, at)| now.saturating_sub(*at) < 3000);
        let live = sample.is_some();
        let mut changed = self.live != live;
        if let Some((metrics, at)) = sample {
            if !self.live {
                self.cpu.fill(metrics.cpu as u16);
                self.memory.fill(metrics.memory as u16);
            }
            if !self.live || at != self.received || metrics != self.sample {
                self.cpu.rotate_left(1);
                self.memory.rotate_left(1);
                self.cpu[31] = metrics.cpu as u16;
                self.memory[31] = metrics.memory as u16;
                self.sample = metrics;
                self.received = at;
                changed = true;
            }
        }
        self.live = live;
        changed
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
        Circle::new(Point::new(294, 185), 6)
            .into_styled(PrimitiveStyle::with_fill(TEAL))
            .draw(target)?;
        Text::with_baseline(
            "LIVE",
            Point::new(307, 184),
            MonoTextStyle::new(&FONT_6X10, TEAL),
            Baseline::Top,
        )
        .draw(target)?;
        for (x, title, value, color) in [
            (12, "CPU", self.sample.cpu, BLUE),
            (186, "MEMORY", self.sample.memory, ORANGE),
        ] {
            RoundedRectangle::with_equal_corners(
                Rectangle::new(Point::new(x, 204), Size::new(162, 154)),
                Size::new(16, 16),
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(target)?;
            Text::with_baseline(
                title,
                Point::new(x + 14, 216),
                MonoTextStyle::new(&FONT_6X10, INK),
                Baseline::Top,
            )
            .draw(target)?;
            // A 240-degree dial keeps the readout clear while extending both round ends downward.
            draw_gauge(target, x, value, color)?;
            let mut label = String::<24>::new();
            let _ = write!(label, "{}.{:01}%", value / 10, value % 10);
            // Inter 26px readout, centered by its measured advance at the dial center.
            let width = super::font::width(&label, 26);
            super::font::draw(
                target,
                &label,
                Point::new(x + (162 - width as i32) / 2, 284),
                26,
                INK,
                Rgb565::WHITE,
            )?;
            for (offset, label) in [(17, "0"), (123, "100")] {
                Text::with_baseline(
                    label,
                    Point::new(x + offset, 340),
                    MonoTextStyle::new(&FONT_6X10, MUTED),
                    Baseline::Top,
                )
                .draw(target)?;
            }
        }
        RoundedRectangle::with_equal_corners(
            Rectangle::new(Point::new(12, 368), Size::new(336, 108)),
            Size::new(16, 16),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
        .draw(target)?;
        for (x, label, color) in [(26, "CPU", BLUE), (100, "MEMORY", ORANGE)] {
            Line::new(Point::new(x, 384), Point::new(x + 12, 384))
                .into_styled(PrimitiveStyle::with_stroke(color, 3))
                .draw(target)?;
            Text::with_baseline(
                label,
                Point::new(x + 18, 379),
                MonoTextStyle::new(&FONT_6X10, INK),
                Baseline::Top,
            )
            .draw(target)?;
        }
        // Both series share a stable 0..100% axis; autoscaling would exaggerate noise.
        for (y, label) in [(400, "100"), (428, "50"), (456, "0")] {
            Text::with_baseline(
                label,
                Point::new(24, y - 4),
                MonoTextStyle::new(&FONT_6X10, MUTED),
                Baseline::Top,
            )
            .draw(target)?;
            Line::new(Point::new(50, y), Point::new(332, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(28, 57, 29), 1))
                .draw(target)?;
        }
        for x in [50, 120, 191, 261, 332] {
            Line::new(Point::new(x, 400), Point::new(x, 456))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(28, 57, 29), 1))
                .draw(target)?;
        }
        draw_trends(target, &self.cpu, &self.memory)?;
        RoundedRectangle::with_equal_corners(
            Rectangle::new(Point::new(12, 486), Size::new(336, 62)),
            Size::new(14, 14),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
        .draw(target)?;
        for (x, label, rate, color) in [
            (26, "DOWNLOAD", self.sample.rx_kib, BLUE),
            (198, "UPLOAD", self.sample.tx_kib, TEAL),
        ] {
            Text::with_baseline(
                label,
                Point::new(x, 496),
                MonoTextStyle::new(&FONT_6X10, MUTED),
                Baseline::Top,
            )
            .draw(target)?;
            let mut text = String::<24>::new();
            if rate >= 1024 * 1024 {
                let _ = write!(
                    text,
                    "{}.{:01} GiB/s",
                    rate / (1024 * 1024),
                    (rate % (1024 * 1024)) * 10 / (1024 * 1024)
                );
            } else if rate >= 1024 {
                let _ = write!(
                    text,
                    "{}.{:01} MiB/s",
                    rate / 1024,
                    (rate % 1024) * 10 / 1024
                );
            } else {
                let _ = write!(text, "{} KiB/s", rate);
            }
            super::font::draw(target, &text, Point::new(x, 513), 18, color, Rgb565::WHITE)?;
        }
        Ok(())
    }
}

// Four subpixel bits per logical pixel preserve coverage at segment joins.
// Small local masks avoid supersampling the full PSRAM framebuffer.
struct Coverage<const N: usize> {
    pixels: [u8; N],
    width: usize,
}
impl<const N: usize> OriginDimensions for Coverage<N> {
    fn size(&self) -> Size {
        Size::new(self.width as u32 * 2, (N / self.width) as u32 * 2)
    }
}
impl<const N: usize> DrawTarget for Coverage<N> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, _) in pixels {
            if self.bounding_box().contains(p) {
                let (x, y) = (p.x as usize, p.y as usize);
                self.pixels[y / 2 * self.width + x / 2] |= 1 << ((y % 2) * 2 + x % 2);
            }
        }
        Ok(())
    }
}
fn blend_coverage(base: Rgb565, color: Rgb565, count: u32) -> Rgb565 {
    let mix = |a: u8, b: u8| ((u32::from(a) * (4 - count) + u32::from(b) * count + 2) / 4) as u8;
    Rgb565::new(
        mix(base.r(), color.r()),
        mix(base.g(), color.g()),
        mix(base.b(), color.b()),
    )
}
#[inline(never)]
fn draw_gauge<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    x: i32,
    value: u32,
    color: Rgb565,
) -> Result<(), D::Error> {
    let mut track = Coverage::<14520> {
        pixels: [0; 14520],
        width: 132,
    };
    let mut active = Coverage::<14520> {
        pixels: [0; 14520],
        width: 132,
    };
    for (mask, sweep) in [
        (&mut track, 240.0),
        (&mut active, 240.0 * value as f32 / 1000.0),
    ] {
        if sweep == 0.0 {
            continue;
        }
        let _ = Arc::new(Point::new(12, 12), 240, 150.0.deg(), sweep.deg())
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 18))
            .draw(mask);
        // End caps share the coverage mask with the arc; unions avoid dark seams.
        for angle in [150.0f32, 150.0 + sweep] {
            let radians = angle * core::f32::consts::PI / 180.0;
            let center = Point::new(
                libm::roundf(131.5 + 119.5 * libm::cosf(radians)) as i32,
                libm::roundf(131.5 + 119.5 * libm::sinf(radians)) as i32,
            );
            let _ = Circle::new(center - Point::new(9, 9), 18)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
                .draw(mask);
        }
    }
    target.draw_iter(
        track
            .pixels
            .iter()
            .zip(&active.pixels)
            .enumerate()
            .filter_map(|(i, (&t, &a))| {
                if t | a == 0 {
                    return None;
                }
                // Resolve all four samples together so the colored arc has no pale edge halo.
                let mut r = 0u32;
                let mut g = 0u32;
                let mut b = 0u32;
                for bit in 0..4 {
                    let c = if a & (1 << bit) != 0 {
                        color
                    } else if t & (1 << bit) != 0 {
                        Rgb565::new(27, 55, 28)
                    } else {
                        Rgb565::WHITE
                    };
                    r += u32::from(c.r());
                    g += u32::from(c.g());
                    b += u32::from(c.b());
                }
                Some(Pixel(
                    Point::new(x + 15 + (i % 132) as i32, 238 + (i / 132) as i32),
                    Rgb565::new(
                        ((r + 2) / 4) as u8,
                        ((g + 2) / 4) as u8,
                        ((b + 2) / 4) as u8,
                    ),
                ))
            }),
    )
}
#[inline(never)]
fn draw_trends<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    cpu: &[u16; 32],
    memory: &[u16; 32],
) -> Result<(), D::Error> {
    // 34 KiB temporary SRAM, released before rendering the next dashboard section.
    let mut cpu_mask = Coverage::<17100> {
        pixels: [0; 17100],
        width: 285,
    };
    let mut mem_mask = Coverage::<17100> {
        pixels: [0; 17100],
        width: 285,
    };
    for (history, mask) in [(cpu, &mut cpu_mask), (memory, &mut mem_mask)] {
        for i in 1..32 {
            let point = |n: usize| {
                Point::new(
                    2 + n as i32 * 564 / 31,
                    116 - i32::from(history[n]) * 112 / 1000,
                )
            };
            let _ = Line::new(point(i - 1), point(i))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 4))
                .draw(mask);
        }
    }
    target.draw_iter(
        cpu_mask
            .pixels
            .iter()
            .zip(&mem_mask.pixels)
            .enumerate()
            .filter_map(|(i, (&c, &m))| {
                if c | m == 0 {
                    return None;
                }
                let x = 49 + (i % 285) as i32;
                let y = 398 + (i / 285) as i32;
                let grid = (400..=456).contains(&y) && [50, 120, 191, 261, 332].contains(&x)
                    || (50..=332).contains(&x) && [400, 428, 456].contains(&y);
                let bg = if grid {
                    Rgb565::new(28, 57, 29)
                } else {
                    Rgb565::WHITE
                };
                // Memory is composited last, matching the original curve ordering.
                let color = blend_coverage(
                    blend_coverage(bg, BLUE, c.count_ones()),
                    ORANGE,
                    m.count_ones(),
                );
                Some(Pixel(Point::new(x, y), color))
            }),
    )
}
