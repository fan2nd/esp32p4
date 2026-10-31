#![no_std]
//! Fixed-capacity retained UI. Coordinates and colors belong to the caller, not a demo.
//! `Scene::update` invalidates both old and new visual extents. Rendering replays all
//! intersecting layers through a clip, including popups last. Failed draws retain damage.
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

pub use embedded_graphics::text::Alignment;
pub type WidgetId = u16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Previous,
    Next,
    Left,
    Right,
    Up,
    Down,
    Activate,
    Escape,
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
pub enum ActionKind {
    Clicked,
    ValueChanged(i32),
    SelectionChanged(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiAction {
    pub id: WidgetId,
    pub kind: ActionKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSize {
    Normal,
    Large,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub background: Rgb565,
    pub foreground: Rgb565,
    pub accent: Rgb565,
    pub border: Rgb565,
    pub pressed: Rgb565,
    pub disabled: Rgb565,
    pub radius: u32,
    pub padding: u32,
    pub font: FontSize,
    pub align: Alignment,
    pub alternate: Rgb565,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            background: Rgb565::WHITE,
            foreground: Rgb565::new(4, 12, 20),
            accent: Rgb565::new(5, 35, 29),
            border: Rgb565::new(22, 49, 29),
            pressed: Rgb565::new(23, 54, 31),
            disabled: Rgb565::new(22, 44, 22),
            radius: 8,
            padding: 8,
            font: FontSize::Normal,
            align: Alignment::Left,
            alternate: Rgb565::new(29, 61, 31),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base {
    pub id: WidgetId,
    pub bounds: Rectangle,
    pub visible: bool,
    pub enabled: bool,
    pub style: Style,
}
impl Base {
    pub fn new(id: WidgetId, bounds: Rectangle) -> Self {
        Self {
            id,
            bounds,
            visible: true,
            enabled: true,
            style: Style::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Button {
    pub base: Base,
    pub text: &'static str,
    pressed: bool,
}
impl Button {
    pub fn new(id: WidgetId, bounds: Rectangle, text: &'static str) -> Self {
        Self {
            base: Base::new(id, bounds),
            text,
            pressed: false,
        }
    }
}
/// UTF-8 inline text; truncates on a character boundary at 96 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineText {
    bytes: [u8; 96],
    len: usize,
}
impl InlineText {
    pub fn new(text: &str) -> Self {
        let mut n = text.len().min(96);
        while !text.is_char_boundary(n) {
            n -= 1;
        }
        let mut out = Self {
            bytes: [0; 96],
            len: n,
        };
        out.bytes[..n].copy_from_slice(&text.as_bytes()[..n]);
        out
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}
impl core::fmt::Write for InlineText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.len + s.len() > 96 {
            return Err(core::fmt::Error);
        }
        self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label {
    pub base: Base,
    pub text: InlineText,
}
impl Label {
    pub fn new(id: WidgetId, bounds: Rectangle, text: &str) -> Self {
        Self {
            base: Base::new(id, bounds),
            text: InlineText::new(text),
        }
    }
    pub fn set_text(&mut self, text: &str) {
        self.text = InlineText::new(text);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panel {
    pub base: Base,
}
impl Panel {
    pub fn new(id: WidgetId, bounds: Rectangle) -> Self {
        Self {
            base: Base::new(id, bounds),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slider {
    pub base: Base,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub value: i32,
}
impl Slider {
    pub fn new(id: WidgetId, bounds: Rectangle, min: i32, max: i32, step: i32, value: i32) -> Self {
        let mut s = Self {
            base: Base::new(id, bounds),
            min,
            max,
            step,
            value,
        };
        s.value = s.quantize(value as i64);
        s
    }
    fn limits(&self) -> (i64, i64) {
        (
            (self.min.min(self.max)) as i64,
            (self.min.max(self.max)) as i64,
        )
    }
    fn quantize(&self, v: i64) -> i32 {
        let (lo, hi) = self.limits();
        let step = (self.step as i64).abs().max(1);
        let v = v.clamp(lo, hi);
        if v == hi {
            return hi as i32;
        }
        (lo + ((v - lo + step / 2) / step) * step).clamp(lo, hi) as i32
    }
    fn at(&mut self, p: Point) {
        let (lo, hi) = self.limits();
        let width = self.base.bounds.size.width.saturating_sub(1).max(1) as i64;
        let x = (p.x as i64 - self.base.bounds.top_left.x as i64).clamp(0, width);
        self.value = self.quantize(lo + ((hi - lo) as u64 * x as u64 / width as u64) as i64);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComboBox {
    pub base: Base,
    pub items: &'static [&'static str],
    pub selected: usize,
    pub row_height: u32,
    pub open: bool,
}
impl ComboBox {
    pub fn new(id: WidgetId, bounds: Rectangle, items: &'static [&'static str]) -> Self {
        Self {
            base: Base::new(id, bounds),
            items,
            selected: 0,
            row_height: 30,
            open: false,
        }
    }
    pub fn popup_bounds(&self) -> Rectangle {
        Rectangle::new(
            Point::new(
                self.base.bounds.top_left.x,
                self.base
                    .bounds
                    .top_left
                    .y
                    .saturating_add(self.base.bounds.size.height.min(i32::MAX as u32) as i32),
            ),
            Size::new(
                self.base.bounds.size.width,
                self.row_height
                    .saturating_mul(self.items.len().min(u32::MAX as usize) as u32),
            ),
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListView {
    pub base: Base,
    pub items: &'static [&'static str],
    pub selected: usize,
    pub row_height: u32,
    pub scroll: u32,
    pub drag_threshold: u32,
    origin: Point,
    initial_scroll: u32,
    dragging: bool,
}
impl ListView {
    pub fn new(id: WidgetId, bounds: Rectangle, items: &'static [&'static str]) -> Self {
        Self {
            base: Base::new(id, bounds),
            items,
            selected: 0,
            row_height: 30,
            scroll: 0,
            drag_threshold: 6,
            origin: Point::zero(),
            initial_scroll: 0,
            dragging: false,
        }
    }
    pub fn max_scroll(&self) -> u32 {
        self.row_height
            .max(1)
            .saturating_mul(self.items.len().min(u32::MAX as usize) as u32)
            .saturating_sub(self.base.bounds.size.height)
    }
    fn reveal(&mut self) {
        let top = (self.selected as u32).saturating_mul(self.row_height.max(1));
        if top < self.scroll {
            self.scroll = top;
        }
        let bottom = top.saturating_add(self.row_height.max(1));
        if bottom > self.scroll.saturating_add(self.base.bounds.size.height) {
            self.scroll = bottom.saturating_sub(self.base.bounds.size.height);
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Widget {
    Button(Button),
    Slider(Slider),
    ComboBox(ComboBox),
    ListView(ListView),
    Label(Label),
    Panel(Panel),
}
macro_rules! conversions { ($($t:ident),*) => {$ (impl From<$t> for Widget {fn from(v:$t)->Self {Self::$t(v)}})*}; }
conversions!(Button, Slider, ComboBox, ListView, Label, Panel);
impl Widget {
    pub fn base(&self) -> &Base {
        match self {
            Self::Button(v) => &v.base,
            Self::Slider(v) => &v.base,
            Self::ComboBox(v) => &v.base,
            Self::ListView(v) => &v.base,
            Self::Label(v) => &v.base,
            Self::Panel(v) => &v.base,
        }
    }
    pub fn base_mut(&mut self) -> &mut Base {
        match self {
            Self::Button(v) => &mut v.base,
            Self::Slider(v) => &mut v.base,
            Self::ComboBox(v) => &mut v.base,
            Self::ListView(v) => &mut v.base,
            Self::Label(v) => &mut v.base,
            Self::Panel(v) => &mut v.base,
        }
    }
    fn interactive(&self) -> bool {
        self.base().visible
            && self.base().enabled
            && !matches!(self, Self::Label(_) | Self::Panel(_))
    }
    fn extent(&self) -> Option<Rectangle> {
        if !self.base().visible {
            return None;
        }
        let b = self.base().bounds;
        Some(match self {
            Self::ComboBox(v) if v.open => union(b, v.popup_bounds()),
            _ => b,
        })
    }
    fn cancel(&mut self) {
        match self {
            Self::Button(v) => v.pressed = false,
            Self::ListView(v) => v.dragging = false,
            Self::ComboBox(v) => v.open = false,
            _ => {}
        }
    }
}
fn union(a: Rectangle, b: Rectangle) -> Rectangle {
    let x = a.top_left.x.min(b.top_left.x);
    let y = a.top_left.y.min(b.top_left.y);
    let right =
        (a.top_left.x as i64 + a.size.width as i64).max(b.top_left.x as i64 + b.size.width as i64);
    let bottom = (a.top_left.y as i64 + a.size.height as i64)
        .max(b.top_left.y as i64 + b.size.height as i64);
    Rectangle::new(
        Point::new(x, y),
        Size::new(
            (right - x as i64).min(u32::MAX as i64) as u32,
            (bottom - y as i64).min(u32::MAX as i64) as u32,
        ),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddError {
    Full,
    DuplicateId,
}
pub struct Scene<const N: usize> {
    widgets: [Option<Widget>; N],
    len: usize,
    viewport: Rectangle,
    background: Rgb565,
    damage: Option<Rectangle>,
    capture: Option<usize>,
    focus: Option<usize>,
    key_pressed: Option<usize>,
    dismissing: bool,
}
impl<const N: usize> Scene<N> {
    pub fn new(viewport: Rectangle, background: Rgb565) -> Self {
        Self {
            widgets: [None; N],
            len: 0,
            viewport,
            background,
            damage: Some(viewport),
            capture: None,
            focus: None,
            key_pressed: None,
            dismissing: false,
        }
    }
    /// Remove all widgets in place while retaining viewport and background.
    /// Replacing a large Scene with Scene::new creates a second inline array on
    /// embedded stacks. Clearing slots avoids that temporary during page changes.
    pub fn clear(&mut self) {
        for widget in &mut self.widgets[..self.len] {
            *widget = None;
        }
        self.len = 0;
        self.capture = None;
        self.focus = None;
        self.key_pressed = None;
        self.dismissing = false;
        self.damage = Some(self.viewport);
    }

    pub fn add(&mut self, widget: impl Into<Widget>) -> Result<(), AddError> {
        let w = widget.into();
        if self.get(w.base().id).is_some() {
            return Err(AddError::DuplicateId);
        }
        if self.len == N {
            return Err(AddError::Full);
        }
        self.invalidate(w.extent());
        self.widgets[self.len] = Some(w);
        self.len += 1;
        Ok(())
    }
    pub fn get(&self, id: WidgetId) -> Option<&Widget> {
        self.widgets[..self.len]
            .iter()
            .flatten()
            .find(|w| w.base().id == id)
    }
    pub fn update(&mut self, id: WidgetId, f: impl FnOnce(&mut Widget)) -> bool {
        if let Some(i) = self.widgets[..self.len]
            .iter()
            .position(|w| w.as_ref().unwrap().base().id == id)
        {
            let old = self.widgets[i].unwrap().extent();
            f(self.widgets[i].as_mut().unwrap());
            self.widgets[i].as_mut().unwrap().base_mut().id = id;
            if !self.widgets[i].unwrap().interactive() {
                self.widgets[i].as_mut().unwrap().cancel();
                if self.capture == Some(i) {
                    self.capture = None;
                }
                if self.focus == Some(i) {
                    self.focus = None;
                }
            }
            let new = self.widgets[i].unwrap().extent();
            self.invalidate(old);
            self.invalidate(new);
            true
        } else {
            false
        }
    }
    pub fn damage(&self) -> Option<Rectangle> {
        self.damage
    }
    pub fn viewport(&self) -> Rectangle {
        self.viewport
    }
    pub fn set_background(&mut self, color: Rgb565) {
        self.background = color;
        self.invalidate(Some(self.viewport));
    }
    pub fn set_viewport(&mut self, viewport: Rectangle) {
        self.invalidate(Some(self.viewport));
        self.viewport = viewport;
        self.invalidate(Some(viewport));
    }
    pub fn invalidate(&mut self, r: Option<Rectangle>) {
        if let Some(r) = r {
            let r = r.intersection(&self.viewport);
            if r.size.width == 0 || r.size.height == 0 {
                return;
            }
            self.damage = Some(self.damage.map_or(r, |old| union(old, r)));
        }
    }
    pub fn focused(&self) -> Option<WidgetId> {
        self.focus.map(|i| self.widgets[i].unwrap().base().id)
    }
    pub fn focus(&mut self, id: WidgetId) -> bool {
        if let Some(i) = self.widgets[..self.len]
            .iter()
            .position(|w| w.as_ref().unwrap().base().id == id && w.as_ref().unwrap().interactive())
        {
            if let Some(captured) = self.capture.take() {
                let old = self.widgets[captured].unwrap().extent();
                self.widgets[captured].as_mut().unwrap().cancel();
                self.invalidate(old);
            }
            self.dismissing = false;
            self.key_pressed = None;
            self.change_focus(Some(i));
            true
        } else {
            false
        }
    }
    fn change_focus(&mut self, next: Option<usize>) {
        if self.focus == next {
            return;
        }
        if let Some(i) = self.focus {
            let old = self.widgets[i].unwrap().extent();
            self.widgets[i].as_mut().unwrap().cancel();
            self.invalidate(old);
        }
        self.key_pressed = None;
        self.focus = next;
        if let Some(i) = next {
            self.invalidate(self.widgets[i].unwrap().extent());
        }
    }
    pub fn handle(&mut self, event: Event) -> Option<UiAction> {
        // A pointer gesture owns input until release/cancel. Focus navigation is
        // an explicit handoff; other keyboard input cannot activate it twice.
        if matches!(event, Event::KeyDown(Key::Escape)) {
            return self.handle(Event::Cancel);
        }
        if self.capture.is_some()
            && matches!(event,Event::KeyDown(k) if !matches!(k,Key::Next|Key::Previous))
        {
            return None;
        }
        if matches!(event, Event::Down(_)) {
            if let Some(i) = self.key_pressed.take() {
                if let Some(Widget::Button(v)) = self.widgets[i].as_mut() {
                    v.pressed = false;
                }
                self.invalidate(self.widgets[i].unwrap().extent());
            }
        }
        if matches!(event, Event::KeyDown(Key::Next | Key::Previous)) {
            if let Some(i) = self.capture.take() {
                let old = self.widgets[i].unwrap().extent();
                self.widgets[i].as_mut().unwrap().cancel();
                self.invalidate(old);
            }
            if let Some(i) = self.focus {
                let old = self.widgets[i].unwrap().extent();
                self.widgets[i].as_mut().unwrap().cancel();
                self.invalidate(old);
            }
            self.key_pressed = None;
            self.dismissing = false;
            let backwards = matches!(event, Event::KeyDown(Key::Previous));
            for offset in 1..=self.len {
                let i = if backwards {
                    (self.focus.unwrap_or(0) + self.len - offset) % self.len
                } else {
                    (self.focus.unwrap_or(self.len.saturating_sub(1)) + offset) % self.len
                };
                if self.widgets[i].unwrap().interactive() {
                    self.change_focus(Some(i));
                    break;
                }
            }
            return None;
        }
        if matches!(event, Event::KeyDown(Key::Activate)) {
            if self.key_pressed.is_some() {
                return None;
            }
            self.key_pressed = self.focus;
        }
        if matches!(event, Event::KeyUp(Key::Activate)) && self.key_pressed.take() != self.focus {
            return None;
        }
        if matches!(event, Event::Cancel | Event::KeyDown(Key::Escape)) {
            self.key_pressed = None;
        }
        if self.dismissing {
            if matches!(event, Event::Up(_) | Event::Cancel) {
                self.dismissing = false;
                self.capture = None;
            }
            return None;
        }
        let popup=(0..self.len).rev().find(|&i|matches!(self.widgets[i],Some(Widget::ComboBox(v)) if v.open&&v.base.visible&&v.base.enabled));
        if let Event::Down(p) = event {
            if let Some(old) = self.capture.take() {
                let w = self.widgets[old].as_mut().unwrap();
                let r = w.extent();
                w.cancel();
                self.invalidate(r);
            }
            let hit = popup.or_else(|| {
                (0..self.len).rev().find(|&i| {
                    let w = self.widgets[i].unwrap();
                    w.interactive() && w.base().bounds.contains(p)
                })
            });
            self.capture = hit;
            if let Some(i) = hit {
                self.change_focus(Some(i));
            }
            if let Some(i) = popup {
                if let Some(Widget::ComboBox(v)) = self.widgets[i] {
                    if !v.base.bounds.contains(p) && !v.popup_bounds().contains(p) {
                        self.dismissing = true;
                    }
                }
            }
        }
        let index = match event {
            Event::Down(_) | Event::Move(_) | Event::Up(_) => self.capture,
            Event::Cancel => self.capture.or(popup).or(self.focus),
            Event::KeyDown(_) | Event::KeyUp(_) => popup.or(self.focus),
        };
        let i = index?;
        let w = self.widgets[i].as_mut().unwrap();
        let original = *w;
        let old = w.extent();
        let id = w.base().id;
        let mut action = None;
        match w {
            Widget::Button(v) => match event {
                Event::Down(_) | Event::KeyDown(Key::Activate) => v.pressed = true,
                Event::Move(p) => v.pressed = v.base.bounds.contains(p),
                Event::Up(p) => {
                    if v.pressed && v.base.bounds.contains(p) {
                        action = Some(ActionKind::Clicked);
                    }
                    v.pressed = false;
                }
                Event::KeyUp(Key::Activate) => {
                    if v.pressed {
                        action = Some(ActionKind::Clicked);
                    }
                    v.pressed = false;
                }
                Event::Cancel | Event::KeyDown(Key::Escape) => v.pressed = false,
                _ => {}
            },
            Widget::Slider(v) => {
                let prev = v.value;
                match event {
                    Event::Down(p) | Event::Move(p) | Event::Up(p) => v.at(p),
                    Event::KeyDown(Key::Left | Key::Down) => {
                        v.value = v.quantize(v.value as i64 - (v.step as i64).abs().max(1))
                    }
                    Event::KeyDown(Key::Right | Key::Up) => {
                        v.value = v.quantize(v.value as i64 + (v.step as i64).abs().max(1))
                    }
                    _ => {}
                }
                if v.value != prev {
                    action = Some(ActionKind::ValueChanged(v.value));
                }
            }
            Widget::ComboBox(v) => match event {
                Event::Up(p) => {
                    if v.open {
                        let popup = v.popup_bounds();
                        if popup.contains(p) && v.row_height > 0 {
                            let row = ((p.y as i64 - popup.top_left.y as i64) / v.row_height as i64)
                                as usize;
                            if row < v.items.len() {
                                v.selected = row;
                                action = Some(ActionKind::SelectionChanged(row));
                            }
                        }
                        v.open = false;
                    } else if v.base.bounds.contains(p) {
                        v.open = true;
                    }
                }
                Event::Down(p)
                    if v.open && !v.popup_bounds().contains(p) && !v.base.bounds.contains(p) =>
                {
                    v.open = false
                }
                Event::Cancel | Event::KeyDown(Key::Escape) => v.open = false,
                Event::KeyDown(Key::Activate) => v.open = !v.open,
                Event::KeyDown(Key::Up | Key::Down) if !v.items.is_empty() => {
                    let old = v.selected;
                    v.selected = if old >= v.items.len() {
                        0
                    } else if matches!(event, Event::KeyDown(Key::Up)) {
                        old.saturating_sub(1)
                    } else {
                        (old + 1).min(v.items.len() - 1)
                    };
                    if old != v.selected {
                        action = Some(ActionKind::SelectionChanged(v.selected));
                    }
                }
                _ => {}
            },
            Widget::ListView(v) => match event {
                Event::Down(p) => {
                    v.origin = p;
                    v.scroll = v.scroll.min(v.max_scroll());
                    v.initial_scroll = v.scroll;
                    v.dragging = false;
                }
                Event::Move(p) => {
                    let dy = p.y as i64 - v.origin.y as i64;
                    let dx = p.x as i64 - v.origin.x as i64;
                    if dy.abs().max(dx.abs()) > v.drag_threshold as i64 {
                        v.dragging = true;
                    }
                    if v.dragging {
                        v.scroll =
                            (v.initial_scroll as i64 - dy).clamp(0, v.max_scroll() as i64) as u32;
                    }
                }
                Event::Up(p) => {
                    let moved = (p.y as i64 - v.origin.y as i64)
                        .abs()
                        .max((p.x as i64 - v.origin.x as i64).abs())
                        > v.drag_threshold as i64;
                    if !v.dragging && !moved && v.base.bounds.contains(p) {
                        let row = ((p.y as i64 - v.base.bounds.top_left.y as i64
                            + v.scroll.min(v.max_scroll()) as i64)
                            / v.row_height.max(1) as i64)
                            as usize;
                        if row < v.items.len() {
                            v.selected = row;
                            action = Some(ActionKind::SelectionChanged(row));
                        }
                    }
                    v.dragging = false;
                }
                Event::KeyDown(Key::Up | Key::Down) => {
                    if !v.items.is_empty() {
                        let old = v.selected;
                        v.selected = if old >= v.items.len() {
                            0
                        } else if matches!(event, Event::KeyDown(Key::Up)) {
                            old.saturating_sub(1)
                        } else {
                            (old + 1).min(v.items.len() - 1)
                        };
                        v.reveal();
                        if old != v.selected {
                            action = Some(ActionKind::SelectionChanged(v.selected));
                        }
                    }
                }
                Event::Cancel => v.dragging = false,
                _ => {}
            },
            _ => {}
        }
        let new = w.extent();
        if *w != original {
            self.invalidate(old);
            self.invalidate(new);
        }
        if matches!(event, Event::Up(_) | Event::Cancel) {
            self.capture = None;
        }
        action.map(|kind| UiAction { id, kind })
    }
    pub fn render<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
    ) -> Result<Option<Rectangle>, D::Error> {
        self.render_with_extra_damage(target, None)
    }
    /// Draw a complete reference frame without consuming pending damage.
    pub fn render_full<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        self.draw_region(target, self.viewport)
    }
    pub fn render_with_extra_damage<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        extra: Option<Rectangle>,
    ) -> Result<Option<Rectangle>, D::Error> {
        self.invalidate(extra);
        let rendered = self.damage;
        if let Some(damage) = rendered {
            self.draw_region(target, damage)?;
            self.damage = None;
        }
        Ok(rendered)
    }
    fn draw_region<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
        damage: Rectangle,
    ) -> Result<(), D::Error> {
        let clip = damage.intersection(&self.viewport);
        let mut target = target.clipped(&clip);
        target.fill_solid(&clip, self.background)?;
        for (i, w) in self.widgets[..self.len].iter().enumerate() {
            let w = w.as_ref().unwrap();
            if w.base().visible {
                draw_widget(w, &mut target)?;
                if self.focus == Some(i) {
                    RoundedRectangle::with_equal_corners(
                        w.base().bounds,
                        Size::new(w.base().style.radius, w.base().style.radius),
                    )
                    .into_styled(PrimitiveStyle::with_stroke(w.base().style.accent, 1))
                    .draw(&mut target)?;
                }
            }
        }
        for w in self.widgets[..self.len].iter().flatten() {
            if let Widget::ComboBox(v) = w {
                if v.base.visible && v.open {
                    draw_items(
                        &mut target,
                        v.popup_bounds(),
                        v.items,
                        v.selected,
                        v.row_height,
                        0,
                        v.base.style,
                    )?;
                }
            }
        }
        Ok(())
    }
}
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
fn draw_items<D: DrawTarget<Color = Rgb565>>(
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
fn draw_widget<D: DrawTarget<Color = Rgb565>>(w: &Widget, t: &mut D) -> Result<(), D::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::{mock_display::MockDisplay, Pixel};
    fn r(x: i32, y: i32, w: u32, h: u32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }
    fn scene() -> Scene<8> {
        Scene::new(r(0, 0, 64, 64), Rgb565::BLACK)
    }
    fn display() -> MockDisplay<Rgb565> {
        let mut d = MockDisplay::new();
        d.set_allow_overdraw(true);
        d
    }
    #[test]
    fn bounded_duplicate_and_independent() {
        let mut a = Scene::<1>::new(r(0, 0, 10, 10), Rgb565::WHITE);
        let mut b = scene();
        a.add(Button::new(8, r(0, 0, 5, 5), "a")).unwrap();
        assert_eq!(
            a.add(Button::new(8, r(0, 0, 5, 5), "b")),
            Err(AddError::DuplicateId)
        );
        assert_eq!(
            a.add(Button::new(9, r(0, 0, 5, 5), "b")),
            Err(AddError::Full)
        );
        b.add(Button::new(8, r(30, 30, 10, 10), "b")).unwrap();
        a.handle(Event::Down(Point::new(2, 2)));
        assert_eq!(a.handle(Event::Up(Point::new(2, 2))).unwrap().id, 8);
        assert_eq!(b.handle(Event::Up(Point::new(2, 2))), None);
    }
    #[test]
    fn capture_cancel_and_hide() {
        let mut s = scene();
        s.add(Button::new(1, r(5, 5, 20, 20), "ok")).unwrap();
        s.handle(Event::Down(Point::new(8, 8)));
        s.handle(Event::Move(Point::new(40, 40)));
        assert_eq!(s.handle(Event::Up(Point::new(40, 40))), None);
        s.handle(Event::Down(Point::new(8, 8)));
        s.handle(Event::Cancel);
        assert_eq!(s.handle(Event::Up(Point::new(8, 8))), None);
        s.handle(Event::Down(Point::new(8, 8)));
        s.update(1, |w| w.base_mut().visible = false);
        assert_eq!(s.handle(Event::Up(Point::new(8, 8))), None);
    }
    #[test]
    fn arbitrary_slider_ranges() {
        let mut s = scene();
        s.add(Slider::new(
            1,
            r(5, 5, 21, 10),
            i32::MIN,
            i32::MAX,
            i32::MIN,
            0,
        ))
        .unwrap();
        assert_eq!(
            s.handle(Event::Down(Point::new(5, 8))).unwrap().kind,
            ActionKind::ValueChanged(i32::MIN)
        );
        assert_eq!(
            s.handle(Event::Move(Point::new(25, 8))).unwrap().kind,
            ActionKind::ValueChanged(i32::MAX)
        );
        s.handle(Event::Cancel);
        s.update(1, |w| {
            if let Widget::Slider(v) = w {
                *v = Slider::new(1, r(0, 0, 11, 10), 100, -100, 7, -99)
            }
        });
        s.handle(Event::Down(Point::new(10, 4)));
        assert!(matches!(
            s.get(1),
            Some(Widget::Slider(Slider { value: 100, .. }))
        ));
        let z = Slider::new(9, r(0, 0, 0, 0), 7, 7, 0, 999);
        assert_eq!(z.value, 7);
    }
    #[test]
    fn popup_and_list() {
        let mut s = scene();
        s.add(ComboBox::new(1, r(0, 0, 30, 15), &["a", "b"]))
            .unwrap();
        s.update(1, |w| {
            if let Widget::ComboBox(v) = w {
                v.row_height = 15
            }
        });
        s.add(ListView::new(
            2,
            r(32, 0, 30, 30),
            &["0", "1", "2", "3", "4"],
        ))
        .unwrap();
        s.update(2, |w| {
            if let Widget::ListView(v) = w {
                v.row_height = 10
            }
        });
        s.handle(Event::Down(Point::new(2, 2)));
        s.handle(Event::Up(Point::new(2, 2)));
        s.handle(Event::Down(Point::new(2, 35)));
        assert_eq!(
            s.handle(Event::Up(Point::new(2, 35))).unwrap().kind,
            ActionKind::SelectionChanged(1)
        );
        s.handle(Event::Down(Point::new(35, 20)));
        s.handle(Event::Move(Point::new(35, 0)));
        assert_eq!(s.handle(Event::Up(Point::new(35, 0))), None);
        assert!(matches!(
            s.get(2),
            Some(Widget::ListView(ListView {
                scroll: 20,
                selected: 0,
                ..
            }))
        ));
        s.focus(2);
        for _ in 0..8 {
            s.handle(Event::KeyDown(Key::Down));
        }
        assert!(matches!(
            s.get(2),
            Some(Widget::ListView(ListView {
                scroll: 20,
                selected: 4,
                ..
            }))
        ));
    }
    #[test]
    fn dirty_matches_full_and_ab_buffers() {
        let mut s = scene();
        s.add(Button::new(1, r(2, 2, 25, 20), "A")).unwrap();
        s.add(ComboBox::new(2, r(10, 20, 40, 15), &["a", "b"]))
            .unwrap();
        s.update(2, |w| {
            if let Widget::ComboBox(v) = w {
                v.row_height = 12
            }
        });
        let mut a = display();
        let mut b = display();
        s.render_full(&mut a).unwrap();
        s.render(&mut b).unwrap();
        let mut previous = None;
        for n in 0..8 {
            match n {
                0 => {
                    s.handle(Event::Down(Point::new(3, 3)));
                }
                1 => {
                    s.handle(Event::Cancel);
                }
                2 => {
                    s.update(1, |w| w.base_mut().bounds = r(30, 1, 30, 18));
                }
                3 => {
                    s.handle(Event::Down(Point::new(12, 22)));
                    s.handle(Event::Up(Point::new(12, 22)));
                }
                4 => {
                    s.update(1, |w| w.base_mut().style.background = Rgb565::RED);
                }
                5 => {
                    s.handle(Event::Cancel);
                }
                6 => {
                    s.update(1, |w| w.base_mut().visible = false);
                }
                _ => {
                    s.update(1, |w| w.base_mut().visible = true);
                }
            }
            let dirty = s.damage();
            let target = if n % 2 == 0 { &mut a } else { &mut b };
            s.render_with_extra_damage(target, previous).unwrap();
            previous = dirty;
            let mut full = display();
            s.render_full(&mut full).unwrap();
            assert_eq!(*target, full, "frame {n}");
        }
    }
    struct Fails;
    impl OriginDimensions for Fails {
        fn size(&self) -> Size {
            Size::new(64, 64)
        }
    }
    impl DrawTarget for Fails {
        type Color = Rgb565;
        type Error = ();
        fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, _: I) -> Result<(), ()> {
            Err(())
        }
    }
    #[test]
    fn failed_render_retains_damage() {
        let mut s = scene();
        let before = s.damage();
        assert!(s.render(&mut Fails).is_err());
        assert_eq!(s.damage(), before);
    }
    #[test]
    fn geometry_invalidation_is_local() {
        let mut s = scene();
        s.add(Button::new(1, r(1, 1, 10, 10), "x")).unwrap();
        s.render(&mut display()).unwrap();
        s.update(1, |w| w.base_mut().bounds = r(20, 20, 10, 10));
        assert_eq!(s.damage(), Some(r(1, 1, 29, 29)));
    }
    #[test]
    fn inline_text_utf8() {
        let t = InlineText::new("éééééééééééééééééééééééééééééééééééééééééééééééééééééé");
        assert_eq!(t.as_str().len(), 96);
    }
    #[test]
    fn keyboard_focus_cancel_and_repeat() {
        let mut s = scene();
        s.add(Button::new(1, r(0, 0, 20, 20), "a")).unwrap();
        s.add(Button::new(2, r(20, 0, 20, 20), "b")).unwrap();
        s.focus(1);
        s.handle(Event::KeyDown(Key::Activate));
        s.handle(Event::KeyDown(Key::Next));
        assert_eq!(s.handle(Event::KeyUp(Key::Activate)), None);
        s.focus(1);
        assert_eq!(s.handle(Event::KeyUp(Key::Activate)), None);
        s.handle(Event::KeyDown(Key::Activate));
        s.handle(Event::Cancel);
        assert_eq!(s.handle(Event::KeyUp(Key::Activate)), None);
        s.handle(Event::Down(Point::new(1, 1)));
        assert_eq!(s.handle(Event::KeyUp(Key::Activate)), None);
        assert_eq!(
            s.handle(Event::Up(Point::new(1, 1))).unwrap().kind,
            ActionKind::Clicked
        );
    }
    #[test]
    fn popup_outside_dismiss_swallows_release() {
        let mut s = scene();
        s.add(ComboBox::new(1, r(0, 0, 20, 20), &["a"])).unwrap();
        s.focus(1);
        s.handle(Event::KeyDown(Key::Activate));
        s.handle(Event::KeyDown(Key::Activate));
        assert!(matches!(
            s.get(1),
            Some(Widget::ComboBox(ComboBox { open: true, .. }))
        ));
        s.handle(Event::Down(Point::new(60, 60)));
        s.handle(Event::Up(Point::new(1, 1)));
        assert!(matches!(
            s.get(1),
            Some(Widget::ComboBox(ComboBox { open: false, .. }))
        ));
    }
    #[test]
    fn no_change_no_damage_full_is_read_only() {
        let mut s = scene();
        s.add(Slider::new(1, r(0, 0, 20, 20), 0, 100, 1, 0))
            .unwrap();
        s.focus(1);
        s.render(&mut display()).unwrap();
        s.handle(Event::KeyDown(Key::Left));
        assert_eq!(s.damage(), None);
        s.invalidate(Some(r(100, 100, 10, 10)));
        assert_eq!(s.damage(), None);
        s.update(1, |w| w.base_mut().id = 99);
        assert!(s.get(1).is_some());
        let d = s.damage();
        s.render_full(&mut display()).unwrap();
        assert_eq!(s.damage(), d);
    }
    #[test]
    fn full_width_slider_math_and_disabled_popup() {
        let mut v = Slider::new(1, r(i32::MIN, 0, u32::MAX, 10), i32::MIN, i32::MAX, 1, 0);
        v.at(Point::new(i32::MAX, 0));
        assert_eq!(v.value, i32::MAX);
        let mut s = scene();
        s.add(ComboBox::new(1, r(0, 0, 20, 20), &["a"])).unwrap();
        s.focus(1);
        s.handle(Event::KeyDown(Key::Activate));
        s.update(1, |w| w.base_mut().enabled = false);
        assert!(matches!(
            s.get(1),
            Some(Widget::ComboBox(ComboBox { open: false, .. }))
        ));
    }
    #[test]
    fn mixed_input_single_owner() {
        let mut s = scene();
        s.add(Button::new(1, r(0, 0, 20, 20), "b")).unwrap();
        s.add(Slider::new(2, r(20, 0, 20, 20), 0, 100, 1, 0))
            .unwrap();
        s.focus(1);
        s.handle(Event::KeyDown(Key::Activate));
        s.handle(Event::Down(Point::new(2, 2)));
        assert_eq!(s.handle(Event::KeyUp(Key::Activate)), None);
        s.handle(Event::KeyDown(Key::Activate));
        assert_eq!(s.handle(Event::KeyUp(Key::Activate)), None);
        assert_eq!(
            s.handle(Event::Up(Point::new(2, 2))).unwrap().kind,
            ActionKind::Clicked
        );
        s.handle(Event::Down(Point::new(20, 2)));
        s.handle(Event::KeyDown(Key::Next));
        assert_eq!(s.handle(Event::Move(Point::new(39, 2))), None);
        assert!(matches!(
            s.get(2),
            Some(Widget::Slider(Slider { value: 0, .. }))
        ));
        s.handle(Event::Down(Point::new(2, 2)));
        s.update(1, |w| w.base_mut().enabled = false);
        assert_eq!(s.handle(Event::Up(Point::new(2, 2))), None);
    }
    #[test]
    fn tab_closes_popup_even_only_focusable() {
        let mut s = scene();
        s.add(ComboBox::new(1, r(0, 0, 20, 20), &["a", "b"]))
            .unwrap();
        s.focus(1);
        s.handle(Event::KeyDown(Key::Activate));
        s.handle(Event::KeyDown(Key::Next));
        assert!(matches!(
            s.get(1),
            Some(Widget::ComboBox(ComboBox { open: false, .. }))
        ));
        assert_eq!(s.capture, None);
    }
    #[test]
    fn tiny_slider_damage_matches_full() {
        for h in [0, 1, 2, 3] {
            let mut s = scene();
            s.add(Slider::new(1, r(10, 10, 30, h), 0, 100, 1, 0))
                .unwrap();
            let mut actual = display();
            s.render(&mut actual).unwrap();
            s.update(1, |w| {
                if let Widget::Slider(v) = w {
                    v.value = 100
                }
            });
            s.render(&mut actual).unwrap();
            let mut expected = display();
            s.render_full(&mut expected).unwrap();
            assert_eq!(actual, expected, "height {h}");
        }
    }
    #[test]
    fn programmatic_focus_cancels_capture() {
        let mut s = scene();
        s.add(Slider::new(1, r(0, 0, 20, 20), 0, 100, 1, 0))
            .unwrap();
        s.add(Button::new(2, r(20, 0, 20, 20), "b")).unwrap();
        s.handle(Event::Down(Point::new(0, 1)));
        s.focus(2);
        assert_eq!(s.handle(Event::Move(Point::new(19, 1))), None);
        assert_eq!(s.handle(Event::Up(Point::new(19, 1))), None);
    }
    #[test]
    fn out_of_range_list_scroll_hit_matches_render() {
        let mut s = scene();
        let mut list = ListView::new(1, r(0, 0, 20, 20), &["0", "1", "2", "3"]);
        list.row_height = 10;
        list.scroll = u32::MAX;
        s.add(list).unwrap();
        s.handle(Event::Down(Point::new(2, 2)));
        assert_eq!(
            s.handle(Event::Up(Point::new(2, 2))).unwrap().kind,
            ActionKind::SelectionChanged(2)
        );
    }
}
