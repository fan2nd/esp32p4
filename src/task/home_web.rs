//! Picoserve routes for desktop assets, settings and bounded streaming uploads.
use crate::{desktop, desktop::home_image as image};
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::{Duration, Timer, with_timeout};
use picoserve::{
    ResponseSent,
    io::Read,
    request::{Request, RequestParts},
    response::{IntoResponse, Json, ResponseWriter, StatusCode, fs::File},
    routing::{RequestHandlerService, get, get_service, post_service},
};

const HEADERS: [(&str, &str); 2] = [
    ("Cache-Control", "no-store"),
    ("X-Content-Type-Options", "nosniff"),
];
const HTML: File = File::with_content_type_and_headers(
    "text/html; charset=utf-8",
    include_bytes!("../../assets/web/home-image.html"),
    &HEADERS,
);
const CSS: File = File::with_content_type_and_headers(
    "text/css; charset=utf-8",
    include_bytes!("../../assets/web/pico.min.css"),
    &HEADERS,
);
const IMAGE_JS: File = File::with_content_type_and_headers(
    "text/javascript; charset=utf-8",
    include_bytes!("../../assets/web/home-image.bundle.js"),
    &HEADERS,
);
const WEATHER_JS: File = File::with_content_type_and_headers(
    "text/javascript; charset=utf-8",
    include_bytes!("../../assets/web/weather.js"),
    &HEADERS,
);

pub async fn run(stack: Stack<'static>) {
    let app = picoserve::Router::new()
        .route("/", get_service(HTML))
        .route("/pico.css", get_service(CSS))
        .route("/app.js", get_service(IMAGE_JS))
        .route("/weather.js", get_service(WEATHER_JS))
        .route("/state", get(image_state))
        .route("/weather", get(weather_state).post_service(WeatherUpdate))
        .route("/image", post_service(ImageUpload { restore: false }))
        .route("/default", post_service(ImageUpload { restore: true }));
    let config = picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Duration::from_secs(5),
        persistent_start_read_request: Duration::from_secs(1),
        read_request: Duration::from_secs(20),
        write: Duration::from_secs(10),
    })
    .close_connection_after_response();
    // Browsers open several connections in parallel for HTML/CSS/scripts.
    // A lone smoltcp listener has no accept backlog and rejects those peers.
    embassy_futures::join::join_array(
        [true, false, false, false, false, false]
            .map(|announce| serve(stack, &app, &config, announce)),
    )
    .await;
}

async fn serve<P: picoserve::routing::PathRouter>(
    stack: Stack<'static>,
    app: &picoserve::Router<P>,
    config: &picoserve::Config,
    announce: bool,
) {
    let mut rx = [0; 2048];
    let mut tx = [0; 2048];
    let mut http = [0; 2048];
    let mut announced = None;
    loop {
        stack.wait_config_up().await;
        if let Some(ip) = stack.config_v4().map(|c| c.address.address()) {
            if announce && announced != Some(ip) {
                log::info!("home image web: http://{}/ (picoserve)", ip);
                announced = Some(ip);
            }
        }
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(20)));
        if socket.accept(80).await.is_ok() {
            // Keep a total connection bound in addition to framework read/write
            // deadlines. Cancellation drops Upload and releases its PSRAM bank.
            let _ = with_timeout(
                Duration::from_secs(120),
                picoserve::Server::new(app, config, &mut http).serve(socket),
            )
            .await;
        }
        Timer::after_millis(1).await;
    }
}

#[derive(serde::Serialize)]
struct ImageState {
    ready: bool,
    busy: bool,
    serial: u32,
    completed: u32,
    ok: bool,
    bytes: usize,
}
async fn image_state() -> impl IntoResponse {
    let state = image::with(|s| ImageState {
        ready: image::ready() && s.loaded && s.storage,
        busy: s.pending.is_some() || s.uploading.is_some(),
        serial: s.serial,
        completed: s.completed,
        ok: s.ok,
        bytes: s.active.map_or(0, |a| a.len),
    });
    Json(state).into_response().with_headers(HEADERS)
}
async fn weather_state() -> impl IntoResponse {
    let state = desktop::with(|s| {
        (
            s.settings.weather.clone(),
            s.view.weather.clone(),
            s.view.save_status,
        )
    });
    Json(state).into_response().with_headers(HEADERS)
}

