#![no_std]
#![no_main]

//! MicroTS-generated PocketJS controls with the shared composite services.
mod desktop_renderer {
    pub use waveshare_p4_eth::tiny_desktop_ui::run_pocket as run;
}
include!("desktop/runtime.rs");
