use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
#[path = "../src/bin/demos/rust_ui_demo/page.rs"]
mod page;
use page::{Action, Event, HEIGHT, ITEMS, MODE, RESET, Ui, VOLUME, WIDTH};
use tiny_ui::Key;
#[derive(Clone)]
struct Frame(Vec<Rgb565>);
impl Frame {
    fn new() -> Self {
        Self(vec![Rgb565::BLACK; (WIDTH * HEIGHT) as usize])
    }
}
impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
impl DrawTarget for Frame {
    type Color = Rgb565;
    type Error = core::convert::Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        iter: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in iter {
            if self.bounding_box().contains(p) {
                self.0[p.y as usize * WIDTH as usize + p.x as usize] = c;
            }
        }
        Ok(())
    }
}
fn click(ui: &mut Ui, x: i32, y: i32) -> Option<Action> {
    ui.handle(Event::Down(Point::new(x, y)));
    ui.handle(Event::Up(Point::new(x, y)))
}
#[test]
fn initial_page_matches_slint_demo_state() {
    let ui = Ui::new();
    assert_eq!((WIDTH, HEIGHT), (360, 640));
    assert_eq!(ui.state().volume, 35);
    assert_eq!(ui.state().mode, 0);
    assert_eq!(ui.state().selected, None);
    assert_eq!(ui.focus(), Some(VOLUME));
}
#[test]
fn slider_capture_clamps_and_cancel_ends_drag() {
    let mut ui = Ui::new();
    assert_eq!(
        ui.handle(Event::Down(Point::new(32, 170))),
        Some(Action::VolumeChanged(0))
    );
    assert_eq!(
        ui.handle(Event::Move(Point::new(999, -40))),
        Some(Action::VolumeChanged(100))
    );
    ui.handle(Event::Cancel);
    assert_eq!(ui.handle(Event::Move(Point::zero())), None);
    assert_eq!(ui.state().volume, 100);
    assert_eq!(ui.handle(Event::Up(Point::zero())), None);
}
#[test]
fn button_click_outside_cancel_and_reset_policy() {
    let mut ui = Ui::new();
    ui.set_volume(80);
    assert_eq!(click(&mut ui, 160, 600), Some(Action::Reset));
    assert_eq!(ui.state().volume, 35);
    assert_eq!(ui.handle(Event::Up(Point::new(160, 600))), None);
    ui.handle(Event::Down(Point::new(160, 600)));
    ui.handle(Event::Move(Point::zero()));
    assert_eq!(ui.handle(Event::Up(Point::zero())), None);
    ui.handle(Event::Down(Point::new(160, 600)));
    ui.handle(Event::Cancel);
    assert_eq!(ui.handle(Event::Up(Point::new(160, 600))), None);
}
#[test]
fn popup_owns_overlap_and_outside_dismiss_does_not_click_list() {
    let mut ui = Ui::new();
    click(&mut ui, 200, 240);
    assert_eq!(click(&mut ui, 200, 368), Some(Action::ModeChanged(2))); // Music, over list geometry.
    assert_eq!(ui.state().selected, None);
    click(&mut ui, 200, 240);
    assert_eq!(click(&mut ui, 50, 500), None);
    assert_eq!(ui.state().selected, None);
    assert_eq!(click(&mut ui, 50, 500), Some(Action::ItemSelected(3)));
}
#[test]
fn scrolling_cancels_click_and_reset_preserves_mode_and_offset() {
    let mut ui = Ui::new();
    click(&mut ui, 200, 240);
    click(&mut ui, 200, 328); // Quiet.
    ui.handle(Event::Down(Point::new(80, 500)));
    ui.handle(Event::Move(Point::new(80, 350)));
    assert_eq!(ui.handle(Event::Up(Point::new(80, 350))), None);
    assert_eq!(ui.state().selected, None);
    assert_eq!(ui.scroll_offset(), 150);
    assert_eq!(click(&mut ui, 80, 330), Some(Action::ItemSelected(2)));
    click(&mut ui, 160, 600);
    assert_eq!(ui.state().selected, None);
    assert_eq!(ui.state().mode, 1);
    assert_eq!(ui.scroll_offset(), 150);
}
#[test]
fn keyboard_focus_cancel_and_activation() {
    let mut ui = Ui::new();
    assert_eq!(
        ui.handle(Event::KeyDown(Key::Right)),
        Some(Action::VolumeChanged(36))
    );
    for id in [MODE, ITEMS, RESET] {
        ui.handle(Event::KeyDown(Key::Next));
        assert_eq!(ui.focus(), Some(id));
    }
    ui.handle(Event::KeyDown(Key::Activate));
    ui.handle(Event::KeyDown(Key::Activate));
    assert_eq!(ui.handle(Event::KeyUp(Key::Activate)), Some(Action::Reset));
    assert_eq!(ui.handle(Event::KeyUp(Key::Activate)), None);
    ui.handle(Event::KeyDown(Key::Activate));
    ui.handle(Event::Cancel);
    assert_eq!(ui.handle(Event::KeyUp(Key::Activate)), None);
    ui.handle(Event::KeyDown(Key::Activate));
    ui.handle(Event::KeyDown(Key::Previous));
    assert_eq!(ui.handle(Event::KeyUp(Key::Activate)), None);
}
fn sequence() -> [Event; 24] {
    [
        Event::Down(Point::new(32, 170)),
        Event::Move(Point::new(327, 170)),
        Event::Up(Point::new(200, 170)),
        Event::Down(Point::new(200, 240)),
        Event::Up(Point::new(200, 240)),
        Event::Down(Point::new(200, 368)),
        Event::Up(Point::new(200, 368)),
        Event::Down(Point::new(80, 500)),
        Event::Move(Point::new(80, 350)),
        Event::Up(Point::new(80, 350)),
        Event::Down(Point::new(80, 330)),
        Event::Up(Point::new(80, 330)),
        Event::Down(Point::new(160, 600)),
        Event::Move(Point::zero()),
        Event::Move(Point::new(160, 600)),
        Event::Cancel,
        Event::KeyDown(Key::Activate),
        Event::KeyUp(Key::Activate),
        Event::KeyDown(Key::Next),
        Event::KeyDown(Key::Left),
        Event::Down(Point::new(200, 240)),
        Event::Up(Point::new(200, 240)),
        Event::KeyDown(Key::Escape),
        Event::Cancel,
    ]
}
#[test]
fn partial_matches_full_for_every_control_and_overlay_transition() {
    let mut ui = Ui::new();
    let mut fb = Frame::new();
    ui.render(&mut fb).unwrap();
    for event in sequence() {
        ui.handle(event);
        ui.render(&mut fb).unwrap();
        let mut full = Frame::new();
        ui.render_full(&mut full).unwrap();
        assert!(fb.0 == full.0, "event {event:?}");
    }
}
#[test]
fn every_volume_value_partial_matches_full_and_unchanged_value_is_clean() {
    let mut ui = Ui::new();
    let mut fb = Frame::new();
    ui.render(&mut fb).unwrap();
    ui.set_volume(35);
    assert_eq!(ui.dirty(), None);
    for v in 0..=100 {
        ui.set_volume(v);
        ui.render(&mut fb).unwrap();
        let mut full = Frame::new();
        ui.render_full(&mut full).unwrap();
        assert!(fb.0 == full.0, "volume {v}");
    }
}
#[test]
fn alternating_buffers_repair_all_control_and_popup_history() {
    let mut ui = Ui::new();
    let mut buffers = [Frame::new(), Frame::new()];
    ui.render(&mut buffers[0]).unwrap();
    ui.render_full(&mut buffers[1]).unwrap();
    let mut prev = None;
    let mut i = 0;
    for event in sequence() {
        ui.handle(event);
        let logical = ui.dirty();
        if logical.is_some() {
            ui.render_with_extra_damage(&mut buffers[i % 2], prev)
                .unwrap();
            prev = logical;
            let mut full = Frame::new();
            ui.render_full(&mut full).unwrap();
            assert!(buffers[i % 2].0 == full.0, "frame {i}, {event:?}");
            i += 1;
        }
    }
}
#[test]
fn full_reference_does_not_consume_dirty_and_invalid_extra_is_ignored() {
    let mut ui = Ui::new();
    let mut fb = Frame::new();
    let d = ui.dirty();
    ui.render_full(&mut fb).unwrap();
    assert_eq!(ui.dirty(), d);
    ui.render(&mut fb).unwrap();
    assert_eq!(
        ui.render_with_extra_damage(&mut fb, Some(Rectangle::new(Point::zero(), Size::zero())))
            .unwrap(),
        None
    );
    assert_eq!(
        ui.render_with_extra_damage(
            &mut fb,
            Some(Rectangle::new(Point::new(1000, 1000), Size::new(5, 5)))
        )
        .unwrap(),
        None
    );
}
#[test]
fn failed_render_preserves_logical_and_history_damage() {
    struct Fail;
    impl OriginDimensions for Fail {
        fn size(&self) -> Size {
            Size::new(WIDTH, HEIGHT)
        }
    }
    impl DrawTarget for Fail {
        type Color = Rgb565;
        type Error = ();
        fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, _: I) -> Result<(), ()> {
            Err(())
        }
    }
    let mut ui = Ui::new();
    assert!(ui.render(&mut Fail).is_err());
    assert!(ui.dirty().is_some());
}
