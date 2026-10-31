use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
#[path = "../src/tiny_desktop_page.rs"]
mod page;
#[path = "../src/password_keyboard.rs"]
mod password_keyboard;
use page::{Action, Network, Ui};
use tiny_ui::Event;
#[derive(Clone)]
struct Frame(Vec<Rgb565>);
impl Frame {
    fn new() -> Self {
        Self(vec![Rgb565::BLACK; 360 * 640])
    }
}
impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(360, 640)
    }
}
impl DrawTarget for Frame {
    type Color = Rgb565;
    type Error = core::convert::Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        it: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in it {
            if (0..360).contains(&p.x) && (0..640).contains(&p.y) {
                self.0[p.y as usize * 360 + p.x as usize] = c
            }
        }
        Ok(())
    }
}
fn tap(ui: &mut Ui, x: i32, y: i32) -> Option<Action> {
    ui.handle(Event::Down(Point::new(x, y)));
    ui.handle(Event::Up(Point::new(x, y)))
}
fn network(name: &str) -> Network {
    Network {
        ssid: heapless::String::try_from(name).unwrap(),
        rssi: -45,
        secure: true,
    }
}
fn correct(ui: &mut Ui, frame: &mut Frame) {
    ui.render_with_extra_damage(frame, None).unwrap();
    let mut expected = Frame::new();
    ui.render_full(&mut expected).unwrap();
    assert_eq!(frame.0, expected.0);
}
#[test]
fn navigation_password_busy_accept() {
    let mut u = Ui::new();
    assert!(matches!(tap(&mut u, 320, 25), Some(Action::Scan)));
    assert_eq!(u.page(), 1);
    u.sync_networks(&[network("test")]);
    tap(&mut u, 30, 175);
    assert_eq!(u.page(), 2);
    tap(&mut u, 55, 260);
    let c = tap(&mut u, 160, 560).unwrap();
    match c {
        Action::Connect { ssid, password } => {
            assert_eq!(ssid.as_str(), "test");
            assert!(!password.is_empty());
        }
        _ => panic!(),
    };
    u.connect_result(false);
    assert_eq!(u.page(), 2);
    assert!(matches!(
        tap(&mut u, 160, 560),
        Some(Action::Connect { .. })
    ));
    u.connect_result(true);
    assert_eq!(u.page(), 1);
    tap(&mut u, 30, 175);
    let Some(Action::Connect { password, .. }) = tap(&mut u, 160, 560) else {
        panic!()
    };
    assert!(password.is_empty());
}
#[test]
fn drag_not_click_and_refresh_retains_scroll() {
    let mut u = Ui::new();
    u.navigate(1);
    let nets: Vec<_> = (0..20).map(|i| network(&format!("ap{i}"))).collect();
    u.sync_networks(&nets);
    u.handle(Event::Down(Point::new(80, 480)));
    u.handle(Event::Move(Point::new(80, 200)));
    assert!(u.handle(Event::Up(Point::new(80, 200))).is_none());
    assert_eq!(u.page(), 1);
    let mut before = Frame::new();
    u.render_full(&mut before).unwrap();
    u.sync_networks(&nets);
    let mut after = Frame::new();
    u.render_full(&mut after).unwrap();
    assert_eq!(before.0, after.0);
    tap(&mut u, 40, 175);
    let Some(Action::Connect { ssid, .. }) = tap(&mut u, 160, 560) else {
        panic!()
    };
    assert_ne!(ssid.as_str(), "ap0");
}
#[test]
fn macro_geometry_and_status() {
    let mut u = Ui::new();
    assert_eq!(u.macro_key_at(Point::new(20, 150)), None);
    tap(&mut u, 160, 580);
    assert_eq!(u.page(), 3);
    assert_eq!(u.macro_key_at(Point::new(20, 150)), Some(0));
    assert_eq!(u.macro_key_at(Point::new(200, 480)), Some(7));
    assert_eq!(u.macro_key_at(Point::new(175, 150)), None);
    assert_eq!(u.macro_key_at(Point::new(20, 239)), None);
    u.sync_macros(&["A", "B", "C", "D", "E", "F", "G", "H"], 3);
    assert!(u.dirty().is_some());
}
#[test]
fn dirty_matches_full_all_pages() {
    let mut u = Ui::new();
    let mut frame = Frame::new();
    correct(&mut u, &mut frame);
    u.sync_status("12:34:56", true, true, true, "IP  192.0.2.1", "Ready");
    correct(&mut u, &mut frame);
    u.navigate(1);
    u.sync_networks(&[network("中文 WiFi"), network("other")]);
    correct(&mut u, &mut frame);
    tap(&mut u, 30, 175);
    correct(&mut u, &mut frame);
    tap(&mut u, 55, 260);
    correct(&mut u, &mut frame);
    u.connect_result(false);
    correct(&mut u, &mut frame);
    u.navigate(3);
    u.sync_macros(&["A", "B", "C", "D", "E", "F", "G", "H"], 1);
    correct(&mut u, &mut frame);
    u.sync_macros(&["X", "B", "C", "D", "E", "F", "G", "H"], 2);
    correct(&mut u, &mut frame);
}
#[test]
fn alternating_buffers_restore_history() {
    let mut u = Ui::new();
    let mut buffers = [Frame::new(), Frame::new()];
    for b in &mut buffers {
        u.render_full(b).unwrap();
    }
    let mut previous = None;
    for n in 0..8 {
        u.sync_status(
            &format!("12:34:{n:02}"),
            n % 2 == 0,
            false,
            true,
            "IP  192.0.2.1",
            "",
        );
        let current = u.dirty();
        u.render_with_extra_damage(&mut buffers[n % 2], previous)
            .unwrap();
        previous = current;
        let mut expected = Frame::new();
        u.render_full(&mut expected).unwrap();
        assert_eq!(buffers[n % 2].0, expected.0);
    }
}
#[test]
fn cancel_does_not_navigate() {
    let mut u = Ui::new();
    u.handle(Event::Down(Point::new(320, 25)));
    u.handle(Event::Cancel);
    assert!(u.handle(Event::Up(Point::new(320, 25))).is_none());
    assert_eq!(u.page(), 0);
}
#[test]
fn render_snapshots() {
    if let Ok(dir) = std::env::var("TINY_DESKTOP_SNAPSHOTS") {
        std::fs::create_dir_all(&dir).unwrap();
        let mut u = Ui::new();
        u.sync_status(
            "12:34:56",
            true,
            true,
            true,
            "IP  192.0.2.1",
            "Scan complete",
        );
        u.sync_networks(&[
            network("OpenAI WiFi"),
            network("中文网络"),
            network("Lab Guest"),
        ]);
        for (p, name) in [
            (0, "home"),
            (1, "settings"),
            (2, "password"),
            (3, "keyboard"),
        ] {
            u.navigate(if p == 2 { 1 } else { p });
            if p == 2 {
                tap(&mut u, 30, 175);
            }
            u.sync_macros(
                &["A", "B", "VOL+", "VOL-", "PLAY", "NEXT", "COPY", "PASTE"],
                1,
            );
            let mut f = Frame::new();
            u.render_full(&mut f).unwrap();
            let mut bytes = b"P6\n360 640\n255\n".to_vec();
            for c in f.0 {
                let c = embedded_graphics::pixelcolor::Rgb888::from(c);
                bytes.extend([c.r(), c.g(), c.b()]);
            }
            std::fs::write(format!("{dir}/{name}.ppm"), bytes).unwrap();
        }
    }
}

