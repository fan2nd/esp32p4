# Local ESP32-P4 cache synchronization fix

Base: esp-hal 1.2.2 registry package; `.cargo_vcs_info.json` records upstream
commit `915465df59aee91b627fe538f0b442e56c7f4fc9`. Original licenses are retained.
Cargo selects this copy via `[patch.crates-io]`, so the fix is reproducible and
does not depend on modifying a developer's Cargo registry cache.

## Changes

- `src/dma/mod.rs`: P4 DMA descriptors each occupy one 64-byte internal-RAM
  cache line. This avoids CPU recycling writeback overwriting a neighbouring
  descriptor concurrently updated by DMA. Compile-time assertions preserve
  hardware word offsets 0/4/8. See `docs/notes/AUDIO_DMA_CACHE_FIX.md` for validation.

- `src/soc/esp32p4/mod.rs`: one `esp_sync::RawMutex` serializes all three address
  cache-maintenance entry points. The critical region covers the entire ROM
  call; it protects against both the other core and local interrupts.
- `src/sdmmc/mod.rs`: preserves optional error-status diagnostics without changing
  the success path or the application's ownership of the peripheral.

## Why

Core 1 VDMA descriptor writeback and core 0 SDIO cache maintenance can overlap.
The original P4 wrappers had no shared lock, unlike ESP-IDF esp_cache_msync's
critical sections. Historical limited board validation supports retaining the
patch, but does not certify this release's full composite workload.
See `docs/notes/SDIO_MIPI_CACHE_FIX.md` for design and primary-source references.

This deliberately preserves cache address selection, alignment and operation
order. Address writebacks are split into at most 4096-byte ROM calls, releasing
the lock between calls so a full-frame flush does not monopolize core 0's
interrupt-disabled critical section. Each individual ROM call remains protected.
Full desktop audio latency and Wi-Fi association are still under validation.

## Release refresh (2026-10-31)

The crates.io API reports `1.2.2` as the latest stable, non-yanked esp-hal
release (published 2026-09-18). The previous base was `1.2.0`. Source archives
were downloaded from `https://static.crates.io/crates/esp-hal/` and their
SHA-256 checksums matched the crates.io version metadata. The 1.2.2 archive
checksum is `c9e3dfd511fed638fe41fdb4595f8c7f0cbb488d0f0cf2bbc063d76d805057cf`.
The three
locally modified Rust source files listed above are byte-identical between
upstream 1.2.0 and 1.2.2, so their existing patches are retained without
fuzzy application. All other upstream package files follow 1.2.2. The extra
license files and this patch record are retained.

`embassy-usb-synopsys-otg` 0.4.0 remains the latest stable published version
on the same check date; its local USB fixes remain required. This refresh
does not claim fresh hardware validation of cache/DMA/USB behavior.

Sources: https://crates.io/api/v1/crates/esp-hal and
https://crates.io/api/v1/crates/embassy-usb-synopsys-otg .

Software validation of the refresh: the existing main-tree firmware with only
the HAL base upgrade passed `cargo check --release --all-features --bins -j1`;
the relocated integration tree passed all 14 firmware release links with
`cargo build --release --locked --bins --features slint-ui -j1`. The latter
includes the independent UI framework work at the tested snapshot; final UI
changes require their own final integration check. No fresh board test was
performed for this refresh.
