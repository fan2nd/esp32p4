# tiny-ui

A reusable `#![no_std]`, allocation-free retained UI over `embedded-graphics`
`DrawTarget<Color = Rgb565>`. It has no board, display-resolution or demo dependency.

- `Scene<N>` owns up to N independently configured `Widget`s; duplicate IDs and
  capacity exhaustion return errors.
- `Button`, `Slider`, `ComboBox`, `ListView`, `Label`, `Panel` have public `Base`
  geometry, visibility, enablement and palette. Styles include radius, padding,
  18/20 pixel fonts and alternating row color. Item strings are static borrowed
  slices; labels use 96-byte UTF-8 inline storage and support `core::fmt::Write`.
- Pointer capture lasts until release/cancel. Buttons click on release; slider
  ranges/steps use widened arithmetic; list dragging cancels taps and clamps
  scrolling. Popups capture input, dismiss outside, and render above all widgets.
- Keyboard previous/next moves focus; arrows adjust/select; activate operates
  buttons/combos and Escape/Cancel clears activation. Each scene owns its state.
- Change properties through `scene.update(id, |widget| ...)` to invalidate both
  old/new extents. Identity is preserved by update. `usize::MAX` means no list
  selection. Public values can be assigned directly; rendering clamps scroll and
  slider positions. Use constructors to normalize slider values to their step.
- `render` returns actual clipped damage and clears it only after success.
  `render_full(&self, ...)` draws a full reference frame without consuming damage.
  `render_with_extra_damage` accepts previous damage to repair alternating frame
  buffers. Save *current scene damage* before rendering for next frame; passing
  the already merged return damage repeatedly will unnecessarily accumulate it.

Host tests: `cargo test --target x86_64-unknown-linux-gnu`. The enclosing firmware
sets an embedded target, so the explicit host target is intentional. Tests cover
independent instances, capacity, capture/cancel, overflow-safe ranges, popup/list
interaction, keyboard focus, UTF-8, localized damage, A/B framebuffer equivalence,
and preservation of dirty state after draw errors. Physical input/display timing
requires hardware validation in the integrating application.

## Minimal composition (no page-specific API)

```rust
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use tiny_ui::{ActionKind, Button, Event, Scene, Slider};

let mut scene = Scene::<4>::new(
    Rectangle::new(Point::zero(), Size::new(240, 160)), Rgb565::BLACK,
);
scene.add(Slider::new(7, Rectangle::new(Point::new(10, 20), Size::new(200, 30)),
                      -20, 80, 5, 10)).unwrap();
scene.add(Button::new(9, Rectangle::new(Point::new(60, 90), Size::new(120, 40)),
                      "Apply")).unwrap();
// The input driver supplies events. It does not know about the application.
scene.handle(Event::Down(Point::new(100, 110)));
if let Some(action) = scene.handle(Event::Up(Point::new(100, 110))) {
    if action.id == 9 && action.kind == ActionKind::Clicked {
        // Enqueue an application command here; never wait for Wi-Fi in a widget.
    }
}
// scene.render(&mut display)?; // Caller provides an RGB565 DrawTarget.
```

An unmatched Up never clicks: a matching Down/capture (or keyboard activation)
is required. Forward the entire input sequence in the real driver.
Use `scene.update(id, |widget| ...)` when changing components already in a scene.
A scene can contain multiple sliders/buttons with independent geometry/state,
not one hardcoded instance of each type. See tests for two-scene independence.

This first version uses a flat retained scene, explicit geometry, static item
arrays, ASCII bitmap fonts, and built-in widget variants. It is intentionally
not a Slint parser, automatic layout system, general custom-widget trait, or
animated/inertial scroller. List clipping is rectangular; popup size/placement
is configured by the caller and clipped to the scene viewport.
