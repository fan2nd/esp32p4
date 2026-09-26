# Display bring-up findings

## Latest verification

- User reports HFP 52 displays correctly; current `src/display.rs` uses 52.
- Four-lane feasibility checked against ESP-IDF:
  `components/esp_hal_lcd/esp32p4/include/hal/mipi_dsi_ll.h` defines
  `MIPI_DSI_LL_MAX_DATA_LANES 2`. `esp_lcd_mipi_dsi_bus.c` rejects lane
  counts above that value with `ESP_ERR_INVALID_ARG`. esp-hal 1.2.0
  likewise exposes only one or two lanes. No four-lane firmware was flashed;
  forcing a register value would not provide four physical data lanes.

- Panel: Waveshare 5-DSI-TOUCH-A, HX8394, 720 x 1280.
- Keep two DSI data lanes at 700 Mbps per lane. Waveshare component
  `esp_lcd_hx8394` 2.1.0 specifies this configuration; esp-hal 1.2.0
  `DataLanes` exposes only `_1` and `_2`. Four lanes are not an available
  software setting for this setup.
- RGB888 framebuffer uses three bytes per pixel. ESP-IDF
  `esp_lcd_panel_dpi.c` allocates width * height * bits_per_pixel / 8;
  DMA2D support does not imply four-byte pixels.
- User confirmed flicker stopped with PLL_F240M. Spatial corruption remains:
  approximately 15 mm of a 110 mm long edge is correct, followed by an
  estimated one-pixel displacement every four lines. Photo orientation does
  not establish panel scan orientation.
- Requested DPI clock 58 MHz is realized as 60 MHz (240 / 4). Current HAL
  compensation makes bridge HTOTAL 828; host HLINE is 1207 byte clocks at
  87.5 MHz. These imply 13.8 us and 13.794286 us per line, respectively,
  a difference of about 0.343 pixel clocks per line. This is a hypothesis
  to test, not a confirmed cause. No porch or lane settings changed here.
- Nominal bandwidth alone cannot exclude transient FIFO underrun. No
  underrun measurement has yet been captured.

## Proposed A/B/A timing check (not yet flashed)

Keep panel commands, framebuffer, lane count/rate and DPI clock unchanged.
Compare requested HFP 40 with 52, then restore 40 to test reproducibility.
With HFP 52 the HAL computes bridge HTOTAL 840 and host HLINE 1225:
840 / 60 = 1225 / 87.5 = 14 us. Integer cross-check:
840 * 175 == 1225 * 120. Only HFP changes, but this also lengthens the
line and lowers refresh rate slightly, so improvement alone does not prove
clock mismatch rather than reduced throughput pressure. Record whether
the correct region extends across the screen and whether diagonal drift
disappears. Stop serial monitoring at `app: display ready`; visual outcome
comes from the user. If unchanged, inspect FIFO underrun/overflow and DMA
status instead of continuing blind porch changes. Register readback is
needed to confirm calculated timing matches the hardware.

Sources: local esp-hal 1.2.0 `src/mipi_dsi/{mod,dpi,vdma}.rs`, Waveshare
component `include/esp_lcd_hx8394.h`, and Espressif ESP-IDF master commit
`4d59230ddff16327812782151ef0afef202dc6d7`,
`components/esp_hal_lcd/mipi_dsi_hal.c` and
`components/esp_lcd/dsi/esp_lcd_panel_dpi.c`.
