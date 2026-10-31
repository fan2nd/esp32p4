# 纯 Rust 组件桌面

`desktop_tiny_ui` 是完整桌面程序的第二个 UI 后端，参考的是
`ui/desktop.slint`，不是滑条/按钮展示页 `ui/demo.slint`。
原 Slint `desktop` bin 保留，二者共用 Core 0 的 USB、音频、无线和存储实现。

## 构建与运行

```sh
# 不需要 slint-ui feature
cargo build --release --locked --bin desktop_tiny_ui
cargo run --release --locked --bin desktop_tiny_ui

# 原 Slint 版本仍可独立构建
cargo build --release --locked --bin desktop --features slint-ui
```

使用 ESP32-P4-NANO-WIFI6-DB 开发板，沿用现有屏幕、GT911、ES8311、C5 SDIO
与 USB 接线。VIA 持久化使用 `config/partitions-keyboard.csv`；首次烧录需按
[桌面指南](DESKTOP.md) 的分区表流程操作。
USB-A 音频实验端口供电，连接主机前必须阅读 [USB 音频安全说明](USB_AUDIO.md)。

## 页面与功能

- **主页**：UTC+8 时钟、Wi-Fi/蓝牙/USB 状态、IP、原有插画和设置/键盘入口。
  未完成网络校时时显示占位时间。
- **设置页**：扫描、可滚动的 Wi-Fi 列表、SSID/RSSI/安全性、连接结果与断开。
  状态/时钟更新不应重置列表滚动位置。
- **密码页**：复用原 PC 键盘布局、Shift/Caps、退格、清空、空格和 Enter；
  密码只在 RAM 中使用，退出页面或成功交付连接命令后清理输入。
- **USB 宏键盘页**：8 键多点触摸和 VIA 改键，保留键盘/Consumer Control 报告与宏播放。
  屏幕使用 VIA 第 0 层，存储与协议仍兼容原四层格式。
- **USB 音频**：原 UAC1 mono PCM16 16 kHz 播放、录音、反馈与 I2S/DMA 服务继续运行，
  不依赖当前 UI 页面。
- **蓝牙**：仅显示 C5 HCI 控制器初始化状态；与原程序一样，没有新增配对、BLE HID
  或蓝牙音频服务。

## 代码边界

- `3rd/tiny-ui`：可复用 no_std、无 alloc 组件框架，不依赖板级 HAL 或桌面业务。
- `src/tiny_desktop_page.rs`：桌面页面组合、界面状态、输入与业务动作。
- `src/tiny_desktop_ui.rs`：Core 1 屏幕/触摸适配和共享业务状态桥接。
- `src/desktop_controls.rs`：两种 UI 共用的无分配宏键盘控制器，保持多键和宏定时语义。
- `src/bin/apps/desktop/runtime.rs`：两个 bin 共用的 Core 0 固件入口与服务。
- `src/bin/apps/desktop_tiny_ui.rs`：选择纯 Rust 后端。
- `src/bin/apps/desktop.rs`：选择原 Slint 后端。

UI 使用固定容量状态和组件，不另建 demo crate，也不需要 Slint 堆。
720×1280 屏幕使用 360×640 逻辑画布和 2 倍输出；两张固定大小的 PSRAM
帧缓冲仍然需要内存。保留缓存维护、垂直同步和 Flash 写入时的 Core 1 停驻协议。

## 外观与验证边界

页面布局、导航和业务功能以原桌面为参考。字体、图标线条及控件皮肤不保证与 Slint
逐像素一致；不宣称完整 Slint 布局/动画引擎或惯性滚动。
现有 ASCII/CJK 字体覆盖不等于完整 Unicode 支持。

软件构建和静态检查不能证明 USB 音频长期稳定、
网络/显示/音频共存、真实触摸响应或 DMA/cache 时序已经通过。
既有音频偶发停顿、5 GHz 实际连接与复合负载长期稳定性仍需单独验证。

## 软件验证

项目自有测试与主机测试包已移除，禁止新增单元测试。使用构建、静态检查以及实机手动验证；
重点检查四页导航、Wi-Fi 选择与滚动、连接忙碌重试、密码输入/清理、局部绘制和双缓冲更新。

本次软件验证：15 个固件入口（包括原 Slint 桌面和新组件桌面）完整 release 编译链接、
全特性检查通过；组件框架 Clippy 与新增文件格式检查通过。
新桌面无 Slint feature 的独立 release 构建也已通过。没有新增真机或跨平台运行验证。

## 内存与绘制约束

第二核栈为 64 KiB。页面重建通过 `Scene::clear` 原地清理组件、焦点、捕获与按键状态，
保留视口和背景并整页标脏，避免用大型临时 `Scene` 替换内联组件槽位。
UI 状态使用固定容量存储，不另建 UI 堆；这些措施不代表已测得最坏栈水位。

每个逻辑像素以两个对齐的 32 位写入输出 2×2 物理像素，纯色矩形裁剪后连续写入，
减少细粒度 PSRAM 写入。保留 RGB565、双缓冲、原显示时序和三个 VSYNC 等待，
不改变共用的音频/无线调度。局部绘制与软件构建通过不等于长期稳定性或帧率保证。
