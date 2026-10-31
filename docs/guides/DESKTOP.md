# 双核桌面固件：复合 USB、Wi-Fi 与蓝牙状态

核心职责划分见 [框架梳理及双核日志审查](../notes/DESKTOP_ARCHITECTURE.md)。
最新音频 DMA 缓存隔离修复与触摸优化见 [本轮验证记录](../notes/AUDIO_DMA_CACHE_FIX.md)；
下方排查过程保留历史现象，不代表最新固件仍持续发生这些故障。

`desktop` 是集成固件，保留独立的 `keyboard`、`usb_audio`、`verify_hosted`
例程，全部应用与驱动实现仍为 Rust。使用已经在 C5 上运行的 ESP-Hosted MCU
3.0.10 SDIO 固件，不在本项目编译 C 固件。

## 界面

2026-10-31 起界面改用 Slint，运行在第二核；页面声明位于 `ui/desktop.slint`。
主页显示现有鲸鱼娘插画；顶部显示放大的时间，以及 Wi-Fi、蓝牙和 USB 图标。
图标高亮表示已连接/就绪，暗色表示未就绪。蓝牙就绪只表示控制器初始化成功，不代表已配对。
USB 图标表示已配置且未挂起。驱动当前未启用 VBUS 检测，不能可靠区分物理拔线和主机挂起，
因此非活动状态使用 `USB inactive`。状态处理保留枚举结果，挂起时变暗、恢复时重新点亮；
复位或取消配置清除枚举结果。此前只处理 configured/reset，遗漏 suspended 回调，导致状态可能停留在连接状态。
2026-10-31 已烧录验证：拔掉 USB-A 数据线后日志出现 `USB suspended=true active=false`，
插回后出现总线复位及 `USB configured=true`；用户确认图标熄灭/点亮正常。两次拔插记录与反馈一致。
Wi-Fi 获取到 DHCP 地址后显示 IP，断开后清除 IP。小 `SET` 按钮进入设置。

- `SCAN WIFI`：读取最多 64 条结果，按信号强度排序、按 SSID 去重，使用可拖动的滚动列表。
- 点击网络进入密码页，采用 PC QWERTY 错位排列，含数字、Shift、Caps、符号、空格及退格，密码遮罩显示。
- `CONNECT`：连接后等待 DHCP，取得 IP 后发起 SNTP 校时；`DISCON` 断开。
- 主页 `KEYBOARD`：进入 8 键单层宏键盘，使用 VIA 第 0 层；`BACK` 返回主页。
  旧配置和 VIA 的四组存储格式保留兼容，但屏幕不提供切层、不执行层切换键码。
  若原第 0 层含 MO/TO/TG 等切层键，可在 VIA 第 0 层将其改成需要的普通键或宏。
- Wi-Fi 名称支持 ASCII 和基本 CJK 汉字；其他 Unicode 字形未完整覆盖。

凭据仅在 RAM 中用于连接，不写 Flash、不记录密码日志；当前未实现开机自动重连。
时间默认 UTC+8（北京时间）；未校时时显示 `--:--:--`。时间服务器依次尝试
`time.cloudflare.com`、`pool.ntp.org`，检查来源地址、端口、模式、层级、同步状态、
请求回显及时间戳。失败约 30 秒后重试，成功后约每小时同步。当前解析支持 NTP
第一时代至 2036 年，不把未校时占位值显示为真实时间。

## 核心与外设分工

| 核心 | 所有者/任务 |
| --- | --- |
| Core 0 | USB HID + UAC1、ES8311/I2S DMA、C5 SDIO、Wi-Fi/DHCP/DNS/SNTP、蓝牙 HCI |
| Core 1 | MIPI/VDMA 初始化及中断、RGB565 双缓冲绘制、GT911 触摸和屏幕交互 |

ES8311 在 Core 0 启动时配置完毕后，I2C1 所有权转交 Core 1；两个核心不并发操作
同一 I2C 外设。跨核状态和命令使用临界区保护，锁内不绘制、不访问 I2C、不 await。
Core 1 使用独立 64 KiB 栈。显示为 720×1280、RGB565 双缓冲、约 31 Hz；
DSI 线上仍为 RGB888，沿用已验证的 565 通道顺序和帧边界切换策略。
Slint 的 4 MiB 堆位于 PSRAM 双帧缓冲之后；USB/I2S 缓冲仍在内部 SRAM。
软件渲染采用 `SwappedBuffers` 脏区域跟踪与 360 像素 SRAM 行缓冲，保留现有缓存互斥。
热点变化时才更新列表模型，状态或时间变化不会重建列表；中文 SSID 使用已有授权位图字库生成透明图像。

