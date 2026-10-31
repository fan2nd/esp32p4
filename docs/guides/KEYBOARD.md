# 8 键触屏宏键盘

`keyboard` 是 ESP32-P4 的独立 Rust 例程，使用现有 720×1280 MIPI 屏幕、
GT911 触摸和 USB-A 数据口；运行它会替换当前 USB Audio 应用。没有引入 C 实现。

## 布局与功能

屏幕为 **2 列 × 4 行的 8 个大按键**，每键 312×198 像素。顶部 L0～L3
直接切换四层，不占用可改键位置。按下高亮，松开释放，VIA 改键同步更新键名。

| 层 | 从上到下、每行从左到右 |
| --- | --- |
| L0 | 复制 / 粘贴；撤销 / 重做；Esc / Enter；按住 L1 / Space |
| L1 | F1 / F2；F3 / F4；音量加 / 音量减；透明 / 播放暂停 |
| L2 | 宏 1～8，初始为空 |
| L3 | 左 / 右；上 / 下；Home / End；Page Up / Page Down |

支持普通 HID 按键、Ctrl/Shift/Alt/GUI 组合、常用媒体键、MO/TG/TO/DF 层切换、
透明键及 8 个 VIA 宏（合计 512 字节）。宏支持 US ANSI ASCII 文本、按下/松开/
点击和延时；单次延时上限 10 秒。新的触摸按键会取消正在执行的宏。
宏执行期间不合并其他已按住的实体触摸键；结束后恢复当前触摸状态。

这不是完整 QMK 实现：不支持 Mod-Tap、Layer-Tap、Tap Dance、鼠标键、RGB 菜单、
Unicode 宏或全部扩展 QMK 键码。未知键码会保留并显示十六进制，但不会发送。
同时最多 5 个触点；USB 键盘报告为 6KRO。GT911 超过 250 ms 无有效报告会释放键，
以避免总线故障后一直按住。按住时改键不会更换当前已按下的键码，松开后再生效。

## 构建与烧录

```powershell
cargo build --release --bin keyboard --features keyboard-low-refresh
espflash flash --port COM6 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/keyboard
```

必须带该分区表，不能直接使用默认 `cargo run` runner 烧录后期待掉电保存。
屏幕出现 `NO SAVE` 表示检测不到正确的独立分区，固件不会写入 Flash。
下载线接 Type-C / CH343；键盘数据继续使用已经测试 USB Audio 的 USB-A 数据连接。
当前 USB 使用 HS 控制器/PHY 的全速模式，VID/PID `CAFE:4013` 为实验用途。
当前板上使用 RGB565 双缓冲、约 30.85 Hz。省略 `keyboard-low-refresh` 时，
RGB565 双缓冲保留，但垂直前沿恢复为 24 行，即原约 54.19 Hz。

## VIA 改键

1. 用支持 WebHID 的 Chrome/Edge 打开 <https://usevia.app/>。
2. 在设置中启用 Show Design tab，在 Design 中加载
   `assets/p4-macropad-via.json`（V3 定义，不启用旧 V2 定义开关）。
3. 授权连接 `P4 Touch Macropad`，选择层和按键后改键。
4. 改完等待约 1.5 秒，底部 `EDITED` 变为 `SAVED`；再断电测试保存。

VIA Raw HID 使用 usage page `0xFF60` / usage `0x61`、32 字节报告、协议版本 13、
键码版本 9。需要手工加载 JSON；此实验设备未加入 VIA 官方键盘列表。
USB 键盘、Consumer Control、VIA 分别使用独立 HID 接口。

## 保存机制

16 MB Flash 最后 8 KiB（`0xFF0000..0xFF2000`）专供键盘配置。
为兼容 espflash 分区解析器，分区表使用 `data,nvs` 类型和 `kbdmap` 标签，
**内容是本例程的双槽记录，并非 ESP-IDF NVS 格式**，不能用 NVS API 访问。

