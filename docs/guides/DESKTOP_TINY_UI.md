# 纯 Rust 组件桌面

`desktop_tiny_ui` 是完整桌面程序的第二个 UI 后端，参考的是
`assets/ui/desktop.slint`，不是滑条/按钮展示页 `assets/ui/demo.slint`。
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
- **设置主页**：Wi-Fi、蓝牙名称、时间同步、屏幕亮度、播放音量、重置六个入口；子页面返回设置主页。
- **Wi-Fi 子页**：扫描、可滚动的 Wi-Fi 列表、SSID/RSSI/安全性、连接结果与断开。
  密码页返回 Wi-Fi 子页；状态/时钟更新不应重置列表滚动位置。
- **密码页**：复用原 PC 键盘布局、Shift/Caps、退格、清空、空格和 Enter；
  退出页面或成功交付连接命令后清理输入；连接成功的凭据保存在 Flash，开机尝试连接。
- **USB 宏键盘页**：8 键多点触摸和 VIA 改键，保留键盘/Consumer Control 报告与宏播放。
  屏幕使用 VIA 第 0 层，存储与协议仍兼容原四层格式。默认键位和宏均为空，已有改键配置保留。
- **USB 音频**：原 UAC1 mono PCM16 16 kHz 播放、录音、反馈与 I2S/DMA 服务继续运行，
  不依赖当前 UI 页面。
- **VIA 定义盘**：复合 USB 同时提供 `P4 VIA` 只读盘，手动从中选择 `VIA.json`
  导入 VIA Design 页，见[只读盘用法与限制](VIA_DRIVE.md)。
- **microSD 读卡器**：同一 MSC 接口的 LUN 1 暴露板载卡槽，可由电脑读写；
  不自动格式化，见[读卡器用法与验证](SD_READER.md)。
- **蓝牙名称子页**：输入最多 24 个 ASCII 字节，明确点击 Apply 后发布不可连接的 BLE
  广播名称；开机恢复上次已保存的广播状态。成功状态只在控制器确认后显示，不提供配对、GATT、BLE HID
  或蓝牙音频服务。手机需要 BLE 扫描工具查看，不保证出现在系统可配对设备列表。
- **屏幕亮度子页**：滑条通过 Core 1 的共享 I2C 写背光控制器 0x45 的 0x96 寄存器。
  范围 5%～100%，保留最低可见亮度以便继续触摸；写入失败回退到上次成功值。
- **播放音量子页**：滑条在 USB PCM 送入 I2S 前作 0%～100% 数字衰减，0% 静音，
  100% 为原有单位增益；不改变麦克风增益、主机音量或 USB 描述符。
  按下、拖动和松开均更新 Core 0 的原子音量值，下一批送入 I2S 的样本立即使用新衰减。
  这不等待两秒 Flash 防抖保存；听感延迟包含触摸/显示循环以及已排入 DMA 的音频
  （4096 字节双声道 16 kHz PCM16 缓冲最多约 64 ms）。主机混音器与设备衰减叠加，
  0% 为静音，设备 100% 不额外放大。

