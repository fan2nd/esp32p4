# 8 键触屏宏键盘

`keyboard` 是 ESP32-P4 的独立 Rust 例程，使用现有 720×1280 MIPI 屏幕、
GT911 触摸和 USB-A 数据口；运行它会替换当前 USB Audio 应用。没有引入 C 实现。

## 布局与功能

屏幕为 **2 列 × 4 行的 8 个大按键**，每键 312×198 像素。顶部 L0～L3
直接切换四层，不占用可改键位置。按下高亮，松开释放，VIA 改键同步更新键名。

所有层的默认键位均为 `KC_NO / 0x0000`，8 个宏也为空；空键只保留按钮外观，
不显示 `NONE`，不向电脑发送按键。通过 VIA 分配功能后显示对应名称。
已有 Flash 改键配置保持不变；新设备、VIA 重置或桌面“重置设置”后采用空白默认值。

支持普通 HID 按键、Ctrl/Shift/Alt/GUI 组合、常用媒体键、MO/TG/TO/DF 层切换、
透明键及 8 个 VIA 宏（合计 512 字节）。宏支持 US ANSI ASCII 文本、按下/松开/
点击和延时；单次延时上限 10 秒。新的触摸按键会取消正在执行的宏。
宏执行期间不合并其他已按住的实体触摸键；结束后恢复当前触摸状态。

这不是完整 QMK 实现：不支持 Mod-Tap、Layer-Tap、Tap Dance、鼠标键、RGB 菜单、
Unicode 宏或全部扩展 QMK 键码。未知键码会保留并显示十六进制，但不会发送。
同时最多 5 个触点；USB 键盘报告为 6KRO。GT911 超过 250 ms 无有效报告会释放键，
以避免总线故障后一直按住。按住时改键不会更换当前已按下的键码，松开后再生效。

## 构建与烧录

以下 `COMx` 须替换为实际板卡串口。

```powershell
cargo build --release --bin keyboard --features keyboard-low-refresh
espflash flash --port COMx --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/keyboard
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
python -m pip install hidapi
python tools/via_probe.py
python tools/via_probe.py --roundtrip
```

读测试检查版本、层数、宏容量、全部键码；roundtrip 临时修改 L3 最后一个键后恢复，
不发送键盘输入。持久化还需复位后重读；触摸坐标、按住/释放和界面需要板上验证。

固件 release 构建通过，不代替当前硬件验收。VIA 网页界面、全部宏与媒体键仍需逐项验收；
触摸控制器偶发启动延迟的硬件原因尚未确定。

### 绘制策略

使用内部 RAM 条带绘制、32 位对齐批量写入 PSRAM，并在条带间让出执行权，
保持 USB 收发；只更新受影响区域。键盘例程在 rev3.1 上使用 400 MHz CPU。

### RGB565、降低刷新率和双缓冲

键盘显存改为 16-bit 565，DSI 输出继续保持 RGB888，由桥接硬件转换。
此板现有 RGB/BGR 配置下，565 打包需要 B 占 bits 15:11、G 占 bits 10:5、
R 占 bits 4:0（小端）。

`keyboard-low-refresh` 保持请求像素时钟 58 MHz、实际 60 MHz、HFP=52、
行周期约 14 µs，只把 VFP 从 24 增至 1023。VFP 寄存器宽度为 10 位，
总行数为 1280+4+10+1023=2317，扫描频率约 30.85 Hz；
这是面板时序配置，不是应用刷新计时器。

| 配置 | 单帧字节 | 平均扫描读取量（估算） |
| --- | ---: | ---: |
| 原 RGB888，54.19 Hz | 2,764,800 | 约 150 MB/s |
| RGB565，54.19 Hz | 1,843,200 | 约 100 MB/s |
| RGB565，30.85 Hz | 1,843,200 | 约 57 MB/s |

以上为有效像素读取量，不含总线额外开销。延长消隐减少平均流量、增加空闲时间，
有效区瞬时读取压力并不会按帧率比例下降。

使用两块 1,843,200 字节帧缓冲，总计 3,686,400 字节（约 3.52 MiB）。
CPU 只更新后台，commit 切换后先消耗可能积压的 VSYNC，再等待两个新帧边界，
防止 HAL 预取的旧 LLI 仍引用待复用缓冲。每次在后台补画上一轮脏区域，
避免轮换后旧高亮或键名回滚，不做每次全帧复制。

