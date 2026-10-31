#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames, clippy::mem_forget)]

use embassy_executor::Executor;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    sdmmc::{Config, SdHostController, SlotConfig},
};
use esp_p4_eth::time_driver::time_polling_task;
use panic_halt as _;
use riscv_rt::entry;
use static_cell::StaticCell;
use waveshare_p4_eth::task::wifi_ble::run;
static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static SDHOST: StaticCell<SdHostController<'static>> = StaticCell::new();

esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_println::logger::init_logger(log::LevelFilter::Info);
    log::info!("wifi_ble: standalone Wi-Fi + BLE, no display / USB / PSRAM");
    let sdhost = SDHOST.init(SdHostController::new(peripherals.SDHOST, Config::default()).unwrap());
    let bus = sdhost
        .slot::<1>(SlotConfig::default())
        .unwrap()
        .with_clk(peripherals.GPIO18)
        .with_cmd(peripherals.GPIO19)
        .with_data0(peripherals.GPIO14)
        .with_data1(peripherals.GPIO15)
        .with_data2(peripherals.GPIO16)
        .with_data3(peripherals.GPIO17)
        .into_async();
    let reset = Output::new(peripherals.GPIO54, Level::Low, OutputConfig::default());
    EXECUTOR.init(Executor::new()).run(move |spawner| {
        spawner.spawn(time_polling_task()).unwrap();
        spawner.spawn(run(bus, reset)).unwrap();
    })
}
