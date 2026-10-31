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

These icons replace the tiny-ui desktop's manually drawn line approximations.
The original Slint assets and program remain available for comparison.
