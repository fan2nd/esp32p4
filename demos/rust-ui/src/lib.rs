#![no_std]
//! Allocation-free, retained two-widget UI. All coordinates are physical pixels.
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, StrokeAlignment},
    text::Text,
};

pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;
const SCREEN: Rectangle = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT));
const SLIDER: Rectangle = Rectangle::new(Point::new(24, 60), Size::new(272, 70));
const BUTTON: Rectangle = Rectangle::new(Point::new(100, 166), Size::new(120, 44));
const BACKGROUND: Rgb565 = Rgb565::new(2, 5, 6);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Slider,
    Button,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Previous,
    Next,
    Left,
    Right,
    Activate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Down(Point),
    Move(Point),
    Up(Point),
    Cancel,
    KeyDown(Key),
    KeyUp(Key),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ValueChanged(u8),
    Clicked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capture {
    Slider,
    Button,
}

pub struct Ui {
    value: u8,
    focus: Focus,
    capture: Option<Capture>,
    pressed: bool,
    key_pressed: bool,
    dirty: Option<Rectangle>,
}
impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
impl Ui {
    pub const fn new() -> Self {
        Self {
            value: 50,
            focus: Focus::Slider,
            capture: None,
            pressed: false,
            key_pressed: false,
            dirty: Some(SCREEN),
        }
    }
    pub fn value(&self) -> u8 {
        self.value
    }
    pub fn focus(&self) -> Focus {
        self.focus
    }
    pub fn dirty(&self) -> Option<Rectangle> {
        self.dirty
    }
    fn invalidate(&mut self, rect: Rectangle) {
        let rect = rect.intersection(&SCREEN);
        if rect.size.width == 0 || rect.size.height == 0 {
            return;
        }
        self.dirty = Some(match self.dirty {
            None => rect,
            Some(old) => Rectangle::with_corners(
                old.top_left.component_min(rect.top_left),
                old.bottom_right()
                    .unwrap()
                    .component_max(rect.bottom_right().unwrap()),
            ),
        });
    }
    pub fn set_value(&mut self, value: u8) {
        let value = value.min(100);
        if value != self.value {
            self.value = value;
            self.invalidate(SLIDER);
        }
    }
    fn change_value(&mut self, value: u8) -> Option<Action> {
        if value.min(100) == self.value {
            None
        } else {
            self.set_value(value);
            Some(Action::ValueChanged(self.value))
        }
    }
    fn point_value(&mut self, p: Point) -> Option<Action> {
        let x = p.x.clamp(40, 280) - 40;
        self.change_value(((x * 100 + 120) / 240) as u8)
    }
    fn set_focus(&mut self, focus: Focus) {
        if self.focus != focus {
            self.focus = focus;
            self.invalidate(SLIDER);
            self.invalidate(BUTTON);
        }
    }
    fn cancel(&mut self) {
        self.capture = None;
        self.key_pressed = false;
        if self.pressed {
            self.pressed = false;
            self.invalidate(BUTTON);
        }
    }
    pub fn handle(&mut self, event: Event) -> Option<Action> {
        match event {
            Event::Cancel => self.cancel(),
            Event::Down(p) => {
                self.cancel();
                if SLIDER.contains(p) {
                    self.set_focus(Focus::Slider);
                    self.capture = Some(Capture::Slider);
                    return self.point_value(p);
                } else if BUTTON.contains(p) {
                    self.set_focus(Focus::Button);
                    self.capture = Some(Capture::Button);
                    self.pressed = true;
                    self.invalidate(BUTTON);
                }
            }
            Event::Move(p) => match self.capture {
                Some(Capture::Slider) => return self.point_value(p),
                Some(Capture::Button) => {
                    let pressed = BUTTON.contains(p);
                    if pressed != self.pressed {
                        self.pressed = pressed;
                        self.invalidate(BUTTON);
                    }
                }
                None => {}
            },
            Event::Up(p) => {
                let capture = self.capture;
                self.cancel();
                match capture {
                    Some(Capture::Slider) => return self.point_value(p),
                    Some(Capture::Button) if BUTTON.contains(p) => return Some(Action::Clicked),
                    _ => {}
                }
            }
            Event::KeyDown(Key::Next | Key::Previous) => {
                self.cancel();
                self.set_focus(match self.focus {
                    Focus::Slider => Focus::Button,
                    Focus::Button => Focus::Slider,
                });
            }
            Event::KeyDown(Key::Left) if self.focus == Focus::Slider && self.capture.is_none() => {
                return self.change_value(self.value.saturating_sub(1))
            }
            Event::KeyDown(Key::Right) if self.focus == Focus::Slider && self.capture.is_none() => {
                return self.change_value(self.value.saturating_add(1))
            }
            Event::KeyDown(Key::Activate)
                if self.focus == Focus::Button && self.capture.is_none() =>
            {
                self.key_pressed = true;
                self.pressed = true;
                self.invalidate(BUTTON);
            }
            Event::KeyUp(Key::Activate) if self.key_pressed => {
                self.cancel();
                return Some(Action::Clicked);
            }
            _ => {}
        }
        None
    }
    /// Draw all scene layers clipped to pending damage. Damage is retained on error.
    pub fn render<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
    ) -> Result<Option<Rectangle>, D::Error> {
        self.render_with_extra_damage(target, None)
    }
    /// Restore an older framebuffer as well as current logical damage.
    /// For strict A/B swapping, pass the PREVIOUS frame's logical `dirty()`.
    /// Save current `dirty()` before calling; do not accumulate returned unions.
    /// Returns actual rendered damage. Both logical and extra damage survive errors.
    pub fn render_with_extra_damage<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        extra: Option<Rectangle>,
    ) -> Result<Option<Rectangle>, D::Error> {
        if let Some(extra) = extra {
            self.invalidate(extra);
        }
        let Some(dirty) = self.dirty else {
            return Ok(None);
        };
        self.draw(&mut target.clipped(&dirty))?;
        self.dirty = None;
        Ok(Some(dirty))
    }
    /// Full redraw for first use of a new framebuffer, or a correctness reference.
    pub fn render_full<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        self.draw(target)
    }
    fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
        SCREEN
            .into_styled(PrimitiveStyle::with_fill(BACKGROUND))
            .draw(target)?;
        let white = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
        Text::new("RUST / TINY UI", Point::new(24, 28), white).draw(target)?;
        Text::new(
            "NO ALLOC  -  EMBEDDED GRAPHICS",
            Point::new(24, 45),
            MonoTextStyle::new(&FONT_6X10, Rgb565::new(14, 30, 17)),
        )
        .draw(target)?;
        if self.focus == Focus::Slider {
            SLIDER
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .stroke_color(Rgb565::new(10, 40, 25))
                        .stroke_width(1)
                        .stroke_alignment(StrokeAlignment::Inside)
                        .build(),
                )
                .draw(target)?;
        }
        Text::new("VALUE", Point::new(36, 77), white).draw(target)?;
        let digits = [
            b'0' + self.value / 100,
            b'0' + (self.value / 10) % 10,
            b'0' + self.value % 10,
        ];
        let text = core::str::from_utf8(&digits).unwrap_or("???");
        Text::new(text, Point::new(258, 77), white).draw(target)?;
        Rectangle::new(Point::new(40, 99), Size::new(240, 6))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(6, 15, 10)))
            .draw(target)?;
        let x = 40 + i32::from(self.value) * 240 / 100;
        Rectangle::new(Point::new(40, 99), Size::new((x - 40) as u32, 6))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(8, 54, 25)))
            .draw(target)?;
        Circle::new(Point::new(x - 8, 94), 16)
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(target)?;
        BUTTON
            .into_styled(PrimitiveStyle::with_fill(if self.pressed {
                Rgb565::new(4, 28, 15)
            } else {
                Rgb565::new(5, 42, 22)
            }))
            .draw(target)?;
        if self.focus == Focus::Button {
            BUTTON
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .stroke_color(Rgb565::WHITE)
                        .stroke_width(2)
                        .stroke_alignment(StrokeAlignment::Inside)
                        .build(),
                )
                .draw(target)?;
        }
        Text::new("RESET TO 50", Point::new(127, 191), white).draw(target)?;
        Text::new("TAB: FOCUS   ARROWS: VALUE", Point::new(24, 227), white).draw(target)?;
        Ok(())
    }
}
