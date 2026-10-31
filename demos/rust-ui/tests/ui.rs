use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use tiny_ui_demo::{Action, Event, Focus, Key, Ui, HEIGHT, WIDTH};
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
            if p.x >= 0 && p.y >= 0 && p.x < WIDTH as i32 && p.y < HEIGHT as i32 {
                self.0[p.y as usize * WIDTH as usize + p.x as usize] = c;
            }
        }
        Ok(())
    }
}
#[test]
fn slider_capture_clamps_and_cancel_ends_drag() {
    let mut ui = Ui::new();
    assert_eq!(
        ui.handle(Event::Down(Point::new(40, 100))),
        Some(Action::ValueChanged(0))
    );
    assert_eq!(
        ui.handle(Event::Move(Point::new(999, -40))),
        Some(Action::ValueChanged(100))
    );
    ui.handle(Event::Cancel);
    assert_eq!(ui.handle(Event::Move(Point::new(0, 0))), None);
    assert_eq!(ui.value(), 100);
    assert_eq!(ui.handle(Event::Up(Point::new(0, 0))), None);
}
#[test]
fn button_click_outside_and_cancel() {
    let mut ui = Ui::new();
    let p = Point::new(160, 180);
    ui.handle(Event::Down(p));
    assert_eq!(ui.handle(Event::Up(p)), Some(Action::Clicked));
    assert_eq!(ui.handle(Event::Up(p)), None);
    ui.handle(Event::Down(p));
    ui.handle(Event::Move(Point::zero()));
    assert_eq!(ui.handle(Event::Up(Point::zero())), None);
    ui.handle(Event::Down(p));
    ui.handle(Event::Cancel);
    assert_eq!(ui.handle(Event::Up(p)), None);
}
#[test]
fn keyboard_focus_and_repeated_activation() {
    let mut ui = Ui::new();
    assert_eq!(
        ui.handle(Event::KeyDown(Key::Right)),
        Some(Action::ValueChanged(51))
    );
    ui.handle(Event::KeyDown(Key::Next));
    assert_eq!(ui.focus(), Focus::Button);
    ui.handle(Event::KeyDown(Key::Activate));
    ui.handle(Event::KeyDown(Key::Activate));
    assert_eq!(
        ui.handle(Event::KeyUp(Key::Activate)),
        Some(Action::Clicked)
    );
    assert_eq!(ui.handle(Event::KeyUp(Key::Activate)), None);
    ui.handle(Event::KeyDown(Key::Activate));
    ui.handle(Event::KeyDown(Key::Previous));
    assert_eq!(ui.handle(Event::KeyUp(Key::Activate)), None);
}
#[test]
fn unchanged_value_has_no_damage() {
    let mut ui = Ui::new();
    let mut fb = Frame::new();
    assert!(ui.render(&mut fb).unwrap().is_some());
    ui.set_value(50);
    assert_eq!(ui.render(&mut fb).unwrap(), None);
    ui.set_value(255);
    assert_eq!(ui.value(), 100);
}
#[test]
fn partial_render_matches_full_after_every_event() {
    let mut ui = Ui::new();
    let mut fb = Frame::new();
    ui.render(&mut fb).unwrap();
    let events = [
        Event::Down(Point::new(40, 100)),
        Event::Move(Point::new(280, 100)),
        Event::Move(Point::new(130, 100)),
        Event::Up(Point::new(130, 100)),
        Event::Down(Point::new(160, 180)),
        Event::Move(Point::zero()),
        Event::Move(Point::new(160, 180)),
        Event::Cancel,
        Event::KeyDown(Key::Activate),
        Event::KeyUp(Key::Activate),
        Event::KeyDown(Key::Next),
        Event::KeyDown(Key::Left),
    ];
    for event in events {
        if ui.handle(event) == Some(Action::Clicked) {
            ui.set_value(50);
        }
        ui.render(&mut fb).unwrap();
        let mut expected = Frame::new();
        ui.render_full(&mut expected).unwrap();
        assert!(fb.0 == expected.0, "event {event:?}");
    }
}
#[test]
fn partial_render_matches_full_for_all_values() {
    let mut ui = Ui::new();
    let mut fb = Frame::new();
    ui.render(&mut fb).unwrap();
    for value in 0..=100 {
        ui.set_value(value);
        ui.render(&mut fb).unwrap();
        let mut expected = Frame::new();
        ui.render_full(&mut expected).unwrap();
        assert!(fb.0 == expected.0, "value {value}");
    }
}
#[test]
fn failed_display_keeps_damage() {
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

#[test]
fn alternating_buffers_restore_previous_damage() {
    let mut ui = Ui::new();
    let mut buffers = [Frame::new(), Frame::new()];
    let mut previous = None;
    let events = [
        Event::Cancel,
        Event::KeyDown(Key::Right),
        Event::KeyDown(Key::Next),
        Event::KeyDown(Key::Activate),
        Event::KeyUp(Key::Activate),
        Event::KeyDown(Key::Next),
        Event::KeyDown(Key::Left),
    ];
    for (index, event) in events.into_iter().enumerate() {
        if ui.handle(event) == Some(Action::Clicked) {
            ui.set_value(50);
        }
        let logical = ui.dirty();
        ui.render_with_extra_damage(&mut buffers[index % 2], previous)
            .unwrap();
        previous = logical;
        let mut expected = Frame::new();
        ui.render_full(&mut expected).unwrap();
        assert!(buffers[index % 2].0 == expected.0, "frame {index}");
    }
}

#[test]
fn empty_and_offscreen_extra_damage_are_ignored() {
    use embedded_graphics::primitives::Rectangle;
    let mut ui = Ui::new();
    let mut frame = Frame::new();
    ui.render_with_extra_damage(
        &mut frame,
        Some(Rectangle::new(Point::zero(), Size::zero())),
    )
    .unwrap();
    assert_eq!(
        ui.render_with_extra_damage(
            &mut frame,
            Some(Rectangle::new(Point::new(400, 300), Size::new(10, 10)))
        )
        .unwrap(),
        None
    );
}
