#![no_std]

pub mod board;
pub mod keyboard;

/// Hardware-facing drivers: panel, codec, and touch controller.
pub mod driver {
    pub mod audio;
    pub mod display;
    pub mod hosted_bus;
    pub mod hosted_v3;
    pub mod touch;
    pub mod usb_audio;
}

/// Executor-facing long-lived application tasks.
pub mod task {
    pub mod hosted;
    pub mod mdns;
    pub mod net;
}