每槽一个 4 KiB 扇区，包含版本、递增序号、校验和、键码和宏。
写另一槽并读回核验，保留旧槽，开机选择最新有效记录。停止编辑 1.5 秒、没有
按住按键且宏未运行时才保存；VIA 宏上传完成标志尚未清除时不会保存。
只访问已核验名称、偏移、长度和类型的分区，避免误写旧应用分区。
普通重新烧录不会主动清空这两个槽；VIA 重置命令可以恢复默认配置。

## 验证与当前状态

```powershell
cargo test --manifest-path tests/keyboard/Cargo.toml --target x86_64-pc-windows-msvc
python -m pip install hidapi
python tests/keyboard/via_probe.py
python tests/keyboard/via_probe.py --roundtrip
```

读测试检查版本、层数、宏容量、全部键码；roundtrip 临时修改 L3 最后一个键后恢复，
不发送键盘输入。持久化还需复位后重读；触摸坐标、按住/释放和界面需要板上验证。

2026-10-31：release 构建和 4 项主机测试通过。已烧录并自动启动，串口确认
`GT911 ready; storage=true restored=false generation=0`，屏幕 DPI 启动正常。
首次烧录遇到 espflash 的自定义分区子类型解析崩溃，已改用支持的分区类型。
后续已接好 USB-A 数据线并完成以下实机验证：

- Windows 正常识别 `CAFE:4013` 的键盘、Consumer Control、VIA 三个 HID 接口。
- VIA 版本、层数、宏容量和全部 32 个键码读取通过；临时改键、读回和恢复原键值通过。
- 写入完成后串口显示 `save ok generation=5`；复位启动显示
  `storage=true restored=true generation=5`，全部键值再次读取一致。
- 用户在记事本确认 COPY / PASTE 能将 `abc` 变为 `abcabc`，按下高亮、松手恢复正常。
- 本次直接验证 Raw HID 协议，尚未逐项验收 VIA 网页界面、全部宏及媒体键。

### 绘制卡顿修复

初版按像素直接写 PSRAM，重画阻塞协作任务，导致改键后的下一条 USB 请求超时。
改为内部 RAM 的 16 行条带绘制，按 32 位对齐批量复制到 PSRAM；条带间主动让出
执行权，保持 USB 收发。只重画可见键名或按下状态变化的区域，裁剪纯色填充范围。

同时将键盘例程的 CPU 从 HAL 默认 100 MHz 配置切换到 rev3.1 支持的 400 MHz
配置。相同页眉更新由约 5.5 秒降至 127～137 ms，实测单键按下/松手更新约
56～57 ms。该数字是本板串口计时结果，不代表所有画面的统一帧率。
上述数据来自当时的 RGB888 单缓冲版本；后续已改为下述 RGB565 双缓冲。

### RGB565、降低刷新率和双缓冲实测（2026-10-31）

键盘显存改为 16-bit 565，DSI 输出继续保持 RGB888，由桥接硬件转换。
此板现有 RGB/BGR 配置下，565 打包需要 B 占 bits 15:11、G 占 bits 10:5、
R 占 bits 4:0（小端）。最初使用相反红蓝顺序，青色变黄、蓝色变棕；
调整后用户确认颜色正常。其他显示例程仍用原 RGB888 配置。

`keyboard-low-refresh` 保持请求像素时钟 58 MHz、实际 60 MHz、HFP=52、
行周期约 14 µs，只把 VFP 从 24 增至 1023。VFP 寄存器宽度为 10 位，
总行数为 1280+4+10+1023=2317。16 个帧间隔测得平均 32416～32420 µs，
约 30.85 Hz；不是将应用刷新计时器调慢后声称降低了屏幕扫描率。

| 配置 | 单帧字节 | 平均扫描读取量（估算） |
| --- | ---: | ---: |
| 原 RGB888，54.19 Hz | 2,764,800 | 约 150 MB/s |
| RGB565，54.19 Hz | 1,843,200 | 约 100 MB/s |
| RGB565，30.85 Hz | 1,843,200 | 约 57 MB/s |

