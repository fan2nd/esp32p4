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

首次提交时新增后端尚未进行开发板实测。软件构建和主机测试不能证明 USB 音频长期稳定、
网络/显示/音频共存、真实触摸响应或 DMA/cache 时序已经通过。
既有音频偶发停顿、5 GHz 实际连接与复合负载长期稳定性仍需单独验证。
实机启动修复记录见下文。

## 软件验证

页面主机回归使用 `python3 tools/test_tiny_desktop.py`，覆盖四页导航、Wi-Fi 选择与滚动、
连接忙碌重试、密码输入/清理、局部绘制和双缓冲历史修复。共享宏控制器测试位于
`3rd/keyboard-host-tests`。测试脚本不会烧录设备或启动浏览器。

本次软件验证：15 个固件入口（包括原 Slint 桌面和新组件桌面）完整 release 编译链接、
全特性检查、63 项主机测试通过；组件框架 Clippy 与新增文件格式检查通过。
新桌面无 Slint feature 的独立 release 构建也已通过。没有新增真机或跨平台运行验证。

## 2026-10-31 实机启动修复

迁移范围限定为 UI 框架，继续共用 Slint 桌面的 Core 0 运行代码；本次修复不改变
USB 音频、键盘/VIA、Wi-Fi/BLE、校时、显示时序、缓存策略或第二核 64 KiB 栈大小。

首次烧录 `28a3b92` 后黑屏。临时启动诊断捕获到 SDIO 初始化期间的
`lock is not reentrant`，但根因是第二核 UI 构造栈溢出，并非无线协议本身。
release 反汇编显示 `run → Ui::new → rebuild` 三个栈帧合计 77,488 字节，
超过 65,536 字节；栈下方恰有 HAL 外设锁状态。

`rebuild` 原先用整个 `Scene::new` 替换内联的 72 个组件槽位，产生大型栈临时量。
改为 `Scene::clear` 原地释放组件并清理焦点、捕获与按键状态，保留视口和背景，
整页标脏。重建栈帧由 20,992 字节降至 1,296 字节，三个栈帧合计降至
57,792 字节（此数值不包含更深的被调函数，不代表最坏栈水位测量）。
用于定位的共享启动代码改动已撤回。

验证：release 构建通过，页面测试 15 项、组件测试 17 项、共享宏控制器测试
12 项通过。修复版已烧录 P4 v3.1；串口确认 USB configured、音频播放接口
alt=1、Wi-Fi initialized、Bluetooth READY 和 display dpi streaming，启动不再
出现上述锁重入错误。用户确认主页出现，但交互偏卡；随后操作期间日志确认热点扫描、
关联、DHCP 和 SNTP 成功，所观察音频统计的 empty/drop/restart 均为 0。

绘制适配进一步对齐 Slint：每个逻辑像素以两个对齐的 32 位写入输出 2×2
物理像素，避免细粒度 PSRAM 写入；纯色矩形裁剪一次后连续写入。
保留 RGB565、双缓冲、原显示时序和三个 VSYNC 等待，未修改音频/无线调度。
该优化版已通过 release 构建并烧录，用户实测反馈“比上一版流畅多了”。
同期串口记录两次扫描成功（54/51 个热点），所观察音频统计的 empty/drop/restart
仍均为 0。该反馈确认交互流畅度改善，不代表已完成长期稳定性验证。