2026-10-31 迁移版已编译全部启用 Slint 的例程并烧录；启动日志确认 Slint/GT911、
USB 复合设备、Wi-Fi 和 Bluetooth READY，VIA 读取通过。密码键盘新增三项测试，
连同原键盘四项测试共七项通过。初始音频统计未见补零、丢包或 DMA 重启；
随后通过屏幕发起 Wi-Fi 连接，日志确认 association、DHCP 和 SNTP 均成功；
多次扫描（原始记录 40–62 条）与连接期间，观测到的音频窗口仍为补零/丢包/重启均零。
屏幕视觉及主观触摸、音频体验仍以用户实机反馈为准。
CPU 使用率当前未实现，统计方案见 [CPU_USAGE.md](../notes/CPU_USAGE.md)。

当前 Hosted Git 版本使用 xarxa 的 owned-packet 驱动接口，与 embassy-net 0.9
的 token 接口不同。`driver/hosted_net.rs` 提供有界 12 包静态池与实际接口转换，
不引入另一套 IP 协议栈，也不使用堆分配。

## USB 与蓝牙

复合 USB 实验 VID/PID 为 **CAFE:4014**，名称 `P4 Desktop Keyboard + Audio`。
包含键盘、Consumer Control、VIA Raw HID 和 UAC1 麦克风/扬声器。
USB 使用已验证的 HS PHY 全速模式；音频保持 mono PCM16、16 kHz、DAC 0 dB。

