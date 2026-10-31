//! Minimal DSI bring-up for the Waveshare two-lane panel connector.
//!
//! The connector does not expose a panel reset GPIO, so the panel is woken with
//! standard DCS commands after the D-PHY is ready. This is the 720x1280
//! Waveshare 5-DSI-TOUCH-A panel.

use esp_hal::{
    Blocking,
    delay::Delay,
    i2c::master::I2c,
    mipi_dsi::{
        Config, DataLanes, MipiDsi,
        dpi::{ColorFormat, DpiClockSource, DpiConfig, FrameTiming},
    },
    peripherals::{MIPI_DSI, VDMA_CH0},
};

pub const WIDTH: u16 = 720;
pub const HEIGHT: u16 = 1280;
pub const FRAMEBUFFER_SIZE: usize = WIDTH as usize * HEIGHT as usize * 3;

/// Known working RGB888 scanout; individual applications may change input format/timing.
pub const DEFAULT_DPI_CONFIG: DpiConfig = DpiConfig {
    virtual_channel: 0,
    pixel_clock_mhz: 58.0,
    // Waveshare's IDF default source is PLL_F240M.  With the
    // 160 MHz source esp-hal can only realize 53.33 MHz for the
    // requested 58 MHz clock, which leaves the panel outside the
    // example's timing margin and can show as light flicker.
    dpi_clk_src: DpiClockSource::PllF240m,
    in_color_format: ColorFormat::Rgb888,
    out_color_format: ColorFormat::Rgb888,
    timing: FrameTiming {
        h_active: WIDTH as u32,
        hsw: 20,
        hbp: 20,
        hfp: 52,
        v_active: HEIGHT as u32,
        vsw: 4,
        vbp: 10,
        vfp: 24,
    },
};

