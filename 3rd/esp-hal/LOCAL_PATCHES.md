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

## Non-blocking DPI VSYNC query (2026-11-08)

Add `DsiDpi::try_wait_for_vsync` to consume a pending event without spinning.
The display loop can continue touch/Flash-park processing while waiting for two
fresh frame boundaries before back-buffer reuse. Existing blocking/async APIs
are unchanged. See `docs/guides/PC_MONITOR.md` for validation status.

## DPI invalid-LLI recovery (2026-11-01)

`src/mipi_dsi/dpi.rs` now enables descriptor-invalid status/signal (bit 13),
re-arms and writes back the ring, then writes BLK_TFR_RESUMEREQ0=1 on that
specific event. Long interrupt masking during Flash erase can consume both
ring entries; changing VALID alone does not request another LLI fetch.
The ISR records a counter without logging. This does not reset all DMA channels
or enlarge the descriptor ring to hide missed servicing.

Reference: Espressif `dw_gdma_ll_channel_resume_multi_block_transfer` and
`DW_GDMA_LL_CHANNEL_EVENT_SHADOWREG_OR_LLI_INVALID_ERR` in
https://github.com/espressif/esp-idf/blob/master/components/esp_hal_dma/esp32p4/include/hal/dw_gdma_ll.h .
Firmware remains Rust. All release binaries built. A 160,572-byte upload produced six invalid-LLI
recoveries; the user confirmed that the whale and status UI were displayed again.
This verifies recovery in that workload, not absence of transient underflow or audio gaps.

## SDIO card interrupts (2026-11-07)

`src/sdmmc/mod.rs` configures the unused active-low `CARD_INT` input high,
card-detect low (present) and write-protect low by default. Explicit CD/WP
builders can still replace those defaults. `CARD_INT` is separate from DAT1;
it must not be left at the GPIO matrix reset value for in-band SDIO interrupts.

The data-pin builders remember DAT1's GPIO for both GPIO-matrix and IO_MUX
slots. The interrupt future registers its waker and unmasks the interrupt before
consuming the ISR latch and sampling DAT1. An already-low DAT1 completes the
wait immediately; otherwise the task sleeps until an interrupt wakes it. This
covers an interrupt asserted before the wait or while a previous wait was
cancelled, without a timer, repeated register reads or self-waking loop.
The ISR and cancellation latch semantics are unchanged.

References: ESP-IDF v5.5
[slot signal defaults](https://github.com/espressif/esp-idf/blob/v5.5/components/esp_driver_sdmmc/src/sdmmc_host.c#L791)
and [SDIO interrupt wait](https://github.com/espressif/esp-idf/blob/v5.5/components/esp_driver_sdmmc/src/sdmmc_host.c#L1153).
The Hosted adapter's 10 ms polling fallback is removed. The P4 `verify_hosted`
release build and default-feature `cargo check --release --locked --bins` passed.
Fresh hardware verification is pending a connected board; other chips have only
been reviewed at source level.

## Experimental ESP32-P4 PPA SRM (2026-11-10)

`src/ppa.rs` adapts upstream esp-hal PR #6476 (commit
`e045405e6f036a7d8b2ef22bc877457dc3e6eba2`, MIT OR Apache-2.0) to P4.
It uses P4 clock/reset registers, a minimal private DMA2D MMIO layer (omitted
by PAC 0.3), 16x16 macroblocks and the ESP-IDF DIG-734 tail-block workaround.
No upstream chip metadata is patched: the module is explicitly P4-only.
DMA ownership is exclusive, waits are bounded with fail-stop on an unsafe-to-release
DMA channel, and cache operations use the existing shared cache lock.
Destination addresses/lengths must be 128-byte aligned for internal RAM and PSRAM.
See `../../docs/guides/PPA.md` for scope, hardware results and remaining display artifacts.