以上名称、蓝牙广播启用状态、亮度和播放音量与 Wi-Fi 凭据、VIA 键位/宏一起保存在 Flash。
蓝牙命令超时或传输状态不确定会显示错误并要求重启，避免迟到响应被后续请求误用。
配置使用纯 Rust `sequential-storage 8.0.2` 的 Map KV，追加写入、CRC、断电修复和
页回收由库负责；`postcard` 序列化有版本号的配置与 VIA 快照，作为一个键提交以保持原子性。
`settings` 独立分区位于 `0xff2000..0x1000000`（56 KiB），不挤占 app 或既有键位分区。
分区表使用工具支持的 `data,nvs` 类型，但内容是 sequential-storage 格式，不能由 IDF NVS 挂载。
首次启动自动读取旧 `P4KEY001` / `P4CFG001` 记录迁移，读回校验新 KV 后使用它；旧格式仅保留读取。
必须用更新后的 `config/partitions-keyboard.csv` 烧录；分区缺失或 KV 读取失败时显示不可用，
不把故障当作空数据库自动格式化。后续升级固定库主版本并保留 schema 迁移策略。
变更停止两秒后保存，按键按下或宏分块写入未结束时延迟，设置页显示保存状态。
USB 播放/录音不再阻止保存；看到 `Saved to flash` 后再断电。
Wi-Fi 密码存于设备 Flash（未加密），不会出现在串口日志或 MSC 的 VIA 定义盘中。
蓝牙名称只在 HCI 应用成功后保存，Wi-Fi 只在关联成功后保存，失败的输入不覆盖旧值。
Wi-Fi 最多记住 8 个网络，按最近成功连接顺序保留。列表优先列出已保存网络，标记
`SAVED` / `CONNECTED`；未扫描到的已保存网络也保留，显示 `not in scan`。
点已保存网络可直接连接、修改密码或忘记；新热点才进入密码输入页。
开机在 Wi-Fi 开启时连接最后成功的网络；连接失败或掉线后间隔 30 秒重试。
手动 `DISCONNECT` 仅停止本次运行的自动重连，不删除密码；`FORGET NETWORK` 才删除记录。
`Wi-Fi ON/OFF` 使用 Hosted MCU 的 WifiStart/WifiStop，停止 Wi-Fi 不重置共享 C5/SDIO。
开关成功后写入 KV；关闭仍保留密码，重新开启恢复自动连接。
BLE 开关停止/启动当前支持的不可连接名称广播，主页图标据已确认广播状态点亮。
它不是 C5 整体断电；共存控制器/传输仍运行，不提供蓝牙音频或配对。
配置 schema v2 显式迁移已有 v1 单网络配置，不清空旧密码或 VIA 键位。

设置首页的 `Reset settings` 打开确认页，列明 Wi-Fi、蓝牙、亮度/音量、VIA 键位/宏。
取消不修改配置；确认后冻结界面修改，先提交默认 KV，再擦除旧迁移槽及 KV 历史页，重新写入并校验默认值后重启。
默认亮度/音量 100%，名称 `P4 Desktop`，不自动广播，无已保存 Wi-Fi，键位均恢复空白 `KC_NO`，宏清空。
写入失败保留重试，不在保存失败时重启。设备状态、扫描结果、IP 和当前时间不当作配置保存；
它们在开机后重新获取。

