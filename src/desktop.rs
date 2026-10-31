//! Shared desktop state. Core 0 owns USB/radio; core 1 owns display and touch.
use crate::keyboard::Keymap;
use core::cell::RefCell;
use critical_section::Mutex;
use heapless::{Deque, String, Vec};

#[derive(Clone, PartialEq, Eq)]
pub struct AccessPoint {
    pub ssid: String<32>,
    pub rssi: i32,
    pub secure: bool,
}
pub enum Command {
    Scan,
    Connect {
        ssid: String<32>,
        password: String<64>,
    },
    Disconnect,
}
#[derive(Clone)]
pub struct View {
    pub ble_name: String<24>,
    pub ble_name_status: &'static str,
    pub ble_advertising: bool,
    pub wifi: &'static str,
    pub bluetooth: &'static str,
    pub usb: bool,
    pub ip: Option<[u8; 4]>,
    pub networks: Vec<AccessPoint, 64>,
    pub utc: Option<(u64, u64)>,
    pub message: &'static str,
    pub revision: u32,
}
pub struct Shared {
    pub ble_name_command: Option<String<24>>,
    pub view: View,
    pub command: Option<Command>,
    pub keymap: Keymap,
    pub reports: Deque<[u8; 10], 32>,
    pub pressed: u8,
}
impl Shared {
    pub fn new(keymap: Keymap) -> Self {
        Self {
            ble_name_command: None,
            view: View {
                ble_name: String::try_from("P4 Desktop").unwrap(),
                ble_name_status: "Not advertising",
                ble_advertising: false,
                wifi: "STARTING",
                bluetooth: "STARTING",
                usb: false,
                ip: None,
                networks: Vec::new(),
                utc: None,
                message: "",
                revision: 0,
            },
            command: None,
            keymap,
            reports: Deque::new(),
            pressed: 0,
        }
    }
    pub fn report(&mut self, report: [u8; 10]) {
        if self.reports.is_full() {
            self.reports.clear();
            self.reports.push_back([0; 10]).unwrap();
        }
        self.reports.push_back(report).unwrap();
    }
}
pub static SHARED: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));
pub static FLASH_REQUEST: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
pub static FLASH_PAUSED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
/// Core 1 must execute entirely from internal RAM while core 0 writes flash.
/// No cross-core mutex is held here, so esp-storage may take its own lock.
#[esp_hal::ram]
pub fn park_for_flash() {
    use core::sync::atomic::Ordering;
    let interrupts = riscv::register::mstatus::read().mie();
    riscv::interrupt::disable();
    FLASH_PAUSED.store(true, Ordering::Release);
    while FLASH_REQUEST.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    FLASH_PAUSED.store(false, Ordering::Release);
    if interrupts {
        unsafe {
            riscv::interrupt::enable();
        }
    }
}
/// Short cross-core transactions only: never render, touch I2C, or await here.
pub fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    critical_section::with(|cs| f(SHARED.borrow(cs).borrow_mut().as_mut().unwrap()))
}

/// Session-only playback attenuation. 100% is unity, never positive gain.
pub static PLAYBACK_VOLUME: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(100);