const HX8394_VENDOR_INIT: &[(u8, &[u8])] = &[
    // SETEXTC: unlock HX8394 vendor registers.
    (0xB9, &[0xFF, 0x83, 0x94]),
    // SETPOWER (bank 0): panel power parameters; keep panel-specific values.
    (
        0xB1,
        &[0x48, 0x0A, 0x6A, 0x09, 0x33, 0x54, 0x71, 0x71, 0x2E, 0x45],
    ),
    // SETMIPI: full panel-side MIPI setup, with two data lanes (0x61).
    (0xBA, &[0x61, 0x03, 0x68, 0x6B, 0xB2, 0xC0]),
    // SETDISP: panel display control, separate from host DPI porch timing.
    (0xB2, &[0x00, 0x80, 0x64, 0x0C, 0x06, 0x2F]),
    // SETCYC: panel driving-cycle parameters.
    (
        0xB4,
        &[
            0x1C, 0x78, 0x1C, 0x78, 0x1C, 0x78, 0x01, 0x0C, 0x86, 0x75, 0x00, 0x3F, 0x1C, 0x78,
            0x1C, 0x78, 0x1C, 0x78, 0x01, 0x0C, 0x86,
        ],
    ),
    // SETGIP0: gate-in-panel scan control.
    (
        0xD3,
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x32, 0x10, 0x05, 0x00, 0x05, 0x32,
            0x13, 0xC1, 0x00, 0x01, 0x32, 0x10, 0x08, 0x00, 0x00, 0x37, 0x03, 0x07, 0x07, 0x37,
            0x05, 0x05, 0x37, 0x0C, 0x40,
        ],
    ),
    // SETGIP1: panel-specific gate scan/output mapping.
    (
        0xD5,
        &[
            0x18, 0x18, 0x18, 0x18, 0x22, 0x23, 0x20, 0x21, 0x04, 0x05, 0x06, 0x07, 0x00, 0x01,
            0x02, 0x03, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18,
            0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x19, 0x19,
            0x19, 0x19,
        ],
    ),
    // SETGIP2: companion gate scan/output mapping.
    (
        0xD6,
        &[
            0x18, 0x18, 0x19, 0x19, 0x21, 0x20, 0x23, 0x22, 0x03, 0x02, 0x01, 0x00, 0x07, 0x06,
            0x05, 0x04, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18,
            0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x19, 0x19, 0x18, 0x18,
        ],
    ),
    // SETGAMMA: grayscale drive curves, not backlight brightness.
    (
        0xE0,
        &[
            0x07, 0x08, 0x09, 0x0D, 0x10, 0x14, 0x16, 0x13, 0x24, 0x36, 0x48, 0x4A, 0x58, 0x6F,
            0x76, 0x80, 0x97, 0xA5, 0xA8, 0xB5, 0xC6, 0x62, 0x63, 0x68, 0x6F, 0x72, 0x78, 0x7F,
            0x7F, 0x00, 0x02, 0x08, 0x0D, 0x0C, 0x0E, 0x0F, 0x10, 0x24, 0x36, 0x48, 0x4A, 0x58,
            0x6F, 0x78, 0x82, 0x99, 0xA4, 0xA0, 0xB1, 0xC0, 0x5E, 0x5E, 0x64, 0x6B, 0x6C, 0x73,
            0x7F, 0x7F,
        ],
    ),
    // SETPANEL: panel characteristics; individual bits not verified here.
    (0xCC, &[0x0B]),
    // Vendor C0: exact meaning unverified (Linux HX8394 calls it UNKNOWN1).
    (0xC0, &[0x1F, 0x73]),
    // SETVCOM: common-electrode voltage parameters, not literal voltages.
    (0xB6, &[0x6B, 0x6B]),
    // Vendor D4: exact meaning unverified (Linux HX8394 calls it UNKNOWN3).
    (0xD4, &[0x02]),
    // SETREGBANK: select bank 1 before writing its B1 register.
    (0xBD, &[0x01]),
    // Bank-1 B1: distinct from bank-0 SETPOWER; field meaning unverified.
    (0xB1, &[0x00]),
    // SETREGBANK: restore bank 0 for the following commands.
    (0xBD, &[0x00]),
    // Vendor BF: exact meaning unverified (Linux HX8394 calls it UNKNOWN5).
    (0xBF, &[0x40, 0x81, 0x50, 0x00, 0x1A, 0xFC, 0x01]),
    // COLMOD: Waveshare overrides the initial 0x77 with 0x50. Its exact
    // MIPI-mode interpretation is unverified; this is not a DMA stride setting.
    (0x3A, &[0x50]),
    // SETDISP: repeat the first six values and supply six additional values.
    // Order differs from Waveshare 2.1.0, which places this AFTER the second
    // 0x11 + 200 ms delay. Preserve the working sequence pending a separate test.
    (
        0xB2,
        &[
            0x00, 0x80, 0x64, 0x0C, 0x06, 0x2F, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x18,
        ],
    ),
];

