# 文档索引

## 使用指南 `guides/`

- [纯 Rust UI demo](guides/RUST_UI_DEMO.md)
- [Slint demo](guides/SLINT_DEMO.md)
- [完整桌面](guides/DESKTOP.md)
- [宏键盘 / VIA](guides/KEYBOARD.md)
- [USB 音频](guides/USB_AUDIO.md)
- [双核显示](guides/DISPLAY_CORE1.md)
- [Wi-Fi / BLE](guides/WIFI_BLE.md)
- [静态宠物](guides/PET.md)
- [Hosted 通信验证](guides/HOSTED_VERIFY.md)
- [音频验证](guides/AUDIO_VERIFY.md)

`verify_display` 与 `verify_touch` 的操作说明另见硬件初始化和排障笔记。
`verify_usb` 的说明在 USB 音频指南中。

## 硬件 `hardware/`

- [ETH 引脚](hardware/pinout.md)
- [NANO-WIFI6-DB 引脚](hardware/pinout-nano-wifi6-db.md)
- [屏幕初始化](hardware/DISPLAY_INIT.md)
- [官方原理图、位图与芯片手册](reference/)

## 设计与排障 `notes/`

这些文档保留技术设计、操作方法与简洁验证边界，不收录运行历史或原始日志；不代表已在当前硬件复测。

- [桌面架构](notes/DESKTOP_ARCHITECTURE.md)
- [UI 框架选型](notes/UI_FRAMEWORK_OPTIONS.md)
- [CPU 使用率方案](notes/CPU_USAGE.md)
- [显示调试](notes/DISPLAY_DEBUG.md)
- [触摸调试](notes/TOUCH_DEBUG.md)
- [CH343 / 复位对比](notes/CH343_COMPARE.md)
- [显示与无线共存](notes/COEXIST_VERIFY.md)
- [SDIO/MIPI 缓存修复](notes/SDIO_MIPI_CACHE_FIX.md)
- [音频 DMA 缓存修复](notes/AUDIO_DMA_CACHE_FIX.md)

## 工程维护

- [仓库结构、迁移与验证](REPOSITORY_LAYOUT.md)
- [主机测试](../tests/README.md)
- [资源生成](../tools/README.md)
