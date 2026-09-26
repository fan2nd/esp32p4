#![no_std]

pub mod board;

/// Hardware-facing drivers: panel, codec, and touch controller.
pub mod driver {
    pub mod audio;
    pub mod display;
    pub mod touch;
}

/// Executor-facing long-lived application tasks.
pub mod task {
    pub mod mdns;
    pub mod net;
}
