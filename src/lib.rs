#![no_std]
// Picoserve router types nested in the concurrent Embassy server futures.
#![recursion_limit = "256"]

#[cfg(any(feature = "slint-ui", feature = "pocket-ui"))]
extern crate alloc;

#[cfg(feature = "pocket-ui")]
pub mod pocket_desktop;

pub mod board;
pub mod diagnostics;
pub mod keyboard;
pub mod desktop;
pub mod desktop_settings;
pub mod desktop_protocol;
pub mod desktop_controls;
pub mod touchpad;
pub mod home_image;
pub mod tiny_desktop_page;
pub mod tiny_desktop_ui;
#[cfg(feature = "slint-ui")]
pub mod desktop_ui;
#[cfg(feature = "slint-ui")]
pub mod slint_backend;
pub mod password_keyboard;

/// Hardware-facing drivers: panel, codec, and touch controller.
pub mod driver {
    pub mod audio;
    pub mod display;
    pub mod hosted_bus;
    pub mod hosted_net;
    pub mod hosted_v3;
    pub mod touch;
    pub mod usb_audio;
    pub mod usb_msc;
    pub mod sd_card;
    pub mod home_image_store;
}

/// Executor-facing long-lived application tasks.
pub mod task {
    pub mod wifi_ble;
    pub mod desktop_radio;
    pub mod home_web;
    pub mod weather;
    pub mod hosted;
    pub mod mdns;
    pub mod net;
}
