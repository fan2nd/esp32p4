#![no_std]
#![no_main]

use esp_hal::{
    delay::Delay,
    i2c::master::{Config as I2cConfig, I2c},
    mipi_dsi::dpi::{ColorFormat, DpiConfig},
    ppa::{Config, Input, Offset, Output, PixelFormat, Rotation, Size, Srm},
    psram::{Psram, PsramConfig},
    time::Instant,
};
use riscv_rt::entry;
use waveshare_p4_eth::driver::display;

esp_bootloader_esp_idf::esp_app_desc!();
const FRAME: usize = 720 * 1280 * 2;
const SOURCE: usize = 128 * 128 * 2;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("ppa_demo PANIC: {}", info);
    loop {
        core::hint::spin_loop();
    }
}

#[entry]
fn main() -> ! {
    let p =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    let memory = Psram::new(p.PSRAM, PsramConfig::default());
    let (ptr, len) = memory.raw_parts();
    assert!(len >= FRAME * 2 + SOURCE);
    let all = unsafe { core::slice::from_raw_parts_mut(ptr, FRAME * 2 + SOURCE) };
    let (frames, source) = all.split_at_mut(FRAME * 2);
    // Small PSRAM source, no static SRAM image/scratch buffers.
    for (i, pixel) in source.chunks_exact_mut(2).enumerate() {
        let value = (((i % 128) / 4) as u16)
            | ((((i / 128) / 2) as u16) << 5)
            | ((((i % 128 + i / 128) / 8) as u16) << 11);
        pixel.copy_from_slice(&value.to_le_bytes());
    }
    let mut srm = Srm::take().unwrap();
    let size = Size {
        width: 128,
        height: 128,
    };
    let started = Instant::now();
    srm.run(
        Input {
            data: source,
            size,
            format: PixelFormat::Rgb565,
        },
        Output {
            data: &mut frames[..SOURCE],
            size,
            format: PixelFormat::Rgb565,
            offset: Offset::default(),
        },
        Config::default(),
    )
    .unwrap();
    let copy_us = started.elapsed().as_micros();
    let mismatch = source
        .iter()
        .zip(&frames[..SOURCE])
        .filter(|(a, b)| a != b)
        .count();
    esp_println::println!(
        "ppa: PSRAM RGB565 identity {} us, mismatched bytes={}",
        copy_us,
        mismatch
    );
    assert_eq!(mismatch, 0);
    // Check orientation against CPU coordinate mapping, including all mirror combinations.
    for (turn, rotation) in [
        Rotation::Degrees0,
        Rotation::Degrees90,
        Rotation::Degrees180,
        Rotation::Degrees270,
    ]
    .into_iter()
    .enumerate()
    {
        for mirror_x in [false, true] {
            for mirror_y in [false, true] {
                srm.run(
                    Input {
                        data: source,
                        size,
                        format: PixelFormat::Rgb565,
                    },
                    Output {
                        data: &mut frames[..SOURCE],
                        size,
                        format: PixelFormat::Rgb565,
                        offset: Offset::default(),
                    },
                    Config::default()
                        .with_rotation(rotation)
                        .with_mirror_x(mirror_x)
                        .with_mirror_y(mirror_y),
                )
                .unwrap();
                for y in 0..128 {
                    for x in 0..128 {
                        let (dx, dy) = (
                            if mirror_x { 127 - x } else { x },
                            if mirror_y { 127 - y } else { y },
                        );
                        let (sx, sy) = match turn {
                            0 => (dx, dy),
                            1 => (127 - dy, dx),
                            2 => (127 - dx, 127 - dy),
                            _ => (dy, 127 - dx),
                        };
                        assert_eq!(
                            &frames[(y * 128 + x) * 2..(y * 128 + x) * 2 + 2],
                            &source[(sy * 128 + sx) * 2..(sy * 128 + sx) * 2 + 2]
                        );
                    }
                }
            }
        }
    }
    esp_println::println!("ppa: 16 rotation/mirror combinations verified pixel-by-pixel");
    // RGB565 conversion expands channels without replicating low bits (upstream API contract).
    for (format, bpp) in [(PixelFormat::Rgb888, 3), (PixelFormat::Argb8888, 4)] {
        srm.run(
            Input {
                data: source,
                size,
                format: PixelFormat::Rgb565,
            },
            Output {
                data: &mut frames[..128 * 128 * bpp],
                size,
                format,
                offset: Offset::default(),
            },
            Config::default(),
        )
        .unwrap();
        for i in 0..128 * 128 {
            let value = u16::from_le_bytes([source[i * 2], source[i * 2 + 1]]);
            assert_eq!(frames[i * bpp], ((value & 31) << 3) as u8);
            assert_eq!(frames[i * bpp + 1], (((value >> 5) & 63) << 2) as u8);
            assert_eq!(frames[i * bpp + 2], ((value >> 11) << 3) as u8);
            if bpp == 4 {
                assert_eq!(frames[i * bpp + 3], 255);
            }
        }
    }
    esp_println::println!("ppa: RGB888/ARGB8888 output verified");
    // A small aligned SRAM transfer exercises the internal-cache path without large scratch space.
    #[repr(C, align(128))]
    struct Small([u8; 512]);
    let small_source = Small([0xa5; 512]);
    let mut small_output = Small([0; 512]);
    let small_size = Size {
        width: 16,
        height: 16,
    };
    srm.run(
        Input {
            data: &small_source.0,
            size: small_size,
            format: PixelFormat::Rgb565,
        },
        Output {
            data: &mut small_output.0,
            size: small_size,
            format: PixelFormat::Rgb565,
            offset: Offset::default(),
        },
        Config::default(),
    )
    .unwrap();
    assert_eq!(small_source.0, small_output.0);
    esp_println::println!("ppa: SRAM copy verified");
    // Repeat with a changed source to catch missing CPU writeback / DMA invalidation.
    source.fill(0x55);
    for _ in 0..8 {
        srm.run(
            Input {
                data: source,
                size,
                format: PixelFormat::Rgb565,
            },
            Output {
                data: &mut frames[..SOURCE * 4],
                size: Size {
                    width: 256,
                    height: 256,
                },
                format: PixelFormat::Rgb565,
                offset: Offset::default(),
            },
            Config::default().with_scale_x(2.0).with_scale_y(2.0),
        )
        .unwrap();
        assert!(frames[..SOURCE * 4].iter().all(|v| *v == 0x55));
    }
    esp_println::println!("ppa: 2x RGB565 uniform image verified, 8 transfers");
    // Non-macroblock-sized input exercises P4's partial-transfer FIFO workaround.
    for rotation in [
        Rotation::Degrees0,
        Rotation::Degrees90,
        Rotation::Degrees180,
        Rotation::Degrees270,
    ] {
        let odd = Size {
            width: 17,
            height: 19,
        };
        let config = Config::default().with_rotation(rotation);
        srm.run(
            Input {
                data: &source[..17 * 19 * 2],
                size: odd,
                format: PixelFormat::Rgb565,
            },
            Output {
                data: &mut frames[..768],
                size: config.output_size(odd).unwrap(),
                format: PixelFormat::Rgb565,
                offset: Offset::default(),
            },
            config,
        )
        .unwrap();
        assert!(frames[..17 * 19 * 2].iter().all(|v| *v == 0x55));
    }
    esp_println::println!("ppa: 17x19 partial blocks verified in all rotations");
    // Asymmetric color blocks make rotations/mirroring visible on the panel.
    for (i, pixel) in source.chunks_exact_mut(2).enumerate() {
        let (x, y) = (i % 128, i / 128);
        let value: u16 = match (x < 64, y < 64) {
            (true, true) => 0x001f,
            (false, true) => 0x07e0,
            (true, false) => 0xf800,
            (false, false) => 0xffff,
        };
        pixel.copy_from_slice(&value.to_le_bytes());
    }
    // verify-static isolates the LCD path: PPA is clock-gated before scanout starts.
    let mut srm = if cfg!(feature = "verify-static") {
        drop(srm);
        None
    } else {
        Some(srm)
    };
    esp_println::println!("ppa: CPU-only display control={}", srm.is_none());
    frames.fill(0);
    let i2c = I2c::new(p.I2C1, I2cConfig::default())
        .unwrap()
        .with_sda(p.GPIO7)
        .with_scl(p.GPIO8);
    let (mut dpi, _i2c) = display::init(
        p.MIPI_DSI,
        p.VDMA_CH0,
        i2c,
        frames,
        DpiConfig {
            in_color_format: ColorFormat::Rgb565,
            ..display::DEFAULT_DPI_CONFIG
        },
    );
    unsafe {
        riscv::interrupt::enable();
    }
    let mut iteration = 0usize;
    loop {
        let rotation = [
            Rotation::Degrees0,
            Rotation::Degrees90,
            Rotation::Degrees180,
            Rotation::Degrees270,
        ][iteration % 4];
        let started = Instant::now();
        if let Some(engine) = srm.as_mut() {
            engine
                .run(
                    Input {
                        data: source,
                        size,
                        format: PixelFormat::Rgb565,
                    },
                    Output {
                        data: dpi.framebuffer_mut(),
                        size: Size {
                            width: 720,
                            height: 1280,
                        },
                        format: PixelFormat::Rgb565,
                        offset: Offset { x: 64, y: 512 },
                    },
                    Config::default()
                        .with_scale_x(2.0)
                        .with_scale_y(2.0)
                        .with_rotation(rotation),
                )
                .unwrap();
        }
        let elapsed = started.elapsed().as_micros();
        // Read the actual DMA result before flipping; detect holes independently of LCD timing.
        let fb = dpi.framebuffer_mut();
        // Right: CPU nearest-neighbour reference, left: PPA bilinear output.
        // Both use the same source and destination scanout / cache commit.
        for y in 0..256 {
            for x in 0..256 {
                let (dx, dy) = (x / 2, y / 2);
                let (sx, sy) = match iteration % 4 {
                    0 => (dx, dy),
                    1 => (127 - dy, dx),
                    2 => (127 - dx, 127 - dy),
                    _ => (dy, 127 - dx),
                };
                let i = ((y + 512) * 720 + x + 400) * 2;
                fb[i..i + 2].copy_from_slice(&source[(sy * 128 + sx) * 2..(sy * 128 + sx) * 2 + 2]);
                if srm.is_none() {
                    let left = ((y + 512) * 720 + x + 64) * 2;
                    fb[left..left + 2]
                        .copy_from_slice(&source[(sy * 128 + sx) * 2..(sy * 128 + sx) * 2 + 2]);
                }
            }
        }
        let mut black = 0usize;
        let mut first = (0usize, 0usize);
        for y in 0..256 {
            for x in 0..256 {
                let i = ((y + 512) * 720 + x + 64) * 2;
                if fb[i] == 0 && fb[i + 1] == 0 {
                    if black == 0 {
                        first = (x, y);
                    }
                    black += 1;
                }
            }
        }
        esp_println::println!("ppa: readback black_pixels={} first={:?}", black, first);
        // Consume stale VSYNC then flip near a new blanking boundary.
        let _ = dpi.try_wait_for_vsync();
        dpi.wait_for_vsync();
        dpi.commit();
        for _ in 0..3 {
            dpi.wait_for_vsync();
        }
        esp_println::println!(
            "ppa: DSI concurrent frame={} rotation={:?} elapsed_us={}",
            iteration,
            rotation,
            elapsed
        );
        iteration += 1;
        Delay::new().delay_millis(2000);
    }
}