VIA 使用 `assets/p4-desktop-via.json`，不能用独立键盘 `4013` 的定义替代。
当前实验设备尚未收录到官方定义库，首次使用官方 VIA 网页需要在 Design 页加载这份 JSON。
这份文件描述矩阵、按键位置和设备标识，不是从设备导出的改键备份，也不会替代固件里的键位保存。
官方说明 Design 页载入的草稿定义可跨会话保留，因此同一浏览器通常只需加载一次；
换浏览器、清理站点数据或使用无痕会话时可能需要重新加载。
若定义被 VIA 官方键盘库收录并发布，网页可自动获取匹配定义，无需用户手工加载。
也可以维护预置本设备定义的自有 WebHID 配置网页，但仅把 JSON 放进固件或项目 GitHub 仓库，
不会让官方 VIA 网站自动读取它。自动加载布局与浏览器的 USB/HID 授权是两件事。
参考：[VIA 定义规范](https://www.caniusevia.com/docs/specification/)、
[草稿定义跨会话保存](https://caniusevia.com/docs/whats_new/)。
分区仍使用 `config/partitions-keyboard.csv`，可读取原键盘配置。修改配置后空闲约 2 秒
保存；播放/录音期间延迟保存。写 Flash 前等待 Core 1 进入内部 RAM 停驻函数，
禁止其同时从 Flash 取指，完成后恢复。双核写 Flash 的显示/音频影响待实机验证。

蓝牙仅做控制器初始化与真实状态展示：经 C5 HCI 发送 Reset、读取公共地址，
收到成功的 Command Complete 后显示 `READY`，超时或错误显示 `ERROR`。
不宣称已配对/连接，不启用蓝牙键盘、广播或音频服务。初始化后持续消费 HCI 事件。

## 构建与验证

```powershell
cargo build --release --bin desktop --features slint-ui
espflash flash --port COM6 --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/desktop
python tests/keyboard/via_probe.py --pid 0x4014 --roundtrip
python tests/usb_audio/playback.py --device "P4 Desktop"
cargo test --manifest-path tests/desktop/Cargo.toml --target x86_64-pc-windows-msvc
```

截至 2026-10-31：集成固件及原 keyboard/usb_audio/verify_hosted 例程编译通过。
桌面协议/跨线程包池 2 项、键盘 4 项、USB Audio 2 项、Hosted 4 项主机测试通过。
后续已完成 COM6 烧录，Windows 能枚举复合 USB，日志确认副核启动及 GT911 探测成功。
此前纯红/通信停止问题经独立组合实验定位到跨核缓存维护缺少互斥，
详见 [缓存修复记录](../notes/SDIO_MIPI_CACHE_FIX.md)。已恢复完整桌面集成并烧录。
VIA 四层键位读取通过，Wi-Fi、蓝牙和触摸初始化成功；USB 播放传输完成。
麦克风录得的样本幅度接近零，尚未确认声学响应。音频曾出现 DMA 欠载，
已将缓存写回按 4 KiB 分段互斥，仍需验证长时间音频稳定性。

用户反馈连接 Wi-Fi 时黑屏、没有 5 GHz 热点。该时段与新版烧录重启接近，
且电脑曾短暂无法枚举 COM6 和复合 USB，随后恢复；用户确认没有拔线。
目前不能把黑屏归因于无线连接，也不能认定供电故障。
增加关联/DHCP 阶段日志，将原 16 条扫描记录上限扩至 64 条，待复现。
同 SSID 仍合并显示，扫描接口没有暴露信道，不能从列表判断实际频段。
ESP32-C5 官方说明默认 AUTO 扫描双频，但尚未读取本机 C5 的实际频段配置：
[Espressif C5 Wi-Fi 说明](https://developer.espressif.com/workshops/esp-idf-with-esp32-c5/lecture-1/)。
2026-10-31 后续实机反馈：用户确认新版正常。串口确认扫描分别返回
58、61、64 条记录，关联成功、DHCP 获取地址成功、SNTP 校时成功。
原先 16 条上限确实会截断该环境的扫描结果；尚未读取连接信道，
不能据此证明当前连接为 5 GHz。64 条仍是有界上限，最后一次已达到上限。
日志仍出现 TX DMA underrun/restarting 和 USB bus reset；音频长时间稳定性
及麦克风声学响应尚未验证，不能把 Wi-Fi 成功等同于全功能验证完成。

## 刷新优化验证（2026-10-31）

用户反馈界面刷新慢。当前逻辑画布按 16 行分成 40 个条带绘制；此前每个条带
都会对全部圆角按钮、文字和 SSID 执行光栅化，再在逐像素输出时丢弃范围外像素。
已改为绘制前检查控件与条带的交集，跳过不相交控件；保留内部 RAM 条带和
批量写 PSRAM，避免退回逐像素 PSRAM 写入。
交互刷新日志新增 render/flush/total 毫秒耗时，供实机定位剩余瓶颈。
当前仍保留约 31 Hz 扫描、提交后等待三次 VSYNC，以及全帧缓存提交；
并未宣称已提高面板刷新率。编译通过，实际交互提速与画面完整性待用户验证。

## 刷屏与音频调度（2026-10-31）

音频使用 HAL 的静态 DMA 宏分配，未使用 PSRAM 堆；屏幕才使用 PSRAM arena。
内部 SRAM 在 P4 上仍有缓存维护，不能通过“放内部 SRAM”绕过缓存同步。
检查发现主核日志每次同步输出整行，而音频每轮最多补 128 字节（2 ms 的
16 kHz 双槽 PCM16）；连续日志输出会使补数跟不上 DMA 消耗。
已将日志拆成不超过 4 字节的 UTF-8 完整片段，段间让出执行权；
TX 内部 SRAM 环形缓冲由 1024 字节扩至 4096 字节（16 ms → 64 ms），
音频单轮处理上限增至 1024 字节（16 ms），以便延迟后追赶。
代价是播放流水线延迟可能增加约 48 ms；麦克风 DMA 容量仍为 4096 字节。
启动日志输出 TX 数据与描述符地址，便于核验实际内存位置。
编译与两项 USB Audio 主机测试通过，已烧录启动。实机地址为 TX 数据
`0x4ff53f00`、描述符 `0x4ff63ec0`，均落在内部 RAM；启动阶段未见欠载日志。
刷屏并发播放的声学连续性仍待实机确认。

后续用户确认静止也卡。增加 5 秒汇总统计后，观察到主页定时刷新时
约收 4975 个 USB 包/5 秒，并有数百采样补零，主循环最长间隔约 3 ms；
不重绘的设置页连续多轮收到 5000 包、80000 个采样，补零/丢弃/重启均为零。
因此主页每秒重绘也会触发问题，不能把“没按键”视为完全没有刷新。
缓存写回分段间试加 2 us 间隔无明显改善，已撤回。

USB 驱动原来只缓冲一个 OUT 包，必须由应用 read 后再启用端点。
已改为 ISO OUT 在传输完成中断里立即接续接收，并在内部 SRAM 中提供
8 包 SPSC 队列；普通 HID/控制端点维持原行为。队列满时丢弃新包，不覆盖
消费者尚未读完的槽；启用端点时在临界区清空队列。独立 usb_audio 的
端点缓冲同步扩大到 512 字节，并启用 OUT transfer-complete 中断。
实机新固件在主页周期刷新时，连续七轮 5 秒统计均为 5000 包、补零 0、
丢弃 0、运行期 DMA 重启 0（启动时一次重启另计）。主循环最大间隔仍约
3.2 ms，说明中断队列确实容纳了应用调度延迟。VIA 四层读取回归通过。
主观听感与连续交互期间稳定性仍待用户确认；不把短期统计等同于长期验证。

后续用户试听反馈“基本不卡”。更长时间日志中，多数窗口保持 5000 包/5 秒、
补零为零，连续切页时也能保持收包；但仍出现偶发 TX DMA underrun/restarting，
对应窗口队列丢弃约 850 个采样，少数窗口有少量补零。因此当前结论是卡顿
显著改善，USB 中断收包改动有效，尚未达到长期零断音。剩余问题应重点核查
I2S TX DMA 描述符链与缓存一致性，不能再次简单归因为音频数据放在 PSRAM。

## 插画与字体

鲸鱼娘复用 `assets/whale-girl-wave-large.rgb`。SSID 汉字位图由 Noto Sans SC
生成，许可见 `assets/CJK-FONT-LICENSE.txt`（SIL OFL）；原字体来自
[Google Fonts Noto Sans SC](https://github.com/google/fonts/tree/main/ofl/notosanssc)。
`tools/build_cjk_font.py <NotoSansSC[wght].ttf>` 生成 `assets/cjk16.bin`，需 Pillow。
