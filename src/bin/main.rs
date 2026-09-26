#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames, clippy::mem_forget)]

use core::hint::spin_loop;

use embassy_executor::{Executor, Spawner};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::psram::{Psram, PsramConfig};
use esp_p4_eth::{ethernet_task, release_waveshare_phy_reset, time_driver::time_polling_task};
use panic_halt as _;
use riscv_rt::entry;
use static_cell::StaticCell;
use waveshare_p4_eth::{
    board,
    driver::display,
    task::{mdns, net},
};

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let psram = Psram::new(peripherals.PSRAM, PsramConfig::default());
    let i2c = I2c::new(peripherals.I2C1, I2cConfig::default())
        .expect("panel I2C init failed")
        .with_sda(peripherals.GPIO7)
        .with_scl(peripherals.GPIO8);
    let (fb_ptr, fb_len) = psram.raw_parts();
    let framebuffer = unsafe { core::slice::from_raw_parts_mut(fb_ptr, display::FRAMEBUFFER_SIZE) };
    assert!(fb_len >= display::FRAMEBUFFER_SIZE);
    framebuffer.fill(0);
    let (_display, _i2c) =
        display::init(peripherals.MIPI_DSI, peripherals.VDMA_CH0, i2c, framebuffer);
    esp_println::println!("app: display ready");

    release_waveshare_phy_reset();
    for _ in 0..board::PHY_RESET_WAIT_CYCLES {
        spin_loop();
    }

    let executor = EXECUTOR.init(Executor::new());
    executor.run(init)
}

fn init(spawner: Spawner) {
    let net::Network {
        stack,
        net_runner,
        eth_runner,
        ethernet,
        ip,
    } = net::init();
    spawner.spawn(time_polling_task()).unwrap();
    spawner.spawn(ethernet_task(eth_runner, ethernet)).unwrap();
    spawner.spawn(net::drive_network(net_runner)).unwrap();
    spawner.spawn(net::maintain_ethernet(stack)).unwrap();
    let mdns = mdns::MdnsConfig {
        host_fqdn: board::HOST_FQDN,
        service_instance: board::SERVICE_INSTANCE,
        service_type: board::SERVICE_TYPE,
        service_txt: board::SERVICE_TXT,
        port: board::HTTP_PORT,
        ip,
    };
    spawner.spawn(mdns::serve(stack, mdns)).unwrap();
}
