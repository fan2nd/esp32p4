# 主页电脑状态与图片

`desktop_pocket_ui` / `desktop_tiny_ui` 的主页保留时间、状态图标、IP、天气和底部入口。
内容区为逻辑坐标 `(12,182,336,368)`：没有电脑状态时，图片按比例缩放并居中；
收到有效状态后显示 CPU/内存 240° 圆头仪表、双曲线趋势区和上传/下载速率。
当前字体：时间采用 eg-seven-segment；百分比、网速、IP、天气采用 Inter。
百分比放大并在圆心居中，左上角 PC/运行时长不再显示。
仪表及曲线采用固定 0..100% 刻度，保留最近 32 个样本（默认约 8 秒）；深色数字提高对比度。
超过 3 秒没有有效数据自动恢复图片；仅插 USB 或打开串口不会误判成状态在线。
主页 GIF 在状态卡片显示时暂停绘制，离线后恢复。Slint 版本暂未移植这套主页卡片。

## 连接电脑

CH343 仍只用于烧录。状态数据通过现有 VIA HID 接口新增的 Feature Report 传输，
走 EP0 控制传输，不占用 VIA 的中断收发队列，不增加 USB 接口或端点。
VID/PID 仍为 `CAFE:4014`；保留音频、MSC、键盘和触控板的接口编号。
电脑不需要新增串口驱动，连接 USB-A 数据线即可。

电脑采集工具位于 `tools/pc-monitor`，参考 `D:/MyGithub/rp2350-yz/host` 的 Rust
系统统计与网卡筛选逻辑，传输改为 HID Feature Report。Windows 使用 hidapi 的
`windows-native`，Linux 使用 `linux-native`，不编译 hidapi C 实现。当前不支持 macOS
采集工具（保持禁止引入 C 实现的项目约束）；网页仍不受此限制。
协议解析与固件通过普通 `#[path] mod` 共用，不引入参考项目的测试或 UI 依赖。

Windows 构建（仓库根目录）：

```powershell
cargo +stable build --release --locked --manifest-path tools/pc-monitor/Cargo.toml --target x86_64-pc-windows-msvc --target-dir target/pc-monitor
target/pc-monitor/x86_64-pc-windows-msvc/release/p4-pc-monitor.exe
```

默认每 250 ms 上报一次，自动选择 P4 的 VID/PID 和 `FF60:61` HID usage。
多个设备时用 `--serial SERIAL` 指定。`--list-devices` 列设备，`--list-interfaces` 列网卡，
`--interface NAME` 只采集指定网卡，`--interval 0.5` 修改采样间隔，Ctrl+C 停止。
Linux 使用 stable 工具链指定本机 Rust target 编译，还需授予 hidraw 访问权限；本轮未实测。
原 rp2350-yz 串口采集工具不能直接连接；本工具保留其 PCMON1 文本格式。

HID Feature Report 为 32 字节：命令 `0x80`、文本长度（0..30）、最多 30 字节文本、
其余补零。长行分多包，常规样本仅 SET_REPORT，控制传输自身确认交付；只有握手后 GET_REPORT
读取应答，减少音频并发时的控制请求。重新握手先发换行，清除上次中断留下的半行。主机 API 的额外首字节 report ID 为 0，不计入上述 32 字节。
VIA 普通 IN/OUT 请求响应保持原样，可与电脑状态采集同时工作。

协议为逐行 ASCII：`HELLO\n` 回复 `PCMON1 READY\n`；数据为
`PCMON1,sequence,cpu,memory,rx_kib,tx_kib,uptime_s\n`。
CPU/内存单位为 0.1%，范围 0..1000；网络单位 KiB/s。超长行、非数字、越界或字段数
不正确的消息丢弃，下一换行重新同步。连接判断以有效数据到达时间为准。

## 刷新与缓冲

RGB565、HFP=52 保持，VFP 从 1023 改为 24；按实际 60 MHz 像素时钟计算，
扫描频率约 `60000000 / 812 / 1318 = 56.1 Hz`。这是扫描频率，不等于业务采样/绘制 FPS。
显示核心新增非阻塞 VSYNC 查询，在提交后等两个新帧边界才复用旧缓冲，同时继续处理触摸。
去掉同步等待三次 VSYNC 和循环末 5 ms 忙等，改为 1 ms 轮询间隔；仍使用双缓冲和前次脏区修复。
状态数据默认 4 Hz，趋势每个新样本推进一次，未变化页面不持续重绘。

