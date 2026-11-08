#![no_std]
// Picoserve router types nested in the concurrent Embassy server futures.
#![recursion_limit = "256"]

#[cfg(any(feature = "slint-ui", feature = "pocket-ui"))]
extern crate alloc;

pub mod board;
pub mod desktop;
pub mod diagnostics;
pub mod keyboard;

/// Hardware-facing drivers: panel, codec, and touch controller.
pub mod driver {
    pub mod audio;
    pub mod display;
    pub mod home_image_store;
    pub mod hosted_bus;
    pub mod hosted_net;
    pub mod hosted_v3;
    pub mod pc_monitor;
    pub mod sd_card;
    pub mod touch;
    pub mod usb_audio;
    pub mod usb_msc;
}

/// Executor-facing long-lived application tasks.
pub mod task {
    pub mod desktop_radio;
    pub mod home_web;
    pub mod hosted;
    pub mod mdns;
    pub mod net;
    pub mod weather;
    pub mod wifi_ble;
}