/// Initialise the panel and start DMA with the requested framebuffer format/timing.
pub fn init(
    dsi: MIPI_DSI<'static>,
    vdma: VDMA_CH0<'static>,
    mut i2c: I2c<'static, Blocking>,
    framebuffer: &'static mut [u8],
    config: DpiConfig,
) -> (
    esp_hal::mipi_dsi::dpi::DsiDpi<'static>,
    I2c<'static, Blocking>,
) {
    crate::diagnostic!("display: power i2c");
    crate::diagnostic!("display: i2c peripheral ready");
    // Board controller at 0x45, not HX8394 DCS: reproduce Waveshare's
    // register 0x95 sequence. Individual control bits are not verified here.
    i2c.write(0x45, &[0x95, 0x11])
        .expect("panel I2C write failed");
    i2c.write(0x45, &[0x95, 0x17])
        .expect("panel I2C write failed");
    // Toggle board register 0x96 from 0x00 to 0xFF with the vendor delays;
    // its electrical function requires the board controller register map.
    i2c.write(0x45, &[0x96, 0x00])
        .expect("panel I2C write failed");
    Delay::new().delay_millis(100);
    i2c.write(0x45, &[0x96, 0xFF])
        .expect("panel I2C write failed");
    Delay::new().delay_millis(1000);
    crate::diagnostic!("display: i2c ready, init dphy");

    let mut bus = MipiDsi::new(
        dsi,
        vdma,
        Config::default()
            .with_num_data_lanes(DataLanes::_2)
            .with_lane_bit_rate_mbps(700.0),
    )
    .expect("MIPI DSI init failed");
    crate::diagnostic!("display: dphy ready");

    let delay = Delay::new();
    let mut dbi = bus.dbi(0);

    // SLPOUT: wake the panel and wait for its internal circuits to stabilize.
    dbi.write_cmd(0x11, &[]).expect("DSI sleep-out failed");
    delay.delay_millis(120);
    // MADCTL: set BGR order (bit 3) to correct the observed red/blue swap.
    // Keep mirror and axis-exchange bits clear; verify with labeled RGB patches.
    dbi.write_cmd(0x36, &[0x08])
        .expect("DSI orientation setup failed");
    // COLMOD: initial RGB888 format; the vendor table later overwrites it.
    dbi.write_cmd(0x3A, &[0x77])
        .expect("DSI RGB888 setup failed");
    // SETMIPI: initial two-lane panel setup, later expanded by the vendor table.
    dbi.write_cmd(0xBA, &[0x61]).expect("DSI lane setup failed");
    for &(cmd, params) in HX8394_VENDOR_INIT {
        crate::diagnostic!("display: cmd {:02x}", cmd);
        dbi.write_cmd(cmd, params).expect("HX8394 init failed");
        // Sleep/display delays follow Waveshare; ordinary commands receive
        // an extra 1 ms here (their vendor-table delay is zero).
        delay.delay_millis(match cmd {
            0x11 => 200,
            0x29 => 80,
            _ => 1,
        });
    }
    // SLPOUT: second wake command from the vendor tail, followed by 200 ms.
    dbi.write_cmd(0x11, &[])
        .expect("DSI vendor sleep-out failed");
    delay.delay_millis(200);
    // DISPON: enable panel display; backlight and host DPI start are separate.
    dbi.write_cmd(0x29, &[]).expect("DSI display-on failed");
    delay.delay_millis(80);
    // Panel commands are complete; release the DBI borrow of the bus.
    drop(dbi);
    crate::diagnostic!("display: panel ready, fill");

    // The DSI DMA continuously scans one frame; a second equally sized frame
    // enables back-buffer drawing without changing the panel's wire format.
    let bytes_per_pixel = match config.in_color_format {
        ColorFormat::Rgb888 => 3,
        ColorFormat::Rgb565 => 2,
    };
    let frame_size = WIDTH as usize * HEIGHT as usize * bytes_per_pixel;
    assert!(framebuffer.len() == frame_size || framebuffer.len() == frame_size * 2);
    // Panel remains configured for RGB888; the bridge expands RGB565 input.
    assert_eq!(config.out_color_format, ColorFormat::Rgb888);
    assert_eq!(
        framebuffer.as_ptr() as usize & 63,
        0,
        "PSRAM framebuffer is not aligned"
    );
    let dpi = if framebuffer.len() == frame_size {
        bus.dpi(config, &[framebuffer])
    } else {
        let (front, back) = framebuffer.split_at_mut(frame_size);
        bus.dpi(config, &[front, back])
    }
    .expect("MIPI DPI init failed");
    crate::diagnostic!("display: dpi streaming");
    (dpi, i2c)
}

/// Set the Waveshare board backlight (I2C 0x45 register 0x96).
/// This UI uses 5..=100 percent to keep the display recoverable by touch.
pub fn set_brightness(i2c: &mut I2c<'_, Blocking>, percent: u8) -> bool {
    let value = (u16::from(percent.clamp(5, 100)) * 255 / 100) as u8;
    i2c.write(0x45, &[0x96, value]).is_ok()
}
