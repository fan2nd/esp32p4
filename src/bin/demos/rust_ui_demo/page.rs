//! Showcase page composed from the independent `tiny-ui` crate.
//!
//! This module owns demo IDs, labels, state, and reset policy. The component crate
//! knows nothing about volume, modes, this screen size, or the ESP32-P4.
use core::fmt::Write;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
pub use tiny_ui::Event;
use tiny_ui::{
    ActionKind, Alignment, Button, ComboBox, FontSize, InlineText, Label, ListView, Panel, Scene,
    Slider, Style, Widget, WidgetId,
};

pub const WIDTH: u32 = 360;
pub const HEIGHT: u32 = 640;
pub const VOLUME: WidgetId = 10;
pub const MODE: WidgetId = 11;
pub const ITEMS: WidgetId = 12;
pub const RESET: WidgetId = 13;
const VOLUME_LABEL: WidgetId = 3;
const SELECTION_LABEL: WidgetId = 5;
const MODES: &[&str] = &["Normal", "Quiet", "Music", "Game"];
const ROWS: &[&str] = &[
    "Item 1", "Item 2", "Item 3", "Item 4", "Item 5", "Item 6", "Item 7", "Item 8", "Item 9",
    "Item 10", "Item 11", "Item 12", "Item 13", "Item 14", "Item 15", "Item 16", "Item 17",
    "Item 18", "Item 19", "Item 20", "Item 21", "Item 22", "Item 23", "Item 24", "Item 25",
    "Item 26", "Item 27", "Item 28", "Item 29", "Item 30",
];
fn rect(x: i32, y: i32, w: u32, h: u32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(w, h))
}
fn rgb(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}
fn style() -> Style {
    Style {
        background: Rgb565::WHITE,
        foreground: rgb(0x18, 0x2d, 0x44),
        accent: rgb(0x26, 0x7b, 0x9a),
        border: rgb(0xcd, 0xe4, 0xef),
        pressed: rgb(0xc1, 0xea, 0xfa),
        disabled: rgb(0xcd, 0xe4, 0xef),
        radius: 10,
        alternate: rgb(0xf0, 0xf7, 0xfa),
        ..Style::default()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub volume: i32,
    pub mode: usize,
    pub selected: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    VolumeChanged(i32),
    ModeChanged(usize),
    ItemSelected(usize),
    Reset,
}
pub struct Ui {
    scene: Scene<10>,
    state: State,
}
impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
impl Ui {
    pub fn new() -> Self {
        let mut scene = Scene::new(rect(0, 0, WIDTH, HEIGHT), rgb(0xe8, 0xf7, 0xff));
        let mut title = Label::new(0, rect(20, 18, 320, 30), "RUST / ESP32-P4");
        title.base.style = Style {
            padding: 0,
            font: FontSize::Large,
            ..style()
        };
        scene.add(title).unwrap();
        let mut subtitle = Label::new(1, rect(20, 56, 320, 20), "Core 1  |  RGB565  |  Touch UI");
        subtitle.base.style = Style {
            padding: 0,
            foreground: rgb(0x26, 0x7b, 0x9a),
            ..style()
        };
        scene.add(subtitle).unwrap();
        let mut card = Panel::new(2, rect(16, 94, 328, 120));
        card.base.style = Style {
            radius: 12,
            ..style()
        };
        scene.add(card).unwrap();
        let mut volume_label = Label::new(VOLUME_LABEL, rect(32, 106, 296, 24), "Volume: 35%");
        volume_label.base.style = Style {
            padding: 0,
            ..style()
        };
        scene.add(volume_label).unwrap();
        let mut slider = Slider::new(VOLUME, rect(32, 150, 296, 48), 0, 100, 1, 35);
        slider.base.style = style();
        scene.add(slider).unwrap();
        let mut mode_label = Label::new(4, rect(20, 230, 88, 28), "Mode");
        mode_label.base.style = Style {
            padding: 0,
            ..style()
        };
        scene.add(mode_label).unwrap();
        let mut mode = ComboBox::new(MODE, rect(110, 220, 230, 48), MODES);
        mode.base.style = style();
        mode.row_height = 40;
        scene.add(mode).unwrap();
        let mut selection = Label::new(
            SELECTION_LABEL,
            rect(20, 282, 320, 22),
            "Swipe the list, then select",
        );
        selection.base.style = Style {
            padding: 0,
            ..style()
        };
        scene.add(selection).unwrap();
        let mut items = ListView::new(ITEMS, rect(16, 314, 328, 252), ROWS);
        items.base.style = Style {
            padding: 16,
            ..style()
        };
        items.row_height = 60;
        items.selected = usize::MAX;
        scene.add(items).unwrap();
        let mut reset = Button::new(RESET, rect(16, 582, 328, 44), "Reset controls");
        reset.base.style = Style {
            align: Alignment::Center,
            ..style()
        };
        scene.add(reset).unwrap();
        scene.focus(VOLUME);
        Self {
            scene,
            state: State {
                volume: 35,
                mode: 0,
                selected: None,
            },
        }
    }
    pub fn dirty(&self) -> Option<Rectangle> {
        self.scene.damage()
    }
    pub fn set_volume(&mut self, value: i32) {
        let value = value.clamp(0, 100);
        if value == self.state.volume {
            return;
        }
        self.state.volume = value;
        self.scene.update(VOLUME, |w| {
            if let Widget::Slider(v) = w {
                v.value = value;
            }
        });
        self.refresh_volume_label();
    }
    fn refresh_volume_label(&mut self) {
        let mut text = InlineText::new("");
        write!(&mut text, "Volume: {}%", self.state.volume).unwrap();
        self.scene.update(VOLUME_LABEL, |w| {
            if let Widget::Label(v) = w {
                v.set_text(text.as_str());
            }
        });
    }
    fn refresh_selection_label(&mut self) {
        let mut text = InlineText::new("");
        if let Some(index) = self.state.selected {
            write!(&mut text, "Selected: Item {}", index + 1).unwrap();
        } else {
            text.write_str("Swipe the list, then select").unwrap();
        }
        self.scene.update(SELECTION_LABEL, |w| {
            if let Widget::Label(v) = w {
                v.set_text(text.as_str());
            }
        });
    }
    /// Match assets/ui/demo.slint: reset volume and selection; preserve mode and scroll.
    pub fn reset(&mut self) {
        self.set_volume(35);
        self.state.selected = None;
        self.scene.update(ITEMS, |w| {
            if let Widget::ListView(v) = w {
                v.selected = usize::MAX;
            }
        });
        self.refresh_selection_label();
    }
    pub fn handle(&mut self, event: Event) -> Option<Action> {
        let action = self.scene.handle(event)?;
        match (action.id, action.kind) {
            (VOLUME, ActionKind::ValueChanged(value)) => {
                self.state.volume = value;
                self.refresh_volume_label();
                Some(Action::VolumeChanged(value))
            }
            (MODE, ActionKind::SelectionChanged(index)) => {
                self.state.mode = index;
                Some(Action::ModeChanged(index))
            }
            (ITEMS, ActionKind::SelectionChanged(index)) => {
                self.state.selected = Some(index);
                self.refresh_selection_label();
                Some(Action::ItemSelected(index))
            }
            (RESET, ActionKind::Clicked) => {
                self.reset();
                Some(Action::Reset)
            }
            _ => None,
        }
    }
    pub fn render<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
    ) -> Result<Option<Rectangle>, D::Error> {
        self.render_with_extra_damage(target, None)
    }
    pub fn render_full<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        self.scene.render_full(target)
    }
    pub fn render_with_extra_damage<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        extra: Option<Rectangle>,
    ) -> Result<Option<Rectangle>, D::Error> {
        self.scene.render_with_extra_damage(target, extra)
    }
}
