//! Bounded, allocation-free Open-Meteo current weather client on Core 0.
use crate::desktop;
use core::fmt::Write as _;
use embassy_net::{Stack, dns::DnsQueryType, tcp::TcpSocket};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::Write;
use heapless::String;

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Location {
    pub name: String<48>,
    /// Fixed-point coordinates avoid float equality/NaN in persisted settings.
    pub latitude: i32,
    pub longitude: i32,
}
impl Location {
    pub fn valid(&self) -> bool {
        !self.name.is_empty()
            && !self.name.chars().any(char::is_control)
            && (-90_000_000..=90_000_000).contains(&self.latitude)
            && (-180_000_000..=180_000_000).contains(&self.longitude)
    }
}
#[derive(serde::Deserialize)]
struct Response {
    current: Current,
}
#[derive(serde::Deserialize)]
struct Current {
    temperature_2m: f32,
    weather_code: u16,
}

pub async fn run(stack: Stack<'static>) {
    let mut previous = None;
    let mut next = Instant::now();
    let mut was_online = false;
    loop {
        let location = desktop::with(|s| s.settings.weather.clone());
        let online = stack.is_config_up() && desktop::with(|s| s.settings.wifi_enabled);
        if location != previous || (online && !was_online) {
            next = Instant::now();
            previous = location.clone();
        }
        was_online = online;
        if let Some(location) = location {
            if online && Instant::now() >= next {
                let result = with_timeout(Duration::from_secs(15), fetch(stack, &location)).await;
                let mut line = String::<64>::new();
                let ok = if let Ok(Some(current)) = result {
                    let _ = write!(
                        line,
                        "{:.1} °C  {}",
                        current.temperature_2m,
                        description(current.weather_code)
                    );
                    true
                } else {
                    let _ = line.push_str("Weather unavailable; retrying");
                    false
                };
                // A slow response for an old city must not overwrite a newly selected city.
                desktop::with(|s| {
                    if s.settings.weather.as_ref() == Some(&location) {
                        s.view.weather = line;
                        s.view.revision = s.view.revision.wrapping_add(1);
                    }
                });
                log::info!("weather: refresh ok={}", ok);
                next = Instant::now() + Duration::from_secs(if ok { 900 } else { 60 });
            } else if !online {
                desktop::with(|s| {
                    if s.view.weather != "Weather: Wi-Fi offline" {
                        s.view.weather = String::try_from("Weather: Wi-Fi offline").unwrap();
                        s.view.revision = s.view.revision.wrapping_add(1);
                    }
                });
            }
        }
        Timer::after_secs(1).await;
    }
}

async fn fetch(stack: Stack<'static>, location: &Location) -> Option<Current> {
    let addresses = stack
        .dns_query("api.open-meteo.com", DnsQueryType::A)
        .await
        .ok()?;
    let mut rx = [0; 1024];
    let mut tx = [0; 512];
    let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
    socket.set_timeout(Some(Duration::from_secs(10)));
    socket.connect((*addresses.first()?, 80)).await.ok()?;
    let mut request = String::<320>::new();
    // HTTP/1.0 asks for a close-delimited, uncompressed body. Never interpret
    // an unsupported chunked/compressed response as JSON or accept redirects.
    write!(request, "GET /v1/forecast?latitude={:.6}&longitude={:.6}&current=temperature_2m,weather_code HTTP/1.0\r\nHost: api.open-meteo.com\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        location.latitude as f64 / 1_000_000.0, location.longitude as f64 / 1_000_000.0).ok()?;
    socket.write_all(request.as_bytes()).await.ok()?;
    let mut buffer = [0; 4096];
    let mut used = 0;
    loop {
        if used == buffer.len() {
            return None;
        }
        let n = socket.read(&mut buffer[used..]).await.ok()?;
        if n == 0 {
            break;
        }
        used += n;
        embassy_futures::yield_now().await;
    }
    let end = buffer[..used].windows(4).position(|s| s == b"\r\n\r\n")? + 4;
    let header = core::str::from_utf8(&buffer[..end]).ok()?;
    if header.lines().next()?.split_whitespace().nth(1)? != "200" {
        return None;
    }
    for line in header.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Transfer-Encoding")
                || name.eq_ignore_ascii_case("Content-Encoding")
            {
                return None;
            }
            if name.eq_ignore_ascii_case("Content-Length")
                && value.trim().parse::<usize>().ok()? != used - end
            {
                return None;
            }
        }
    }
    let (response, consumed) = serde_json_core::from_slice::<Response>(&buffer[end..used]).ok()?;
    let value = response.current;
    if !buffer[end + consumed..used]
        .iter()
        .all(u8::is_ascii_whitespace)
        || !value.temperature_2m.is_finite()
        || !(-100.0..=70.0).contains(&value.temperature_2m)
    {
        return None;
    }
    Some(value)
}

fn description(code: u16) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 => "Rain",
        71..=77 => "Snow",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95..=99 => "Thunderstorm",
        _ => "Unknown",
    }
}
