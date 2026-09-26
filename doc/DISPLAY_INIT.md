# HX8394 initialization command reference

This describes the current src/display.rs, not a proposed new sequence.
Command names are cross-checked against the Waveshare esp_lcd_hx8394 2.1.0
driver and Linux drivers/gpu/drm/panel/panel-himax-hx8394.c:
https://raw.githubusercontent.com/torvalds/linux/master/drivers/gpu/drm/panel/panel-himax-hx8394.c
No complete panel-specific register bit definitions were verified.

## Board control over I2C

The four writes target address 0x45, not HX8394 DCS registers:

| Write | Verified behavior / limit of knowledge |
| --- | --- |
| 95 11 | First board-control configuration write from Waveshare. Individual output bits have not been verified against a schematic. |
| 95 17 | Second board-control configuration write. Do not infer a particular reset or supply signal without the board register map. |
| 96 00 | Write zero to board-control register 96, followed by 100 ms delay. Whether this is brightness or another control requires the board register map. |
| 96 FF | Write FF to the same register, followed by 1000 ms delay. Do not infer its electrical function from the values alone. |

## DCS and vendor commands in current execution order

| Command | Meaning |
| --- | --- |
| 11, wait 120 ms | Exit sleep; allow panel internal circuitry to stabilize. Does not itself stream pixels. |
| 36 00 | MADCTL/address mode: clear the standard mirror, row/column exchange and BGR bits. Does not define a universal physical landscape/portrait orientation. |
| 3A 77 | Set pixel format. Standard encoding selects 24-bit RGB888 for both interface format fields; later overwritten by 3A 50. |
| BA 61 | Initial panel MIPI configuration. Waveshare selects this byte for two lanes. Host lane configuration is separate. |
| B9 FF 83 94 | SETEXTC: unlock extended HX8394 registers. |
| B1, 10 bytes, bank 0 | SETPOWER: panel power-related parameters. Exact voltages cannot be decoded from this table alone. |
| BA, 6 bytes | SETMIPI: full panel-side MIPI interface settings, including the initial 61 byte. |
| B2, 6 bytes | SETDISP: panel display-control parameters. These are not the ESP32 DPI porch registers. |
| B4, 21 bytes | SETCYC: panel driving-cycle parameters. |
| D3, 33 bytes | SETGIP0: gate-in-panel scan-control settings. |
| D5, 44 bytes | SETGIP1: gate-in-panel configuration, including panel scan/output mapping. Exact assignments require panel documentation. |
| D6, 44 bytes | SETGIP2: companion gate-in-panel configuration. |
| E0, 58 bytes | SETGAMMA: grayscale/gamma drive curve settings. Not framebuffer pixel data or a backlight brightness setting. |
| CC 0B | SETPANEL: panel characteristic configuration. Individual bit meanings not verified here. |
| C0 1F 73 | Vendor setting; Linux labels C0 UNKNOWN1. Preserve supplied bytes pending authoritative bit definitions. |
| B6 6B 6B | SETVCOM: common-electrode voltage parameters. Exact voltages require the register transfer function. |
| D4 02 | Vendor setting; Linux labels D4 UNKNOWN3. |
| BD 01 | SETREGBANK: switch to register bank 1. |
| B1 00, bank 1 | Write B1 in bank 1; this does not simply replace bank-0 SETPOWER parameters. Exact bank-1 field not verified. |
| BD 00 | Return to register bank 0. |
| BF, 7 bytes | Vendor setting; Linux labels BF UNKNOWN5. |
| 3A 50 | Overwrite the earlier pixel-format byte. This is not the ordinary RGB888 77 encoding. Standard field interpretation has RGB/DPI field 5 and DBI field 0; actual meaning in this panel's MIPI mode needs HX8394 documentation. Do not infer the DMA buffer layout from this byte alone. |
| B2, 12 bytes | Second SETDISP write: repeat the first six values with six additional values. |
| 11, wait 200 ms | Second exit-sleep command, retained from Waveshare's vendor sequence. |
| 29, wait 80 ms | Display on. Separate from backlight control and from starting host DPI streaming. |

Then drop(dbi) releases the Rust bus borrow; it is not a panel command.
The framebuffer is filled and bus.dpi starts continuous pixel transmission.

## Known sequence discrepancy

Current Rust tail: 3A 50 -> B2 (12 bytes) -> 11 -> wait 200 ms -> 29.
Waveshare 2.1.0 tail: 3A 50 -> 11 -> wait 200 ms -> B2 (12 bytes) -> 29.
These orders are not equivalent by inspection. This reference records the
difference; no firmware changes were made as part of this explanation.
Rust also adds 1 ms after ordinary vendor commands whose table delay is 0.
