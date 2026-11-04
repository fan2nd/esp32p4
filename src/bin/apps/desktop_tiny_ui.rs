#![no_std]
#![no_main]

mod desktop_runtime;
const DISPLAY: desktop_runtime::Display = waveshare_p4_eth::desktop::display::run::<false>;