/// Application policy, applied to already parsed framework headers. Picoserve
/// owns HTTP parsing/framing; reject ambiguous lengths and unsupported transfer
/// codings before acquiring an upload bank or mutating configuration.
fn mutation_length(parts: RequestParts<'_>) -> Result<usize, (StatusCode, &'static str)> {
    let mut length = None;
    let mut host = None;
    let mut origin = None;
    let mut binary = false;
    let mut has_type = false;
    for (name, value) in parts.headers().iter() {
        let value = value
            .as_str()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid header"))?;
        if name == "content-length" {
            if length.is_some() || value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
                return Err((StatusCode::BAD_REQUEST, "Ambiguous Content-Length"));
            }
            length = Some(
                value
                    .parse()
                    .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid Content-Length"))?,
            );
        } else if name == "transfer-encoding" {
            return Err((StatusCode::BAD_REQUEST, "Transfer-Encoding unsupported"));
        } else if name == "host" {
            if host.replace(value).is_some() {
                return Err((StatusCode::BAD_REQUEST, "Duplicate Host"));
            }
        } else if name == "origin" {
            if origin.replace(value).is_some() {
                return Err((StatusCode::BAD_REQUEST, "Duplicate Origin"));
            }
        } else if name == "content-type" {
            if has_type {
                return Err((StatusCode::BAD_REQUEST, "Duplicate Content-Type"));
            }
            has_type = true;
            binary = value == "application/octet-stream";
        }
    }
    if !binary || origin.is_some_and(|o| o.strip_prefix("http://") != host) {
        return Err((StatusCode::FORBIDDEN, "Use this device's settings page"));
    }
    length.ok_or((StatusCode::LENGTH_REQUIRED, "Content-Length required"))
}

struct ImageUpload {
    restore: bool,
}
impl RequestHandlerService for ImageUpload {
    async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _: &(),
        _: (),
        mut request: Request<'_, R>,
        writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let result: Result<u32, (StatusCode, &'static str)> = async {
            let length = mutation_length(request.parts)?;
            if (self.restore && length != 0)
                || (!self.restore && !(16..=image::LIMIT).contains(&length))
            {
                return Err((StatusCode::PAYLOAD_TOO_LARGE, "Image size invalid"));
            }
            let upload = image::Upload::begin().ok_or((
                StatusCode::SERVICE_UNAVAILABLE,
                "Image storage unavailable or busy",
            ))?;
            let mut scratch = [0; 2048];
            let mut body = request.body_connection.body().reader();
            let mut at = 0;
            while at < length {
                let count = (length - at).min(scratch.len());
                let n = body
                    .read(&mut scratch[..count])
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "Upload interrupted"))?;
                if n == 0 {
                    return Err((StatusCode::BAD_REQUEST, "Truncated upload"));
                }
                // Only the claimed inactive bank is writable; body bytes never
                // alias scanout, decoded frames, or a pending Flash commit.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        scratch.as_ptr(),
                        image::bank_ptr(upload.bank).add(at),
                        n,
                    );
                }
                at += n;
                embassy_futures::yield_now().await;
            }
            if !self.restore && !image::validate(upload.bank, length).await {
                return Err((StatusCode::UNPROCESSABLE_ENTITY, "Invalid image frames"));
            }
            Ok(upload.submit(length))
        }
        .await;
        let connection = request.body_connection.finalize().await?;
        match result {
            Ok(serial) => {
                (StatusCode::ACCEPTED, HEADERS, format_args!("{}", serial))
                    .write_to(connection, writer)
                    .await
            }
            Err((code, message)) => (code, HEADERS, message).write_to(connection, writer).await,
        }
    }
}

struct WeatherUpdate;
impl RequestHandlerService for WeatherUpdate {
    async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _: &(),
        _: (),
        mut request: Request<'_, R>,
        writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let result: Result<(), (StatusCode, &'static str)> = async {
            let length = mutation_length(request.parts)?;
            if !(1..=256).contains(&length) {
                return Err((StatusCode::PAYLOAD_TOO_LARGE, "Invalid configuration size"));
            }
            // Small JSON settings can use the framework's bounded body buffer.
            let data = request
                .body_connection
                .body()
                .read_all()
                .await
                .map_err(|_| (StatusCode::BAD_REQUEST, "Incomplete configuration"))?;
            let (location, consumed) =
                serde_json_core::from_slice::<Option<crate::task::weather::Location>>(data)
                    .map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "Invalid location"))?;
            if !data[consumed..].iter().all(u8::is_ascii_whitespace)
                || location.as_ref().is_some_and(|p| !p.valid())
            {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Invalid coordinates or name",
                ));
            }
            desktop::with(|s| {
                if s.reset_requested {
                    return Err((StatusCode::CONFLICT, "Reset in progress"));
                }
                if s.settings.weather != location {
                    s.settings.weather = location;
                    s.settings.revision = s.settings.revision.wrapping_add(1);
                    s.view.save_status = "Save pending...";
                    s.view.weather = heapless::String::try_from(if s.settings.weather.is_some() {
                        "Weather: updating..."
                    } else {
                        "Weather: configure on web"
                    })
                    .unwrap();
                    s.view.revision = s.view.revision.wrapping_add(1);
                }
                Ok(())
            })
        }
        .await;
        let (code, message) = match result {
            Ok(()) => (StatusCode::ACCEPTED, "Configuration accepted"),
            Err(error) => error,
        };
        (code, HEADERS, message)
            .write_to(request.body_connection.finalize().await?, writer)
            .await
    }
}
