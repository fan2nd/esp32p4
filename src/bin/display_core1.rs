#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use esp_hal::{
    delay::Delay,
    i2c::master::{Config, I2c},
    mipi_dsi::dpi::{ColorFormat, DpiConfig},
    psram::{Psram, PsramConfig},
    system::{CpuControl, Stack},
    time::Instant,
};
use riscv_rt::entry;
use static_cell::StaticCell;
use waveshare_p4_eth::driver::display;

esp_bootloader_esp_idf::esp_app_desc!();
static SCREEN_STACK: StaticCell<Stack<32768>> = StaticCell::new();
static FRAMES: AtomicU32 = AtomicU32::new(0);
static STAGE: AtomicU32 = AtomicU32::new(0);
const WIDTH: usize = 720;
const HEIGHT: usize = 1280;
const FRAME: usize = WIDTH * HEIGHT * 2;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!(
        "display_core1 PANIC {:?}: {}",
        esp_hal::system::Cpu::current(),
        info
    );
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn ExceptionHandler(_frame: *mut u32) -> ! {
    esp_println::println!(
        "display_core1 EXCEPTION {:?}: cause={:#x} pc={:#x} value={:#x}",
        esp_hal::system::Cpu::current(),
        riscv::register::mcause::read().bits(),
        riscv::register::mepc::read(),
        riscv::register::mtval::read()
    );
    loop {
        core::hint::spin_loop();
    }
}

#[entry]
fn main() -> ! {
    let p =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    #[cfg(feature = "verify-radio")]
    waveshare_p4_eth::diagnostics::init();
    // Configure the shared PSRAM mapping before the second CPU starts accessing it.
    let psram = Psram::new(p.PSRAM, PsramConfig::default());
    let (ptr, len) = psram.raw_parts();
    assert!(len >= FRAME * 2);
    // Only core 1 receives this exclusive slice. Core 0 never touches framebuffer memory.
    let fb = unsafe { core::slice::from_raw_parts_mut(ptr, FRAME * 2) };
    let mut cpu = CpuControl::new(p.CPU_CTRL);
    let _guard = cpu
        .start_app_core(SCREEN_STACK.init(Stack::new()), move || {
            STAGE.store(1, Ordering::Release);
            let i2c = I2c::new(p.I2C1, Config::default())
                .unwrap()
                .with_sda(p.GPIO7)
                .with_scl(p.GPIO8);
            draw(&mut fb[..FRAME], 0);
            let (front, back) = fb.split_at_mut(FRAME);
            back.copy_from_slice(front);
            STAGE.store(2, Ordering::Release);
            let (mut dpi, _i2c) = display::init(
                p.MIPI_DSI,
                p.VDMA_CH0,
                i2c,
                fb,
                DpiConfig {
                    in_color_format: ColorFormat::Rgb565,
                    ..display::DEFAULT_DPI_CONFIG
                },
            );
            // DPI/VDMA interrupt routing is installed from this core, its sole owner.
            unsafe {
                riscv::interrupt::enable();
            }
            STAGE.store(3, Ordering::Release);
            let start = Instant::now();
            loop {
                // Diagnostic control: scanout/VDMA remain active, but CPU writes
                // and full-frame cache writeback stop after the two startup flips.
                if cfg!(feature = "verify-static") && FRAMES.load(Ordering::Acquire) >= 2 {
                    break;
                }
                draw(dpi.framebuffer_mut(), start.elapsed().as_millis() as u32);
                STAGE.store(4, Ordering::Release);
                dpi.commit();
                STAGE.store(5, Ordering::Release);
                // First VSYNC may already be pending. Two fresh boundaries let both
                // descriptor slots observe the new source before reusing the old front.
                for _ in 0..3 {
                    dpi.wait_for_vsync();
                }
                FRAMES.fetch_add(1, Ordering::Release);
                STAGE.store(3, Ordering::Release);
            }
            if cfg!(feature = "verify-stop-display") {
                drop(dpi);
                STAGE.store(7, Ordering::Release);
            } else {
                STAGE.store(6, Ordering::Release);
            }
            loop {
                Delay::new().delay_millis(10);
            }
        })
        .unwrap();
    #[cfg(feature = "verify-radio")]
    {
        use esp_hal::{
            gpio::{Level, Output, OutputConfig},
            sdmmc::{Config as SdConfig, SdHostController, SlotConfig},
        };
        static EXECUTOR: StaticCell<embassy_executor::Executor> = StaticCell::new();
        static HOST: StaticCell<SdHostController<'static>> = StaticCell::new();
        // Start both workloads without an artificial display-before-radio delay.
        // HAL serializes their shared cache maintenance operations across cores.
        let host = HOST.init(SdHostController::new(p.SDHOST, SdConfig::default()).unwrap());
        let bus = host
            .slot::<1>(SlotConfig::default())
            .unwrap()
            .with_clk(p.GPIO18)
            .with_cmd(p.GPIO19)
            .with_data0(p.GPIO14)
            .with_data1(p.GPIO15)
            .with_data2(p.GPIO16)
            .with_data3(p.GPIO17)
            .into_async();
        let reset = Output::new(p.GPIO54, Level::Low, OutputConfig::default());
        // Guard stays in this never-returning main frame: core 1 must remain running.
        EXECUTOR
            .init(embassy_executor::Executor::new())
            .run(move |spawner| {
                spawner
                    .spawn(esp_p4_eth::time_driver::time_polling_task())
                    .unwrap();
                spawner.spawn(waveshare_p4_eth::diagnostics::run()).unwrap();
                spawner
                    .spawn(waveshare_p4_eth::task::wifi_ble::run(bus, reset))
                    .unwrap();
                spawner.spawn(heartbeat()).unwrap();
            });
    }
    // A separate heartbeat proves core 0 keeps running during core 1's refresh.
    #[cfg(not(feature = "verify-radio"))]
    {
        let mut last = 0;
        loop {
            Delay::new().delay_millis(2000);
            let frames = FRAMES.load(Ordering::Acquire);
            esp_println::println!(
                "core0 alive; core1 stage={} frames={} updates/2s={}",
                STAGE.load(Ordering::Acquire),
                frames,
                frames.wrapping_sub(last)
            );
            last = frames;
        }
    }
}

