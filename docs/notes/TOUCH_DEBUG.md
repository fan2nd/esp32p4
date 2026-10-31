# GT911 polling

`0x814E = 0x80` means a ready report with zero contacts. The host must
write zero to `0x814E` even for this release/empty report. Returning early
without acknowledging it can prevent subsequent reports from arriving.
Only read coordinates when bit 7 is set; a nonzero count alone is not enough.

The driver tries 7-bit address `0x14` first, then `0x5D`, and checks for
product ID `911`. Probing an address does not change the controller address.
INT is not required for host polling; reset/address selection and wake-up
requirements are separate from reading reports. No unexposed GPIO is driven.

Hardware regression check: `cargo run --release --bin verify_touch`.
Start untouched, then press, release, and press again. Both presses must
produce coordinates, including after an initial `0x80` report. An
acknowledgement failure is logged explicitly. Compilation alone cannot
verify physical touch detection.

Multi-touch: the GT911 driver reads all reported contacts (up to 5) in one
burst from `0x814F`, with 8 bytes per contact. Each entry contains track ID,
little-endian X and Y, size and a reserved byte. The verification bin prints
the count and every ID/raw coordinate pair; count zero reports release.
Polling runs every 20 ms. Hold two or more fingers, move them independently,
then release all and repeat. Check distinct IDs and coordinates for every
contact, followed by `contacts=0`. The screen's advertised ten contacts do
not establish ten-contact support for a controller identified as GT911.
