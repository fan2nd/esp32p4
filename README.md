# Waveshare ESP32-P4 ETH mDNS + MIPI DSI

Waveshare ESP32-P4-ETH mDNS demo, currently configured for minimum chip revision 3.1.

The display path uses stable `esp-hal 1.2.0`'s MIPI-DSI DBI and DPI drivers. It applies
the Waveshare HX8394 power and vendor sequence for the 5-DSI-TOUCH-A panel
(720×1280, two lanes at 700 Mbps per lane), then streams an RGB888 PSRAM
framebuffer through DPI/VDMA. The main binary displays black; `verify_display`
draws a coordinate guide. See [the initialization summary](doc/DISPLAY_INIT.md)
for the sequence, delays, panel commands, and clock/timing parameters.

## Notes

- Hardware pinouts: [ESP32-P4-ETH (original board)](doc/pinout.md) and
  [ESP32-P4-NANO-WIFI6-DB (replacement board)](doc/pinout-nano-wifi6-db.md).

- The build minimum is chip revision `3.1` via `ESP_HAL_CONFIG_MIN_CHIP_REVISION=301`.
- Ethernet uses the board's RMII PHY path through `esp-p4-eth`.
- The Cargo runner uses `espflash flash --monitor --chip esp32p4`.
  The replacement v3.1 board's post-flash reset issue is documented in
  `doc/DISPLAY_DEBUG.md`.
- The demo advertises `waveshare-p4-eth.local` and a small `_http._tcp.local` service.

## Flash

The standalone `verify_hosted` binary runs the on-board C5 SDIO communication check
in Rust, without initializing display, audio or Ethernet.
Use the matching Packet + checksum coprocessor firmware and follow
[the Hosted verification guide](doc/HOSTED_VERIFY.md).

```bash
cargo run
```

Hardware verification binaries are discovered from `src/bin`:

```powershell
cargo run --release --bin verify_display
cargo run --release --bin verify_touch
cargo run --release --bin verify_audio
cargo run --release --bin pet
```

`verify_touch` polls GT911 at `0x5D` and `0x14`, reports probe failures and
prints all four coordinate rotations. Native coordinates are `(0,0)` at the
top-left, x increasing right and y increasing down. `verify_audio` initializes
ES8311, records 3 seconds through I2S0 DMA, then plays the buffer through the
codec.

`pet` displays a static whale-girl companion and greeting. Its screen, memory
requirements, and artwork are described in
[the pet guide](doc/PET.md).

Adjust the static IPv4 settings in `src/board.rs` if your LAN does not use `192.168.1.0/24`.

## USB microphone + speaker (P4+C5 board)

`cargo run --release --bin usb_audio` builds a standalone UAC1 device using
the NANO-WIFI6-DB USB-A HS port and onboard ES8311 (mono PCM16, 16 kHz).
Read [USB audio setup and validation](doc/USB_AUDIO.md) before connecting: the
USB-A port supplies 5 V and requires host VBUS isolation for this experiment.
Do not connect a normal powered A-to-A cable. Hardware validation is pending.
