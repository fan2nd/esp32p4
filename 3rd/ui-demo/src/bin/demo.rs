use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use std::{
    convert::Infallible,
    io::{Read, Write},
    net::TcpListener,
};
use tiny_ui_demo::{Event, Key, Ui, HEIGHT, WIDTH};
struct Frame {
    pixels: Vec<Rgb565>,
}
impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
impl DrawTarget for Frame {
    type Color = Rgb565;
    type Error = Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 && p.x < WIDTH as i32 && p.y < HEIGHT as i32 {
                self.pixels[p.y as usize * WIDTH as usize + p.x as usize] = c;
            }
        }
        Ok(())
    }
}
fn rgb(c: Rgb565) -> [u8; 3] {
    [
        (u16::from(c.r()) * 255 / 31) as u8,
        (u16::from(c.g()) * 255 / 63) as u8,
        (u16::from(c.b()) * 255 / 31) as u8,
    ]
}
fn main() -> std::io::Result<()> {
    let mut ui = Ui::new();
    let mut frame = Frame {
        pixels: vec![Rgb565::BLACK; (WIDTH * HEIGHT) as usize],
    };
    ui.render(&mut frame).unwrap();
    if let Some(path) = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .strip_prefix(&["--snapshot".to_owned()])
        .and_then(|s| s.first())
    {
        let mut f = std::fs::File::create(path)?;
        write!(f, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
        for c in &frame.pixels {
            f.write_all(&rgb(*c))?;
        }
        println!("Wrote {path}");
        return Ok(());
    }
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Open http://127.0.0.1:7878 (Ctrl-C to stop)");
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
        let mut request = [0u8; 4096];
        let n = match stream.read(&mut request) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&request[..n]);
        let path = request.split_whitespace().nth(1).unwrap_or("/");
        let (mime, body): (&str, Vec<u8>) = if path == "/" {
            (
                "text/html; charset=utf-8",
                include_bytes!("../../assets/demo.html").to_vec(),
            )
        } else if path == "/state" {
            let state = ui.state();
            (
                "application/json",
                format!(
                    "{{\"volume\":{},\"mode\":{},\"selected\":{},\"scroll\":{}}}",
                    state.volume,
                    state.mode,
                    state
                        .selected
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "null".into()),
                    ui.scroll_offset(),
                )
                .into_bytes(),
            )
        } else if path.starts_with("/event?") || path == "/frame" {
            if let Some(query) = path.strip_prefix("/event?") {
                let get = |name: &str| {
                    query
                        .split('&')
                        .filter_map(|p| p.split_once('='))
                        .find(|(k, _)| *k == name)
                        .map(|(_, v)| v)
                        .unwrap_or("")
                };
                let x = get("x").parse().unwrap_or(0);
                let y = get("y").parse().unwrap_or(0);
                let point = Point::new(x, y);
                let key = match get("key") {
                    "left" => Key::Left,
                    "right" => Key::Right,
                    "up" => Key::Up,
                    "down" => Key::Down,
                    "escape" => Key::Escape,
                    "previous" => Key::Previous,
                    "activate" => Key::Activate,
                    _ => Key::Next,
                };
                let event = match get("kind") {
                    "down" => Some(Event::Down(point)),
                    "move" => Some(Event::Move(point)),
                    "up" => Some(Event::Up(point)),
                    "cancel" => Some(Event::Cancel),
                    "keydown" => Some(Event::KeyDown(key)),
                    "keyup" => Some(Event::KeyUp(key)),
                    _ => None,
                };
                if let Some(event) = event {
                    if let Some(action) = ui.handle(event) {
                        println!("{action:?}");
                    }
                }
            }
            ui.render(&mut frame).unwrap();
            let bytes = frame
                .pixels
                .iter()
                .flat_map(|c| {
                    let [r, g, b] = rgb(*c);
                    [r, g, b, 255]
                })
                .collect();
            ("application/octet-stream", bytes)
        } else {
            ("text/plain", b"Not found".to_vec())
        };
        let header = format!("HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
        let _ = stream
            .write_all(header.as_bytes())
            .and_then(|_| stream.write_all(&body));
    }
    Ok(())
}
