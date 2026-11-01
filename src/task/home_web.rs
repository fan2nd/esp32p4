//! Local-network homepage image uploader; one bounded HTTP/1.1 connection at a time.
use crate::home_image as image;
use core::fmt::Write as _;
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::{Duration, Timer, with_timeout};
use embedded_io_async::Write;
const HTML: &[u8] = include_bytes!("../../assets/web/home-image.html");
const JS: &[u8] = include_bytes!("../../assets/web/home-image.bundle.js");
pub async fn run(stack: Stack<'static>) {
    let mut rx = [0u8; 4096];
    let mut tx = [0u8; 4096];
    let mut announced = None;
    loop {
        stack.wait_config_up().await;
        if let Some(ip) = stack.config_v4().map(|c| c.address.address()) {
            if announced != Some(ip) {
                log::info!("home image web: http://{}/", ip);
                announced = Some(ip);
            }
        }
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(20)));
        if socket.accept(80).await.is_ok() {
            let _ = with_timeout(Duration::from_secs(120), serve(&mut socket)).await;
            socket.close();
            let _ = with_timeout(Duration::from_secs(2), socket.flush()).await;
        }
        socket.abort();
        Timer::after_millis(1).await;
    }
}
async fn reply(socket: &mut TcpSocket<'_>, status: &str, kind: &str, body: &[u8]) {
    let mut header = heapless::String::<256>::new();
    let _ = write!(
        header,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        status,
        kind,
        body.len()
    );
    if socket.write_all(header.as_bytes()).await.is_ok() {
        let _ = socket.write_all(body).await;
    }
}
async fn serve(socket: &mut TcpSocket<'_>) {
    let mut buffer = [0u8; 2048];
    let mut used = 0;
    let end = loop {
        if used == buffer.len() {
            reply(
                socket,
                "431 Request Header Fields Too Large",
                "text/plain",
                b"Header too large",
            )
            .await;
            return;
        }
        let Ok(n) = socket.read(&mut buffer[used..]).await else {
            return;
        };
        if n == 0 {
            return;
        }
        used += n;
        if let Some(at) = buffer[..used].windows(4).position(|v| v == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let Ok(head) = core::str::from_utf8(&buffer[..end]) else {
        return;
    };
    let mut lines = head.split("\r\n");
    let mut request = lines.next().unwrap_or("").split_whitespace();
    let method = request.next().unwrap_or("");
    let path = request.next().unwrap_or("");
    let mut length = None;
    let mut binary = false;
    let mut host = "";
    let mut origin = None;
    let mut invalid = false;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            if key.eq_ignore_ascii_case("Content-Length") {
                if length.is_some() {
                    invalid = true;
                }
                length = value.parse::<usize>().ok();
                if length.is_none() {
                    invalid = true;
                }
            }
            if key.eq_ignore_ascii_case("Transfer-Encoding") {
                invalid = true;
            }
            if key.eq_ignore_ascii_case("Content-Type") {
                binary = value == "application/octet-stream";
            }
            if key.eq_ignore_ascii_case("Host") {
                host = value;
            }
            if key.eq_ignore_ascii_case("Origin") {
                origin = Some(value);
            }
        }
    }
    if invalid {
        reply(socket, "400 Bad Request", "text/plain", b"Invalid framing").await;
        return;
    }
    if method == "GET" {
        match path {
            "/" => reply(socket, "200 OK", "text/html; charset=utf-8", HTML).await,
            "/app.js" => reply(socket, "200 OK", "text/javascript; charset=utf-8", JS).await,
            "/state" => {
                let mut body = heapless::String::<256>::new();
                image::with(|s| {
                    let _ = write!(
                        body,
                        "{{\"ready\":{},\"busy\":{},\"serial\":{},\"completed\":{},\"ok\":{},\"bytes\":{}}}",
                        image::ready() && s.loaded && s.storage,
                        s.pending.is_some() || s.uploading.is_some(),
                        s.serial,
                        s.completed,
                        s.ok,
                        s.active.map_or(0, |a| a.len)
                    );
                });
                reply(socket, "200 OK", "application/json", body.as_bytes()).await;
            }
            _ => reply(socket, "404 Not Found", "text/plain", b"Not found").await,
        }
        return;
    }
    if method != "POST" || !matches!(path, "/image" | "/default") {
        reply(
            socket,
            "405 Method Not Allowed",
            "text/plain",
            b"Unsupported request",
        )
        .await;
        return;
    }
    // No CORS: binary requests need a preflight across origins. Also reject
    // mismatched Origin explicitly; normal same-origin uploads work over LAN HTTP.
    if !binary || origin.is_some_and(|o| o.strip_prefix("http://") != Some(host)) {
        reply(
            socket,
            "403 Forbidden",
            "text/plain",
            b"Use this device's upload page",
        )
        .await;
        return;
    }
    let Some(length) = length else {
        reply(
            socket,
            "411 Length Required",
            "text/plain",
            b"Length required",
        )
        .await;
        return;
    };
    let restore = path == "/default";
    if (restore && length != 0) || (!restore && !(16..=image::LIMIT).contains(&length)) {
        reply(
            socket,
            "413 Content Too Large",
            "text/plain",
            b"Image size invalid",
        )
        .await;
        return;
    }
    let Some(upload) = image::Upload::begin() else {
        reply(
            socket,
            "503 Service Unavailable",
            "text/plain",
            b"Image storage unavailable or busy",
        )
        .await;
        return;
    };
    let initial = (used - end).min(length);
    unsafe {
        core::ptr::copy_nonoverlapping(
            buffer[end..].as_ptr(),
            image::bank_ptr(upload.bank),
            initial,
        );
    }
    let mut at = initial;
    while at < length {
        let count = (length - at).min(buffer.len());
        let Ok(n) = socket.read(&mut buffer[..count]).await else {
            return;
        };
        if n == 0 {
            return;
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                image::bank_ptr(upload.bank).add(at),
                n,
            );
        }
        at += n;
        embassy_futures::yield_now().await;
    }
    if !restore && !image::validate(upload.bank, length).await {
        reply(
            socket,
            "422 Unprocessable Content",
            "text/plain",
            b"Invalid image frames",
        )
        .await;
        return;
    }
    let serial = upload.submit(length);
    let mut body = heapless::String::<32>::new();
    let _ = write!(body, "{}", serial);
    reply(socket, "202 Accepted", "text/plain", body.as_bytes()).await;
}
