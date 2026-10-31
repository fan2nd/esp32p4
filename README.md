# ESP32-P4 开发与验证

Waveshare ESP32-P4-ETH / ESP32-P4-NANO-WIFI6-DB 的 Rust 固件、独立演示和硬件验证。
当前最低芯片版本为 **3.1**，使用本地补丁版 `esp-hal 1.2.2`。

## 从哪里开始

| 目的 | 固件入口（`--bin`） | 说明 |
| --- | --- | --- |
| 纯 Rust 无 alloc 组件框架：滑条、按钮、下拉、列表 | `rust_ui_demo` | [运行与验证限制](docs/guides/RUST_UI_DEMO.md) |
| Slint UI 演示 | `slint_demo` + `--features slint-ui` | [Slint demo](docs/guides/SLINT_DEMO.md) |
| 完整桌面：触摸、USB、音频、无线 | `desktop` + `--features slint-ui` | [桌面固件](docs/guides/DESKTOP.md) |
| 触摸宏键盘 / VIA | `keyboard` | [键盘与分区表](docs/guides/KEYBOARD.md) |
| USB 麦克风 / 扬声器 | `usb_audio` | [接线安全与验证](docs/guides/USB_AUDIO.md) |
| 基础 Ethernet / mDNS / 黑屏显示 | `main`（默认） | [显示初始化](docs/hardware/DISPLAY_INIT.md) |
| 双核显示、Wi-Fi/BLE、静态宠物 | `display_core1`、`wifi_ble`、`pet` | [全部入口](docs/README.md) |
| 单项硬件排查 | `verify_display`、`verify_touch`、`verify_audio`、`verify_usb`、`verify_hosted` | [验证文档索引](docs/README.md) |

```sh
# 只编译，不连接设备
cargo build --release --locked --bin rust_ui_demo
# 烧录并监视（需要 espflash 与目标开发板）
cargo run --release --locked --bin rust_ui_demo
```

Cargo 默认目标是 `riscv32imafc-unknown-none-elf`。主机测试和电脑预览必须显式指定主机 target，见 [测试说明](tests/README.md) 和 [UI 主机预览](3rd/ui-demo/README.md)。

## 目录

- `src/bin/apps/`：完整应用入口。
- `src/bin/demos/`：聚焦单一能力的演示入口。
- `src/bin/verify/`：硬件验证入口。
- `src/driver/`、`src/task/`：共享驱动与异步任务；`src/*.rs`：现有公共模块与板级配置。
- [3rd/tiny-ui](3rd/tiny-ui/README.md)：可复用的无 alloc UI 框架；[3rd/ui-demo](3rd/ui-demo/README.md)：演示应用及主机预览。
- `ui/`、`assets/`：Slint 描述和图像/字体资源。
- `docs/`：使用指南、硬件说明、历史排障和官方资料，[从索引进入](docs/README.md)。
- `3rd/*-tests/`：独立主机测试包及实机辅助脚本；`tests/`：测试导航；`tools/`：资源生成工具。
- `config/`：烧录配置；`3rd/esp-hal/`、`3rd/embassy-usb-synopsys-otg/`：必须保留的第三方源码及本地补丁。

所有原有 `--bin` 名称保留。第三方 HAL 已升级至 1.2.2，并保留 P4 缓存/DMA 本地补丁；功能未删减。迁移路径和验证记录见 [仓库结构](docs/REPOSITORY_LAYOUT.md)。

## 使用前注意

- 两块板的引脚不同：[ETH](docs/hardware/pinout.md)、[NANO-WIFI6-DB](docs/hardware/pinout-nano-wifi6-db.md)。网络配置位于 `src/board.rs`。
- USB-A 音频实验端口供电，**不要使用普通带电 A-to-A 线直接连接主机**。先阅读 [USB 音频安全说明](docs/guides/USB_AUDIO.md)。
- SDIO/MIPI 共存依赖本地缓存维护补丁，不能直接删除 `3rd/esp-hal/`。见 [缓存修复](docs/notes/SDIO_MIPI_CACHE_FIX.md)。
- 编译通过不代表真机验证通过，各功能的实测状态与限制以对应指南为准。