#[test]
fn long_text_damage_and_keyboard_shift_caps() {
    let mut u = Ui::new();
    let mut f = Frame::new();
    correct(&mut u, &mut f);
    u.navigate(1);
    u.sync_networks(&[network("12345678901234567890123456789012")]);
    u.sync_status(
        "23:59:59",
        true,
        true,
        true,
        "IP  255.255.255.255",
        &"scan status ".repeat(10),
    );
    correct(&mut u, &mut f);
    u.sync_status("00:00:00", false, false, false, "", "");
    correct(&mut u, &mut f);
    tap(&mut u, 30, 175);
    correct(&mut u, &mut f);
    for k in password_keyboard::layout() {
        let x = (8. + k.x * 23.7 + k.width * 23.7 / 2.) as i32;
        let y = 238 + k.row as i32 * 54 + 24;
        tap(&mut u, x, y);
        correct(&mut u, &mut f);
    }
    println!("Ui fixed size={} bytes", core::mem::size_of::<Ui>());
    assert!(core::mem::size_of::<Ui>() <= 24 * 1024);
}

#[test]
fn changing_scan_cancels_old_pointer_selection() {
    let mut u = Ui::new();
    u.navigate(1);
    u.sync_networks(&[network("old")]);
    u.handle(Event::Down(Point::new(30, 175)));
    u.sync_networks(&[network("replacement")]);
    assert!(u.handle(Event::Up(Point::new(30, 175))).is_none());
    assert_eq!(u.page(), 1);
}

#[test]
fn unrelated_status_does_not_reset_scrolled_networks() {
    let mut u = Ui::new();
    u.navigate(1);
    let nets: Vec<_> = (0..20).map(|i| network(&format!("ap{i}"))).collect();
    u.sync_networks(&nets);
    u.handle(Event::Down(Point::new(80, 480)));
    u.handle(Event::Move(Point::new(80, 200)));
    u.handle(Event::Up(Point::new(80, 200)));
    let mut before = Frame::new();
    u.render_full(&mut before).unwrap();
    u.sync_status("12:45:56", true, true, true, "IP 192.0.2.2", "");
    let mut after = Frame::new();
    u.render_full(&mut after).unwrap();
    assert_eq!(before.0, after.0);
}
#[test]
fn overlay_failure_reinvalidates_damage() {
    struct FailOverlay {
        n: usize,
    }
    impl OriginDimensions for FailOverlay {
        fn size(&self) -> Size {
            Size::new(360, 640)
        }
    }
    impl DrawTarget for FailOverlay {
        type Color = Rgb565;
        type Error = ();
        fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, iter: I) -> Result<(), ()> {
            for _ in iter {
                self.n += 1;
                if self.n > 240000 {
                    return Err(());
                }
            }
            Ok(())
        }
    }
    let mut u = Ui::new();
    assert!(
        u.render_with_extra_damage(&mut FailOverlay { n: 0 }, None)
            .is_err()
    );
    assert!(u.dirty().is_some());
    let mut f = Frame::new();
    correct(&mut u, &mut f);
    assert!(u.dirty().is_none());
}

#[test]
fn long_clock_is_clipped_to_damage() {
    let mut u = Ui::new();
    let mut f = Frame::new();
    correct(&mut u, &mut f);
    u.sync_status("1234567890123456", false, false, false, "", "");
    correct(&mut u, &mut f);
    u.sync_status("--:--:--", false, false, false, "", "");
    correct(&mut u, &mut f);
}