KV 选型依据：[sequential-storage 官方说明](https://github.com/tweedegolf/sequential-storage)
提供无堆的 Flash KV、追加写入和空间回收；当前仅有少量设置，采用单键快照避免多键提交不一致。
[Embassy ekv 官方文档](https://docs.embassy.dev/ekv/1.0.0/default/index.html)
说明其 LSM-tree 适合大量键，小数据集可优先使用 sequential-storage。
`ekv` 有多键事务，但当前不需要引入该复杂度。

保存不再受音频流活跃标志限制。Core 1 确认进入内部 RAM 停驻后，Core 0 才写 Flash；
`esp-storage` 使用 `multicore_ignore`，因为手动停驻并不关闭另一核时钟，默认检查会误报
`OtherCoreRunning`。该设置只在启动第二核前或收到停驻确认后使用，不允许任意并发写 Flash。
`BlockingAsync` 只是接口适配；擦写期间仍存在阻塞，不等于硬件自动挂起或后台 DMA 写入。

2026-11-01 实机：从旧键位记录迁移成功（generation=8），随后重启读取 KV 成功。
连续播放时 15 次不同配置写入及读回成功，约 3.4～4.2 ms/次，所在统计区间
`empty/drop/restart=0`。这些数据只覆盖追加写；页回收结果见下，重置尚未单独验证。
VIA 全部 19 个宏读取分块（包括 offset=504、length=28）与 MSC 定义文件校验通过。
Wi-Fi 停止/开启 RPC、热点关联、DHCP 和 SNTP 已在实机成功。
90 次键位变更及恢复原键位的持续播放验证进入页回收，测得约 39～40 ms 写入窗口，并出现音频补零（约 450 样本），
DMA 未重启。保存可以在播放时完成，但页回收尚不能保证无卡顿；不能把追加写结果推广到擦除。
重启后 Wi-Fi 凭据、50% 亮度、BLE 开启状态恢复；自动关联、DHCP 均成功；校时现改为仅手动触发。
用户确认已保存网络重连、扫描中连接请求、BLE 开关及音量实时生效正常。扫描期间的连接请求排队执行，
不再因控制器忙而拒绝已经输入的密码。Hosted 驱动另用断开事件序号同步 RPC，
避免上一次主动断开的迟到事件误报下一次连接失败。

参考：[Espressif Flash 并发说明](https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/spi_flash/spi_flash_concurrency.html)。


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

主页状态图标采用 Google Material Design Icons 的 Wi-Fi、Bluetooth 和 USB 标准图形，
来源、固定版本及 Apache-2.0 许可证见 [图标说明](../../assets/material-icons/README.md)。
30×30 alpha 蒙版在离线生成，设备只做 RGB565 混色；未引入运行时 SVG 解码或额外 UI 堆。
普通按钮和键盘按键都有内描边，按下填充与键盘焦点分别显示，触摸抬起后不留下焦点框。
列表溢出时，右侧边框上显示比例滚动指示段：保留原宽 4 个逻辑像素，颜色混合边框色与
强调色，不再另画独立轨道或预留宽滚动槽。上下避开圆角，内容不溢出时隐藏。
它是位置指示器，滚动仍通过列表拖动，不支持单独拖动滑块。


软件构建和静态检查不能证明 USB 音频长期稳定、
网络/显示/音频共存、真实触摸响应或 DMA/cache 时序已经通过。
既有音频偶发停顿、5 GHz 实际连接与复合负载长期稳定性仍需单独验证。

## 软件验证

项目自有测试与主机测试包已移除，禁止新增单元测试。使用构建、静态检查以及实机手动验证；
重点检查多级设置导航、Wi-Fi 选择与滚动、连接忙碌重试、密码输入/清理、局部绘制和双缓冲更新。

本次软件验证：15 个固件入口（包括原 Slint 桌面和新组件桌面）完整 release 编译链接、
全特性检查通过；组件框架 Clippy 与新增文件格式检查通过。
新桌面无 Slint feature 的独立 release 构建也已通过。没有新增真机或跨平台运行验证。背光档位、实际音量、C5 不可连接广播名称及共存行为仍需上板确认。

## 内存与绘制约束

第二核栈为 64 KiB。页面重建通过 `Scene::clear` 原地清理组件、焦点、捕获与按键状态，
保留视口和背景并整页标脏，避免用大型临时 `Scene` 替换内联组件槽位。
UI 状态使用固定容量存储，不另建 UI 堆；这些措施不代表已测得最坏栈水位。

每个逻辑像素以两个对齐的 32 位写入输出 2×2 物理像素，纯色矩形裁剪后连续写入，
减少细粒度 PSRAM 写入。保留 RGB565、双缓冲、原显示时序和三个 VSYNC 等待，
不改变共用的音频/无线调度。局部绘制与软件构建通过不等于长期稳定性或帧率保证。

## 蓝牙命令参考

名称广播使用 Bluetooth Core HCI 的 LE Set Advertising Parameters、LE Set Advertising Data
和 LE Set Advertising Enable（Vol 4, Part E, 7.8.5/7.8.7/7.8.9）；参数采用不可连接广播，
不把 UI 文本更新当作控制器应用成功。
[Bluetooth Core 6.1 HCI 规范](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host-controller-interface/host-controller-interface-functional-specification.html)。

## 烧录速度

ESP32-P4 UART 官方上限为 5 MBaud，该值是硬件规格，不等于当前下载链路可靠速度。
按用户选择，项目 runner 使用 `--baud 921600`（不是 961200）。
CH343 的 6 Mbps 上限不作为本板下载速率依据。串口日志仍使用独立的 115200 baud。
实机已用 921600 完成 desktop_tiny_ui 烧录并自动启动；日志波特率不变。
实际烧录可靠性需由整条 UART/USB 链路验证，芯片上限不等于下载有效吞吐量。
[ESP32-P4 官方规格书 UART 章节](https://documentation.espressif.com/esp32-p4_datasheet_cn.html)，
[Espressif 下载波特率说明](https://docs.espressif.com/projects/esptool/en/latest/esp32p4/esptool/basic-options.html)。

## 手动校时

设置 → Time synchronization → SYNC NOW 才执行 SNTP 请求。连接 Wi-Fi、重新连接和
等待一小时都不再触发校时；同步失败也不自动重试。未连接时提示先连接 Wi-Fi。
同步成功后本机单调时钟继续计时，默认北京时间 UTC+8；重启后未同步显示 `--:--:--`。
原 Slint 设置页也提供 SYNC TIME 按钮，使用相同 Core 0 命令。

手动校时版已完成全部固件 release 构建、921600 烧录及实机验证；用户确认连接后不自动校时、点击同步和已保存热点重连正常。VIA 协议读取和定义盘校验再次通过。