帧边界等待会增加端到端更新延迟；不能将绘制耗时等同于完整更新耗时。
GT911 使用有界启动重试与后台重新探测。

桥接格式参考：[Espressif MIPI DSI bridge HAL](https://github.com/espressif/esp-idf/blob/master/components/esp_hal_lcd/esp32p4/include/hal/mipi_dsi_brg_ll.h)。

### 能否暂停 MIPI DMA、更新后再恢复

硬件 VDMA 支持暂停/恢复，但它与“屏幕安全地保持上一帧”不是同一件事。
当前 HX8394 使用 DSI Video Mode，由 P4 连续扫描 PSRAM。只暂停 VDMA 而让
DPI/DSI 继续输出，会使桥接 FIFO 面临欠载；暂停整个视频输出也不能据此保证
面板保持原画面，可能闪烁、黑屏或恢复时错位，需要实测同步恢复流程。

本地 esp-hal 1.2.0 的 `DsiDpi` 没有公开 `pause/resume`。其 Drop 会停 VDMA、
关闭 DPI、释放时钟并关闭 DPHY，并非轻量暂停。私有 VDMA start 会设置链表
地址并使能通道；不能把清除/设置通道使能直接当作经过验证的扫描恢复流程。

默认 VFP=24 时，垂直消隐为 4+10+24=38 行，以约 14 µs 行周期估算仅 0.532 ms；
不能假设完整更新能放进消隐期。

若目标是避免撕裂，优先考虑双缓冲：DMA 扫描前台帧，CPU 更新后台帧，
缓存写回后在安全帧边界切换，确认旧帧已不被 DMA 使用后才允许重写。
RGB888 两帧共 5,529,600 字节，约 5.27 MiB；双缓冲并不会消除 PSRAM 带宽竞争。
HAL 的 commit 修改 LLI 源地址，仍需核实硬件锁存边界和缓冲释放时机，
不能把任意时刻调用 commit 等同于已实现无撕裂翻帧。暂停/恢复方案尚未完成硬件验证。

参考：[Espressif MIPI DSI 工作方式](https://github.com/espressif/esp-iot-solution/blob/master/docs/en/display/lcd/mipi_dsi_lcd.rst)、
[ESP32-P4 数据手册 VDMA 功能](https://documentation.espressif.com/esp32-p4_datasheet_en.html)，
以及本机 esp-hal 1.2.0 的 `mipi_dsi/dpi.rs`、`mipi_dsi/vdma.rs`。

协议参考：[VIA 规范](https://www.caniusevia.com/docs/specification/)、
[VIA V3 变更](https://www.caniusevia.com/docs/post_v3_changes/)、
[QMK Raw HID](https://docs.qmk.fm/features/rawhid)、
[QMK VIA 协议实现](https://github.com/qmk/qmk_firmware/blob/master/quantum/via.c)。

## VIA 网页宏缓冲末包兼容（2026-11-01）

网页报 `Receiving incorrect response for command` 时，已在实机复现：
`0E 01 DC 1C`（offset=476）正常，`0E 01 F8 1C`（offset=504、length=28）
返回 `FF`。原因是固件拒绝了超过 512 字节宏缓冲末尾的读取，而
[VIA 官方客户端 getMacroBytes](https://github.com/the-via/app/blob/main/src/utils/keyboard-api.ts)
固定每次请求 28 字节，最后一包仍然如此。

读取命令现在保持命令、偏移、长度原样回显，只复制有效范围，越界尾部补零；
写入继续拒绝越界与超过 28 字节的请求，不做部分写入。未修改宏容量、布局 JSON、
VID/PID，也无需为此重新制作 VIA 定义。现有简短探针没有覆盖读取完整宏缓冲的路径，
因此只通过探针不代表网页初始化全部成功。

修复版已烧录。手动硬件检查按网页流程连续读取 19 包，每包命令/偏移/长度回显
均正确；末包返回 8 字节有效数据及 20 字节零填充。网页交互结果待用户确认。

空白默认值实机验证：VIA 键位重置后读回全部 32 个键码均为 0；随后恢复原用户键位，未修改 Wi-Fi、其他设置或宏。
