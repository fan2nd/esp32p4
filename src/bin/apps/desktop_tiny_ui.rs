#![no_std]
#![no_main]

//! Composite USB audio / clock / Wi-Fi / keyboard desktop using the Rust UI framework.
use waveshare_p4_eth::tiny_desktop_ui as desktop_renderer;
include!("desktop/runtime.rs");
