# 双核桌面固件

`desktop` 是 ESP32-P4-NANO-WIFI6-DB 的复合应用。应用与板级驱动使用 Rust；
C5 使用已有 ESP-Hosted MCU 3.0.10 SDIO 固件，本项目不编译 C 固件。
独立 `keyboard`、`usb_audio`、`verify_hosted` 例程继续保留。

## 功能

- Slint 页面位于 `ui/desktop.slint`，包含插画主页、时间与连接状态、设置页。
- Wi-Fi 扫描最多 64 条结果，按信号强度排序并按 SSID 去重；支持触摸滚动、
  屏幕密码键盘、连接与断开。凭据仅用于本次 RAM 连接，不写 Flash，未实现自动重连。
- DHCP 成功后尝试 SNTP 校时，默认 UTC+8；未校时时显示占位值。
- 8 键单层宏键盘使用 VIA 第 0 层，兼容已有四层存储格式，但屏幕不执行切层。
- 复合 USB 提供键盘、Consumer Control、VIA Raw HID 与 UAC1 音频。
- 蓝牙状态仅表示控制器初始化结果，不提供配对、蓝牙键盘或音频服务。

USB 状态不能可靠区分物理拔线与主机挂起。Wi-Fi 列表未提供信道信息，
不能凭列表判断频段。SSID 字体覆盖 ASCII 与基本 CJK，非完整 Unicode。

## 板级配置

| 核心 | 职责 |
| --- | --- |
| Core 0 | USB、ES8311/I2S DMA、C5 SDIO、网络及蓝牙 HCI |
| Core 1 | MIPI/VDMA、GT911、Slint 页面与交互 |

ES8311 配置后，I2C1 所有权移交 Core 1，两个核心不并发使用同一 I2C。
跨核状态与命令采用临界区保护，锁内不绘制、不访问 I2C、不 await。

- 显示：720×1280 RGB565 双缓冲，DSI RGB888，约 31 Hz 面板扫描。
- UI：360×640 逻辑画布，2 倍输出；SRAM 行缓冲，PSRAM 帧缓冲与独立 4 MiB Slint 堆。
- Core 1 栈：64 KiB；USB/I2S 缓冲在内部 SRAM。
- 显示提交保留缓存互斥与 VSYNC 等待，绘制耗时不是端到端交互延迟。
- USB：HS PHY 的全速模式，实验 VID/PID `CAFE:4014`。
- 音频：mono PCM16、16 kHz、DAC 0 dB。
- VIA 定义：`assets/p4-desktop-via.json`；分区：`config/partitions-keyboard.csv`。

VIA 首次使用需在 Design 页载入本设备 JSON，不能使用独立键盘的定义替代。
定义文件不包含设备改键备份；浏览器 USB/HID 授权与布局加载是独立步骤。
配置变更空闲后保存，播放或录音期间延迟；写 Flash 时让另一核停驻内部 RAM。
双核写 Flash 对实时显示与音频的影响仍需验证。

## 构建与使用

将 `COMx` 替换为板卡实际串口。

```powershell
cargo build --release --bin desktop --features slint-ui
espflash flash --port COMx --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/desktop
python 3rd/keyboard-host-tests/via_probe.py --pid 0x4014 --roundtrip
python 3rd/usb-audio-host-tests/playback.py --device "P4 Desktop"
cargo test --manifest-path 3rd/desktop-protocol-tests/Cargo.toml --target x86_64-pc-windows-msvc
```

## 验证范围与限制

本次框架拆分及依赖升级通过软件构建和主机测试，没有新增板上验证。
既有单功能验证不能代替升级后的复合负载验证。
长期音频稳定性、偶发播放停顿原因、麦克风声学响应、5 GHz 实际连接、
显示与无线/音频长期共存仍需单独确认。不宣称全功能或长期零断音。

## 资源与设计

- [双核架构](../notes/DESKTOP_ARCHITECTURE.md)
- [缓存互斥设计](../notes/SDIO_MIPI_CACHE_FIX.md)
- [音频 DMA 布局与限制](../notes/AUDIO_DMA_CACHE_FIX.md)
- [CPU 统计设计](../notes/CPU_USAGE.md)

插画：`assets/whale-girl-wave-large.rgb`。SSID 位图使用 Noto Sans SC，
许可见 `assets/CJK-FONT-LICENSE.txt`（SIL OFL）。
`tools/build_cjk_font.py <NotoSansSC[wght].ttf>` 可生成 `assets/cjk16.bin`，需 Pillow。