#[cfg(feature = "verify-radio")]
#[embassy_executor::task]
async fn heartbeat() {
    let mut previous = 0;
    loop {
        embassy_time::Timer::after_secs(2).await;
        let frames = FRAMES.load(Ordering::Acquire);
        log::info!(
            "coexist: core0 alive; core1 stage={} frames={} updates/2s={}",
            STAGE.load(Ordering::Acquire),
            frames,
            frames.wrapping_sub(previous)
        );
        previous = frames;
    }
}

/// Full-frame word writes via one internal-RAM row. BGR565 matches the verified
/// bridge/panel channel ordering. The RGB bars and border make corruption visible.
fn draw(fb: &mut [u8], milliseconds: u32) {
    let mut row = [0u32; WIDTH / 2];
    let travel = (milliseconds / 5) % 1080;
    let x = 30 + if travel < 540 { travel } else { 1080 - travel } as usize;
    let phase = (milliseconds / 5) % 1760;
    let y = 240 + if phase < 880 { phase } else { 1760 - phase } as usize;
    for py in 0..HEIGHT {
        for (word, pair) in row.iter_mut().zip((0..WIDTH).step_by(2)) {
            let mut packed = 0;
            for pixel in 0..2 {
                let px = pair + pixel;
                let color: u16 = if !(8..WIDTH - 8).contains(&px) || !(8..HEIGHT - 8).contains(&py)
                {
                    0xffff
                } else if (40..180).contains(&py) {
                    match px / 240 {
                        0 => 0x001f,
                        1 => 0x07e0,
                        _ => 0xf800,
                    }
                } else if (x..x + 120).contains(&px) && (y..y + 120).contains(&py) {
                    0x07ff // Yellow, moves at a time-based 200 pixels/second.
                } else {
                    0x1082
                };
                packed |= (color as u32) << (pixel * 16);
            }
            *word = packed;
        }
        // Framebuffer base and 1440-byte row stride are u32 aligned.
        unsafe {
            core::ptr::copy_nonoverlapping(
                row.as_ptr(),
                fb.as_mut_ptr().add(py * WIDTH * 2).cast::<u32>(),
                row.len(),
            );
        }
    }
}
