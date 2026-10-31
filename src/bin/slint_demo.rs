#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};
use esp_hal::{
    delay::Delay,
    i2c::master::{Config, I2c},
    mipi_dsi::dpi::{ColorFormat, DpiConfig, FrameTiming},
    psram::{Psram, PsramConfig},
    system::{CpuControl, Stack},
    time::Instant,
};
use riscv_rt::entry;
use slint::{
    ComponentHandle,
    platform::{
        self, PointerEventButton, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel},
    },
};
use static_cell::StaticCell;
use waveshare_p4_eth::{
    diagnostics,
    driver::{display, touch::Gt911},
};

use waveshare_p4_eth::slint_backend::{Demo, FRAME, HEAP, HEAP_SIZE, Lines, Platform};
esp_bootloader_esp_idf::esp_app_desc!();

static STACK: StaticCell<Stack<65536>> = StaticCell::new();
static EXECUTOR: StaticCell<embassy_executor::Executor> = StaticCell::new();
static FRAMES: AtomicU32 = AtomicU32::new(0);
static DRAW_US: AtomicU32 = AtomicU32::new(0);
static PIXELS: AtomicU32 = AtomicU32::new(0);

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    waveshare_p4_eth::diagnostic!("slint PANIC: {}", info);
    loop {
        core::hint::spin_loop();
    }
}

#[entry]
fn main() -> ! {
    let p =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    diagnostics::init();
    let psram = Psram::new(p.PSRAM, PsramConfig::default());
    let (ptr, len) = psram.raw_parts();
    assert!(len >= FRAME * 2 + HEAP_SIZE);
    // Disjoint PSRAM regions: scanout owns the first two frames; Slint's heap
    // follows them. Initialize before core 1 starts. UI allocation stays on core 1.
    let fb = unsafe {
        HEAP.lock().init(ptr.add(FRAME * 2), HEAP_SIZE);
        core::slice::from_raw_parts_mut(ptr, FRAME * 2)
    };
    let mut cpu = CpuControl::new(p.CPU_CTRL);
    let _guard = cpu
        .start_app_core(STACK.init(Stack::new()), move || {
            fb.fill(0);
            let i2c = I2c::new(p.I2C1, Config::default())
                .unwrap()
                .with_sda(p.GPIO7)
                .with_scl(p.GPIO8);
            let (mut dpi, mut i2c) = display::init(
                p.MIPI_DSI,
                p.VDMA_CH0,
                i2c,
                fb,
                DpiConfig {
                    in_color_format: ColorFormat::Rgb565,
                    timing: FrameTiming {
                        vfp: 1023,
                        ..display::DEFAULT_DPI_CONFIG.timing
                    },
                    ..display::DEFAULT_DPI_CONFIG
                },
            );
            unsafe {
                riscv::interrupt::enable();
            }
            let window = MinimalSoftwareWindow::new(RepaintBufferType::SwappedBuffers);
            platform::set_platform(Box::new(Platform {
                window: window.clone(),
            }))
            .unwrap();
            let ui = Demo::new().unwrap();
            window.set_size(slint::PhysicalSize::new(360, 640));
            ui.on_volume_changed(|value| log::info!("slint: volume={}", value as u32));
            ui.on_item_selected(|index| log::info!("slint: selected={}", index + 1));
            ui.show().unwrap();
            let mut touch = Gt911::probe(&mut i2c);
            log::info!("slint: core1 ready; touch={}", touch.is_some());
            let mut scratch = [Rgb565Pixel(0); 360];
            let mut position = slint::LogicalPosition::new(0., 0.);
            let mut down = false;
            let mut last_report = Instant::now();
            let mut last_probe = Instant::now();
            loop {
                if touch.is_none() && last_probe.elapsed().as_millis() >= 1000 {
                    touch = Gt911::probe(&mut i2c);
                    last_probe = Instant::now();
                    if touch.is_some() {
                        log::info!("slint: touch ready");
                    }
                }
                if let Some(points) = touch.as_mut().and_then(|t| t.poll(&mut i2c)) {
                    last_report = Instant::now();
                    if let Some(point) = points.first() {
                        position = slint::LogicalPosition::new(
                            point.position.x as f32 / 2.,
                            point.position.y as f32 / 2.,
                        );
                        window.dispatch_event(if down {
                            WindowEvent::PointerMoved { position }
                        } else {
                            WindowEvent::PointerPressed {
                                position,
                                button: PointerEventButton::Left,
                            }
                        });
                        down = true;
                    } else if down {
                        window.dispatch_event(WindowEvent::PointerReleased {
                            position,
                            button: PointerEventButton::Left,
                        });
                        window.dispatch_event(WindowEvent::PointerExited);
                        down = false;
                    }
                }
                // A lost release report must not leave a slider/list dragging forever.
                if down && last_report.elapsed().as_millis() > 500 {
                    window.dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                    window.dispatch_event(WindowEvent::PointerExited);
                    down = false;
                }
                platform::update_timers_and_animations();
                let start = Instant::now();
                let mut pixels = 0;
                if window.draw_if_needed(|renderer| {
                    renderer.render_by_line(Lines {
                        framebuffer: dpi.framebuffer_mut(),
                        scratch: &mut scratch,
                        pixels: &mut pixels,
                    });
                }) {
                    DRAW_US.store(start.elapsed().as_micros() as u32, Ordering::Relaxed);
                    PIXELS.store(pixels, Ordering::Relaxed);
                    // commit writes back every modified cache line, including history
                    // repaired for the back buffer; retain the validated safe swap wait.
                    dpi.commit();
                    for _ in 0..3 {
                        dpi.wait_for_vsync();
                    }
                    FRAMES.fetch_add(1, Ordering::Release);
                }
                Delay::new().delay_millis(5);
            }
        })
        .unwrap();
    EXECUTOR
        .init(embassy_executor::Executor::new())
        .run(|spawner| {
            spawner
                .spawn(esp_p4_eth::time_driver::time_polling_task())
                .unwrap();
            spawner.spawn(diagnostics::run()).unwrap();
            spawner.spawn(stats()).unwrap();
        });
}

#[embassy_executor::task]
async fn stats() {
    loop {
        embassy_time::Timer::after_secs(2).await;
        log::info!(
            "slint: frames={} last_draw={}us logical_pixels={} heap_used={}",
            FRAMES.load(Ordering::Acquire),
            DRAW_US.load(Ordering::Relaxed),
            PIXELS.load(Ordering::Relaxed),
            HEAP.lock().used()
        );
    }
}
