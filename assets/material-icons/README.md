# Desktop status icons

Source: Google Material Design Icons, Apache License 2.0 (see LICENSE).
Pinned upstream commit: `10ecc14790612ca8fd164e3f4ffe9c16fc50d5b7`.

Original SVG paths under https://github.com/google/material-design-icons/tree/10ecc14790612ca8fd164e3f4ffe9c16fc50d5b7/src :

- `notification/wifi/materialicons/24px.svg`
- `device/bluetooth/materialicons/24px.svg`
- `device/usb/materialicons/24px.svg`

The SVG files are unmodified upstream artwork. The `.a8` files are derivatives:
30×30 row-major alpha coverage (900 bytes each), rendered at 120×120 with CairoSVG
and downsampled with Pillow LANCZOS. Runtime tinting uses the desktop's active and
inactive colors. No SVG parser, font dependency or heap allocation is required on MCU.

All desktop backends share this artwork. Slint references the SVG files directly;
Tiny UI and PocketJS share the pre-rendered A8 coverage masks through the common
page overlay. SVG is the source and A8 is its derived MCU resource, not a second
icon design. The old hand-drawn status-*.svg files have been removed.

Active tint is #267b9a; inactive tint is #b8cdd5. All three icons occupy 30×30
logical pixels. Keep SVG and A8 updates paired when changing artwork.
