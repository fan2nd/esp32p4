# Waveshare ESP32-P4 ETH mDNS + MIPI DSI

Waveshare ESP32-P4-ETH mDNS demo for ESP32-P4 chip revision 3.2.

The display path uses stable `esp-hal 1.2.0`'s MIPI-DSI DBI driver. It applies
the Waveshare HX8394 power and vendor sequence for the 5-DSI-TOUCH-A panel
(720×1280, two lanes), then fills its RGB565 address space green.

## Notes

- The build is pinned for chip revision `3.2` via `ESP_HAL_CONFIG_MIN_CHIP_REVISION=302`.
- Ethernet uses the board's RMII PHY path through `esp-p4-eth`.
- The image is flashed with the espflash runner configured for chip revision 3.2.
- The demo advertises `waveshare-p4-eth.local` and a small `_http._tcp.local` service.

## Flash

```bash
cargo run
```

Hardware verification binaries are discovered from `src/bin`:

```powershell
cargo run --release --bin verify_display
cargo run --release --bin verify_touch
cargo run --release --bin verify_audio
```

`verify_touch` polls GT911 at `0x5D` and `0x14`, reports probe failures and
prints all four coordinate rotations. Native coordinates are `(0,0)` at the
top-left, x increasing right and y increasing down. `verify_audio` initializes
ES8311, records 3 seconds through I2S0 DMA, then plays the buffer through the
codec.

Adjust the static IPv4 settings in `src/board.rs` if your LAN does not use `192.168.1.0/24`.