## 验证记录

2026-11-08：全部程序 all-features release 构建通过，Windows 采集工具 release 构建通过。
仪表盘 Pocket 固件已按 921600 烧录；USB HS、SD、Wi-Fi、天气启动正常，VIA v13 读取通过。
真实 CPU/内存/网络/开机时长采样完成 240 次（约一分钟），采集工具正常退出，无断连重试；
采样与连续音频并行运行；本轮未出现 DMA 重启或丢包，
但 Wi-Fi 联网附近有一次 empty=39、最大收包间隔约 9.2 ms，尚不能认定音频完全无卡顿。
仪表盘可读性、曲线和停止采集后图片恢复仍待用户目视确认。

## Tiny UI 固件

仪表与曲线位于共享业务页面 `src/desktop/pc_monitor.rs`，由 `page.rs` 的主页叠加绘制。
`desktop_tiny_ui` 使用 Tiny UI 场景绘制，`desktop_pocket_ui` 使用 PocketJS 场景绘制；
两者共用数据接收、历史采样、离线图片恢复及双缓冲刷新逻辑，不复制仪表代码。

```powershell
cargo build --release --bin desktop_tiny_ui
espflash -S flash --port COM6 --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/desktop_tiny_ui
```

Tiny UI 构建不启用 `pocket-ui`，不需要 PocketJS/MicroTS 运行时。电脑采集工具通用。

Tiny UI 实机记录（2026-11-08）：独立 release 构建并烧录成功，日志确认运行
`desktop_tiny_ui`，屏幕 DPI 已启动；电脑采集程序自动重连，VIA v13 读取通过。
启动后的音频统计 empty/drop/restart 均为 0；本轮仅短时观察，画面和触摸待用户确认。

配色调整：CPU 仪表/曲线采用蓝色，内存仪表/曲线采用橙色，替换此前接近的蓝绿色；
图例同步，数值仍使用深色，避免影响可读性。

整体主题调整：页面背景采用中性浅灰、白色卡片、深灰文字与灰色边框，去掉大面积蓝绿色底。
CPU 蓝色、内存橙色；在线 Wi-Fi 绿色、蓝牙蓝色、USB 紫色，离线统一灰色。
控件按下状态使用浅蓝，仪表轨道与曲线网格改为中性灰，避免与数据颜色混淆。

仪表改为 240°，底轨与进度头尾均为圆头。圆弧及趋势线采用局部 2×2 覆盖采样抗锯齿，
只在显示核临时栈空间生成小遮罩，不增加整屏缓冲或 PSRAM 超采样；圆头与弧线取并集，避免接缝。
时间策略更新：Wi-Fi 获得 IP 且尚无有效时间时自动尝试一次；已有时间不自动校时。
失败后同一次连接不循环重试，下次重新联网且仍无时间时可再次自动尝试，主页点击时间仍可手动重试。

本轮 Tiny UI 已构建烧录，采集自动重连、VIA 读取通过。日志确认 DHCP 后在没有有效时间时
自动完成 SNTP（time synchronized=true）。音频持续运行无 DMA 重启，但联网/校时附近有一次
empty=97、最大收包间隔约 9.8 ms；抗锯齿外观与切页流畅性待用户确认。

百分比调整：用户要求放大的是数字，仪表直径保持 120 逻辑像素；百分比字形放大至
15×30 逻辑像素单元，并在圆心 (卡片 x+81, 304) 居中。删除左上角 PC/运行时长文本。

字体更新：时钟按用户指定改用 eg-seven-segment 0.2.0（七段数码管）；
百分比使用 26px Inter，网速使用 18px Inter，主页 IP/天气使用 16px Inter。
Inter 复用现有 A8 字库并做双线性采样，不引入 C 或运行时字体解析。
`FONT_10X20` 及其变体/缩放列入 AGENTS.md 黑名单；历史诊断程序、键盘/设置页仍有旧用法，
后续触及这些界面时迁移，不允许在新界面中继续引入。百分比仍按实测字宽圆心居中。
七段字体文档：https://docs.rs/eg-seven-segment/0.2.0/eg_seven_segment/

字体版 Tiny UI release 编译、921600 烧录完成，显示已启动；100.0% 的 Inter 字宽为
91 逻辑像素，能放入仪表内部。字形观感和视觉居中待用户确认。
