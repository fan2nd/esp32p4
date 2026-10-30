#![no_std]
#![no_main]

use esp_hal::usb::otg::{
    Usb,
    embassy_usb_device::{Config, Driver},
};
use riscv_rt::entry;

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("verify_usb: PANIC {}", info);
    loop {
        core::hint::spin_loop();
    }
}

struct Events;
#[cfg(feature = "usb-trace")]
struct UsbLog;
#[cfg(feature = "usb-trace")]
impl log::Log for UsbLog {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        // Suppress hot-loop polling and frame IRQ traces so UART does not
        // dominate enumeration timing. Keep requests and transfer transitions.
        use core::fmt::Write;
        let mut text = heapless::String::<256>::new();
        let _ = write!(text, "{}", record.args());
        if text == "irq"
            || text == "SETUP waiting"
            || text.contains("wait for prev:")
            || text.contains("diepctl")
            || text.contains("doepctl")
            || text.contains("fifo for ep=")
        {
            return;
        }
        esp_println::println!("usb: {}", text);
    }
    fn flush(&self) {}
}
impl embassy_usb::Handler for Events {
    fn enabled(&mut self, enabled: bool) {
        esp_println::println!("verify_usb: enabled={}", enabled);
    }
    fn reset(&mut self) {
        esp_println::println!("verify_usb: reset");
    }
    fn addressed(&mut self, address: u8) {
        esp_println::println!("verify_usb: address={}", address);
    }
    fn configured(&mut self, configured: bool) {
        esp_println::println!("verify_usb: configured={}", configured);
    }
}

#[entry]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    #[cfg(feature = "usb-trace")]
    {
        log::set_logger(&UsbLog).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    }
    let mut ep_buffer = [0; 256];
    let driver = Driver::new(Usb::new_hs(p.USB_HS), &mut ep_buffer, Config::default());
    // A vendor interface isolates enumeration from the codec and audio streams.
    // Windows may lack a class driver; VID/PID appearance still proves enumeration.
    let mut config = embassy_usb::Config::new(0xcafe, 0x4012);
    config.manufacturer = Some("P4 Lab");
    config.product = Some("P4 USB enumeration probe");
    config.serial_number = Some("P4-USB-001");
    config.self_powered = true;
    config.max_power = 0;
    let mut descriptors = [0; 256];
    let mut bos = [0; 64];
    let mut msos = [0; 64];
    let mut control = [0; 64];
    let mut events = Events;
    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        &mut descriptors,
        &mut bos,
        &mut msos,
        &mut control,
    );
    builder
        .function(0xff, 0, 0)
        .interface()
        .alt_setting(0xff, 0, 0, None);
    builder.handler(&mut events);
    let mut usb = builder.build();
    // block_on does not enable the machine interrupt flag on bare metal.
    unsafe { riscv::interrupt::enable() };
    esp_println::println!("verify_usb: start HS enumeration only");
    embassy_futures::block_on(usb.run());
}
