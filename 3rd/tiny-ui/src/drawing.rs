//! Software rasterization of widgets and popup layers.
use crate::{Alignment, FontSize, Style, Widget};
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_10X20, FONT_9X18},
        MonoTextStyle,
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle, RoundedRectangle},
    text::{Baseline, Text, TextStyleBuilder},
};

fn card<D: DrawTarget<Color = Rgb565>>(
    t: &mut D,
    b: Rectangle,
    s: Style,
    color: Rgb565,
) -> Result<(), D::Error> {
    RoundedRectangle::with_equal_corners(b, Size::new(s.radius, s.radius))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(t)
}
fn text<D: DrawTarget<Color = Rgb565>>(
    t: &mut D,
    b: Rectangle,
    s: Style,
    value: &str,
) -> Result<(), D::Error> {
    let mut t = t.clipped(&b);
    let font = match s.font {
        FontSize::Normal => &FONT_9X18,
        FontSize::Large => &FONT_10X20,
    };
    Text::with_text_style(
        value,
        b.top_left
            + Point::new(
                match s.align {
                    Alignment::Left => s.padding.min(i32::MAX as u32) as i32,
                    Alignment::Center => (b.size.width / 2).min(i32::MAX as u32) as i32,
                    Alignment::Right => b
                        .size
                        .width
                        .saturating_sub(s.padding.saturating_add(1))
                        .min(i32::MAX as u32) as i32,
                },
                ((b.size.height as i64 - font.character_size.height as i64) / 2).max(0) as i32,
            ),
        MonoTextStyle::new(font, s.foreground),
        TextStyleBuilder::new()
            .baseline(Baseline::Top)
            .alignment(s.align)
            .build(),
    )
    .draw(&mut t)
    .map(|_| ())
}
pub(crate) fn draw_items<D: DrawTarget<Color = Rgb565>>(
    t: &mut D,
    b: Rectangle,
    items: &[&str],
    selected: usize,
    row_height: u32,
    scroll: u32,
    s: Style,
) -> Result<(), D::Error> {
    let mut t = t.clipped(&b);
    t.fill_solid(&b, s.background)?;
    let rh = row_height.max(1);
    let first = (scroll / rh) as usize;
    let count = (b.size.height / rh).saturating_add(2) as usize;
    for (i, label) in items.iter().enumerate().skip(first).take(count) {
        let y = (b.top_left.y as i64 + i as i64 * rh as i64 - scroll as i64)
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let row = Rectangle::new(Point::new(b.top_left.x, y), Size::new(b.size.width, rh));
        if i == selected {
            t.fill_solid(&row, s.pressed)?;
        } else if i % 2 == 1 {
            t.fill_solid(&row, s.alternate)?;
        }
        text(&mut t, row, s, label)?;
    }
    Ok(())
}
pub(crate) fn draw_widget<D: DrawTarget<Color = Rgb565>>(
    w: &Widget,
    t: &mut D,
) -> Result<(), D::Error> {
    let b = w.base().bounds;
    let s = w.base().style;
    match w {
        Widget::Panel(_) => card(t, b, s, s.background),
        Widget::Label(v) => text(t, b, s, v.text.as_str()),
        Widget::Button(v) => {
            card(
                t,
                b,
                s,
                if !v.base.enabled {
                    s.disabled
                } else if v.pressed {
                    s.pressed
                } else {
                    s.background
                },
            )?;
            text(t, b, s, v.text)
        }
        Widget::Slider(v) => {
            let mut clipped = t.clipped(&b);
            let t = &mut clipped;
            let (lo, hi) = v.limits();
            let width = b.size.width.saturating_sub(1);
            let x = if hi == lo {
                0
            } else {
                (((v.value as i64).clamp(lo, hi) - lo) as u64 * width as u64 / (hi - lo) as u64)
                    as i64
            };
            let y = b.top_left.y.saturating_add((b.size.height / 2) as i32);
            t.fill_solid(
                &Rectangle::new(
                    Point::new(b.top_left.x, y.saturating_sub(2)),
                    Size::new(b.size.width, 4),
                ),
                s.border,
            )?;
            t.fill_solid(
                &Rectangle::new(
                    Point::new(b.top_left.x, y.saturating_sub(2)),
                    Size::new(x as u32, 4),
                ),
                s.accent,
            )?;
            let knob = Rectangle::new(
                Point::new(
                    b.top_left.x.saturating_add(x as i32).saturating_sub(6),
                    y.saturating_sub(9),
                ),
                Size::new(12, 18),
            );
            card(t, knob, s, s.accent)
        }
        Widget::ComboBox(v) => {
            card(t, b, s, s.background)?;
            let text_bounds = Rectangle::new(
                b.top_left,
                Size::new(b.size.width.saturating_sub(22), b.size.height),
            );
            text(
                t,
                text_bounds,
                s,
                v.items.get(v.selected).copied().unwrap_or(""),
            )?;
            let mut t = t.clipped(&b);
            let x = b
                .top_left
                .x
                .saturating_add(b.size.width.saturating_sub(14).min(i32::MAX as u32) as i32);
            let y = b
                .top_left
                .y
                .saturating_add((b.size.height / 2).min(i32::MAX as u32) as i32);
            Line::new(Point::new(x - 4, y - 2), Point::new(x, y + 2))
                .into_styled(PrimitiveStyle::with_stroke(s.foreground, 1))
                .draw(&mut t)?;
            Line::new(Point::new(x, y + 2), Point::new(x + 4, y - 2))
                .into_styled(PrimitiveStyle::with_stroke(s.foreground, 1))
                .draw(&mut t)
        }
        Widget::ListView(v) => draw_items(
            t,
            b,
            v.items,
            v.selected,
            v.row_height,
            v.scroll.min(v.max_scroll()),
            s,
        ),
    }
}
