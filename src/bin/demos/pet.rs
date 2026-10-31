#![no_std]
#![no_main]

use esp_hal::{
    i2c::master::{Config, I2c},
    psram::{Psram, PsramConfig},
};
use panic_halt as _;
use riscv_rt::entry;
use waveshare_p4_eth::driver::display::{self, WIDTH};

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_WIDTH: usize = 700;
const SPRITE_HEIGHT: usize = 653;
// RGB888 artwork is precomposited against the screen background.
const GREETING: &[u8; SPRITE_WIDTH * SPRITE_HEIGHT * 3] =
    include_bytes!("../../../assets/whale-girl-wave-large.rgb");
const BACKGROUND: [u8; 3] = [232, 247, 255];
const BUBBLE_WIDTH: usize = 440;
const BUBBLE_HEIGHT: usize = 125;
const BUBBLE: &[u8; BUBBLE_WIDTH * BUBBLE_HEIGHT * 3] =
    include_bytes!("../../../assets/pet-bubble.rgb");

#[entry]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    let psram = Psram::new(p.PSRAM, PsramConfig::default());
    let (ptr, len) = psram.raw_parts();
    assert!(
        len >= display::FRAMEBUFFER_SIZE,
        "not enough PSRAM for display"
    );
    // PSRAM owns the framebuffer throughout the lifetime of the display.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, display::FRAMEBUFFER_SIZE) };
    for pixel in buffer.chunks_exact_mut(3) {
        pixel.copy_from_slice(&BACKGROUND);
    }
    for row in 0..BUBBLE_HEIGHT {
        let src = row * BUBBLE_WIDTH * 3;
        let dst = ((250 + row) * WIDTH as usize + (WIDTH as usize - BUBBLE_WIDTH) / 2) * 3;
        buffer[dst..dst + BUBBLE_WIDTH * 3].copy_from_slice(&BUBBLE[src..src + BUBBLE_WIDTH * 3]);
    }
    let x = (WIDTH as usize - SPRITE_WIDTH) / 2;
    for row in 0..SPRITE_HEIGHT {
        let src = row * SPRITE_WIDTH * 3;
        // Center the bubble and artwork together on the 720x1280 display.
        let dst = ((387 + row) * WIDTH as usize + x) * 3;
        buffer[dst..dst + SPRITE_WIDTH * 3].copy_from_slice(&GREETING[src..src + SPRITE_WIDTH * 3]);
    }

    let i2c = I2c::new(p.I2C1, Config::default())
        .expect("I2C init failed")
        .with_sda(p.GPIO7)
        .with_scl(p.GPIO8);
    let (_screen, _i2c) = display::init(
        p.MIPI_DSI,
        p.VDMA_CH0,
        i2c,
        buffer,
        display::DEFAULT_DPI_CONFIG,
    );
    // VDMA needs its interrupt enabled to keep streaming the framebuffer.
    unsafe { riscv::interrupt::enable() };
    esp_println::println!("pet: greeting image ready");
    loop {
        riscv::asm::wfi();
    }
}
