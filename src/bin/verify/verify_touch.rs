#![no_std]
#![no_main]

use esp_hal::{
    delay::Delay,
    i2c::master::{Config, I2c},
};
use panic_halt as _;
use riscv_rt::entry;
use waveshare_p4_eth::driver::touch::{self, Gt911};

esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    let i2c = I2c::new(p.I2C1, Config::default())
        .expect("I2C init failed")
        .with_sda(p.GPIO7)
        .with_scl(p.GPIO8);
    let mut i2c = i2c;
    let Some(mut touch) = Gt911::probe(&mut i2c) else {
        esp_println::println!("verify_touch: GT911 not found; check shared I2C wiring/address");
        loop {
            Delay::new().delay_millis(1000);
        }
    };
    esp_println::println!(
        "verify_touch: GT911 ready; native coordinates x=0..719 right, y=0..1279 down"
    );
    let mut idle = 0u32;
    loop {
        if let Some(points) = touch.poll(&mut i2c) {
            esp_println::println!("verify_touch: contacts={}", points.len());
            for point in points {
                // Always print native coordinates, even outside expected panel bounds.
                esp_println::println!(
                    "verify_touch: id={} raw x={} y={}",
                    point.id,
                    point.position.x,
                    point.position.y
                );
                if !(0..i32::from(touch::WIDTH)).contains(&point.position.x)
                    || !(0..i32::from(touch::HEIGHT)).contains(&point.position.y)
                {
                    esp_println::println!("verify_touch: id={} out of range", point.id);
                }
            }
            idle = 0;
        }
        idle += 1;
        if idle % 50 == 0 {
            esp_println::println!("verify_touch: waiting for touch");
        }
        Delay::new().delay_millis(1000);
    }
}