以上为有效像素读取量，不含总线额外开销。延长消隐减少平均流量、增加空闲时间，
有效区瞬时读取压力并不会按帧率比例下降。单缓冲阶段单键绘制从约 56 ms
降至 34～35 ms，但用户看到按下时出现横条纹。

随后改为两块 1,843,200 字节帧缓冲，总计 3,686,400 字节（约 3.52 MiB）。
CPU 只更新后台，commit 切换后先消耗可能积压的 VSYNC，再等待两个新帧边界，
防止 HAL 预取的旧 LLI 仍引用待复用缓冲。每次在后台补画上一轮脏区域，
避免轮换后旧高亮或键名回滚，不做每次全帧复制。

帧边界等待会增加更新延迟，因此不能再把单缓冲的 34 ms 当成最终完整更新耗时。
用户确认当前双缓冲版颜色、按下/松开、L0～L3 切换正常，无横条纹、旧高亮残留或闪烁。
VIA 改键读写测试通过。启动时 GT911 曾短暂 NACK，现有有界启动重试及后台重新探测，
日志确认能自动恢复；尚未确定触摸控制器偶发启动延迟的硬件原因。

桥接格式参考：[Espressif MIPI DSI bridge HAL](https://github.com/espressif/esp-idf/blob/master/components/esp_hal_lcd/esp32p4/include/hal/mipi_dsi_brg_ll.h)。

### 能否暂停 MIPI DMA、更新后再恢复（2026-10-31）

硬件 VDMA 支持暂停/恢复，但它与“屏幕安全地保持上一帧”不是同一件事。
当前 HX8394 使用 DSI Video Mode，由 P4 连续扫描 PSRAM。只暂停 VDMA 而让
DPI/DSI 继续输出，会使桥接 FIFO 面临欠载；暂停整个视频输出也不能据此保证
面板保持原画面，可能闪烁、黑屏或恢复时错位，需要实测同步恢复流程。

本地 esp-hal 1.2.0 的 `DsiDpi` 没有公开 `pause/resume`。其 Drop 会停 VDMA、
关闭 DPI、释放时钟并关闭 DPHY，并非轻量暂停。私有 VDMA start 会设置链表
地址并使能通道；不能把清除/设置通道使能直接当作经过验证的扫描恢复流程。

当前垂直消隐为 4+10+24=38 行，以约 14 µs 行周期估算仅 0.532 ms；
单键更新约 56 ms，不能仅靠等 VSYNC 后暂停 DMA，就把完整更新放进消隐期。

若目标是避免撕裂，优先考虑双缓冲：DMA 扫描前台帧，CPU 更新后台帧，
缓存写回后在安全帧边界切换，确认旧帧已不被 DMA 使用后才允许重写。
RGB888 两帧共 5,529,600 字节，约 5.27 MiB；双缓冲并不会消除 PSRAM 带宽竞争。
HAL 的 commit 修改 LLI 源地址，仍需核实硬件锁存边界和缓冲释放时机，
不能把任意时刻调用 commit 等同于已实现无撕裂翻帧。本轮仅核查与说明，未修改固件。

参考：[Espressif MIPI DSI 工作方式](https://github.com/espressif/esp-iot-solution/blob/master/docs/en/display/lcd/mipi_dsi_lcd.rst)、
[ESP32-P4 数据手册 VDMA 功能](https://documentation.espressif.com/esp32-p4_datasheet_en.html)，
以及本机 esp-hal 1.2.0 的 `mipi_dsi/dpi.rs`、`mipi_dsi/vdma.rs`。

协议参考：[VIA 规范](https://www.caniusevia.com/docs/specification/)、
[VIA V3 变更](https://www.caniusevia.com/docs/post_v3_changes/)、
[QMK Raw HID](https://docs.qmk.fm/features/rawhid)、
[QMK VIA 协议实现](https://github.com/qmk/qmk_firmware/blob/master/quantum/via.c)。
