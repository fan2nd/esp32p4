# Local ESP32-P4 cache synchronization fix

Base: esp-hal 1.2.0 registry package; `.cargo_vcs_info.json` records upstream
commit `dbd951f78c1276461ea3050c2ba71f552a3508ef`. Original licenses are retained.
Cargo selects this copy via `[patch.crates-io]`, so the fix is reproducible and
does not depend on modifying a developer's Cargo registry cache.

## Changes

- `src/dma/mod.rs`: P4 DMA descriptors each occupy one 64-byte internal-RAM
  cache line. This avoids CPU recycling writeback overwriting a neighbouring
  descriptor concurrently updated by DMA. Compile-time assertions preserve
  hardware word offsets 0/4/8. See `doc/AUDIO_DMA_CACHE_FIX.md` for validation.

- `src/soc/esp32p4/mod.rs`: one `esp_sync::RawMutex` serializes all three address
  cache-maintenance entry points. The critical region covers the entire ROM
  call; it protects against both the other core and local interrupts.
- `src/sdmmc/mod.rs`: report raw MINTSTS/IDSTS on FBE/DU errors before clearing
  the hardware status. Uses the existing optional HAL logger. No success-path
  UART output is added; the application routes logs through its bounded queue.

## Why

Core 1 VDMA descriptor writeback and core 0 SDIO cache maintenance can overlap.
The original P4 wrappers had no shared lock, unlike ESP-IDF esp_cache_msync's
critical sections. On-board A/B testing reproduced SDIO DU (`IDSTS=0x2210`)
without this lock, removed the fault with it, and reproduced the fault again
after removing it. Both display and radio remain enabled with the fix.

See `doc/COEXIST_VERIFY.md` and `doc/SDIO_MIPI_CACHE_FIX.md` in the repository
for hardware evidence and primary-source references.

This deliberately preserves cache address selection, alignment and operation
order. Address writebacks are split into at most 4096-byte ROM calls, releasing
the lock between calls so a full-frame flush does not monopolize core 0's
interrupt-disabled critical section. Each individual ROM call remains protected.
Full desktop audio latency and Wi-Fi association are still under validation.
