# ESP32-P4 点亮 MIPI 屏幕：当前实现总结

整理日期：2026-10-24。以当前 `src/driver/display.rs`、
`src/bin/apps/main.rs`、`src/bin/verify/verify_display.rs` 和本机 esp-hal 1.2.2
源码为依据；本次仅核对代码与记录，没有重新烧录或进行硬件测量。

## 屏幕和关键参数

| 项目 | 当前值 |
| --- | --- |
| 主控 / HAL | ESP32-P4，当前工程最低芯片版本 3.1（301）；esp-hal 固定 1.2.2 |
| 屏幕 | Waveshare 5-DSI-TOUCH-A，HX8394，720 × 1280 |
| 板控 I2C | I2C1；SDA GPIO7，SCL GPIO8；板控地址 `0x45`，速率使用 HAL 默认配置 |
| DSI | 2 条数据 lane，配置每 lane 700 Mbps；专用 D-PHY 引脚 |
| 虚拟通道 | DBI 命令和 DPI 视频均使用 VC 0 |
| 像素输出 | DPI 视频流；输入、输出均为 RGB888 |
| 帧缓冲 | PSRAM 单缓冲，720 × 1280 × 3 = 2,764,800 字节（约 2.637 MiB） |
| 内存布局 | 行优先，连续 R、G、B 三字节；每行 2160 字节；起始地址要求 64 字节对齐 |
| DMA | VDMA_CH0；启动后持续读取帧缓冲，DPI 句柄必须保持存活 |
| 像素时钟 | 请求 58 MHz；PLL_F240M，经 HAL 整数分频实际得到 60 MHz |
| 水平时序 | HActive=720，HSW=20，HBP=20，HFP=52；配置合计 812 像素时钟 |
| 垂直时序 | VActive=1280，VSW=4，VBP=10，VFP=24；合计 1318 行 |
| 原生绘图坐标 | 左上角 (0,0)，x 向右 0..719，y 向下 0..1279 |

## 实际执行顺序

1. 入口调用 `esp_hal::init`，初始化 PSRAM、I2C1，从 PSRAM 取得帧缓冲。
   主程序先把缓冲清零，因此默认输出黑画面；`verify_display` 则先绘制深色背景、
   坐标轴和文字。两者都在启动 DMA 前完成绘图。
2. 调用 `display::init`，向 I2C 地址 `0x45` 依次发送：
   `95 11` → `95 17` → `96 00` → 等待 **100 ms** → `96 FF` → 等待 **1000 ms**。
   这是板控寄存器序列；各位对应的供电、复位或背光电气功能尚未核实。
   当前实现没有直接操作独立的屏幕复位 GPIO。
3. 创建 `MipiDsi`，绑定 `MIPI_DSI` 和 `VDMA_CH0`，配置 **2 lane / 700 Mbps**，
   完成主机与 D-PHY 初始化。
4. 取得 `bus.dbi(0)`，发送前置 DCS 命令：
   `11` → 等待 **120 ms** → `36 08` → `3A 77` → `BA 61`。
   分别用于退出睡眠、地址/扫描模式、初始像素格式和初始面板 lane 设置。
5. 按源码 `HX8394_VENDOR_INIT` 的顺序发送 19 条厂商配置，每条后等待 **1 ms**：
   `B9 → B1 → BA → B2 → B4 → D3 → D5 → D6 → E0 → CC → C0 → B6 → D4 → BD → B1 → BD → BF → 3A → B2`。
   涉及扩展寄存器解锁、电源、MIPI、面板驱动时序、GIP 扫描、Gamma、VCOM 和寄存器 bank。
   完整字节以 `src/driver/display.rs` 的常量表为准；下文列出各命令长度和含义。
6. 再发送一次 `11` → 等待 **200 ms** → `29` → 等待 **80 ms**。
   这是当前保留的两次 sleep-out 流程；`29` 打开面板显示。
7. `drop(dbi)` 释放命令接口对总线的借用；检查帧缓冲长度及 64 字节对齐。
   调用 `bus.dpi(...)` 配置下表时序和 RGB888，交给 DMA 持续输出像素，
   返回 DPI 与 I2C 句柄。
8. 调用方保持 `_display` 存活。主程序随后初始化以太网任务；
   `verify_display` 进入等待循环，保留静态测试画面。

上述显式等待合计 **1519 ms**，另加外设初始化、I2C/DSI 传输、绘图与串口日志耗时。
`29` 开屏和 DPI 开始传输是两个独立步骤；当前没有独立的 PWM 背光初始化或亮度 API。

## 58 MHz 配置与实际行周期

本地 HAL `src/mipi_dsi/dpi.rs` 的处理是：

- 分频取 `round(240 / 58) = 4`，实际 DPI 时钟为 **60 MHz**。
- 主机 lane byte clock 按配置计算为 `700 / 8 = 87.5 MHz`。
- 主机 HLINE 为 `round(812 × 87.5 / 58) = 1225` 个 byte clock。
- Bridge 补偿后的 HTOTAL 为 `round(812 × 60 / 58) = 840`，对应 HFP=80。
  **源码配置的 HFP 仍然是 52**，80 是 HAL 计算出的 bridge 值。
- 两侧计算行周期相等：`840 / 60 MHz = 1225 / 87.5 MHz = 14 μs`。
  据此推算刷新率约为 `1 / (14 μs × 1318) = 54.19 Hz`。

这些是按当前源码推导的值，未做本次硬件寄存器回读或时钟测量。
## 为什么请求 58 MHz 却得到 60 MHz，HFP=52 如何确定

当前 HAL 路径只设置整数分频，没有为该请求配置小数分频：
`div = floor(240 / 58 + 0.5) = 4`，所以实际时钟是 `240 / 4 = 60 MHz`。
这属于配置频率的量化结果，不是晶振漂移，也不是测量得到的 60 MHz。
相同源的五分频是 48 MHz，无法通过这条整数分频路径精确得到 58 MHz。

HFP 是每行有效像素之后的前肩空白时间，以像素时钟为单位。
改变它不改变 720 像素宽度、RGB888 内存布局或 DMA 行跨度，但会改变总行长和刷新率。

关键在于 HAL 的两个换算分别取整。设配置总行长为 N，则：

```text
N = 720 + 20 + 20 + HFP = 760 + HFP
Bridge 总行长 B = round(N × 60 / 58)，单位：实际 60 MHz 像素时钟
Host 总行长   L = round(N × 87.5 / 58)，单位：87.5 MHz lane byte clock
两侧行周期分别为 B / 60 和 L / 87.5，单位：μs
```

| 配置 HFP | N | Bridge B | Host L | Bridge 行周期 | Host 行周期 | 行周期差折合 60 MHz 像素时钟 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 40（此前对照值） | 800 | 828 | 1207 | 13.800000 μs | 13.794286 μs | 0.342857 / 行 |
| 52（当前值） | 812 | 840 | 1225 | 14.000000 μs | 14.000000 μs | 0 / 行 |

52 有明确的算术依据：`60/58 = 30/29`，`87.5/58 = 175/116`。
选择 N 为 116 的倍数，可让两次换算都不需要舍入。
从原来的 N=800 往上找，最近的是 `116 × 7 = 812`，因此
`HFP = 812 - 760 = 52`。这是当前时钟组合下方便的对齐候选值，
并不说明 52 是面板唯一允许的值，也不说明所有其他值都不可能在舍入后对齐。
该计算值不是实测画面位移，不能据此断言可见像素错位量。
实际是否发生欠载、溢出、重同步及其如何表现，仍需状态寄存器或测量证据。

因此，现有证据支持“消除两侧行周期舍入差是修复错位的强候选解释”，
尚不足以证明唯一根因。HFP 从 40 增至 52 同时使行周期增加约 1.5%、
刷新率降低约 1.5%，也可能改善 FIFO 吞吐余量。
HFP=52 不会把实际像素时钟从 60 MHz 改回 58 MHz；它对齐的是两侧的整行时间。

## 如果直接配置 60 MHz，HFP 是否仍需 52

可以请求 60 MHz，此时 `240 / 60 = 4`，请求与实际频率一致。
之前保留 58 MHz 是沿用屏幕示例的请求值；这不代表硬件必须请求 58 MHz。
但更改请求值也会更改 HAL 的 Host 行时序换算和 Bridge 前肩补偿，
即使实际 DPI 时钟同为 60 MHz，整行时间仍会变化。

请求 60 MHz 时，Bridge 无需补偿，总行长就是 `N = 760 + HFP`；
Host 总行长为 `round(N × 87.5 / 60) = round(N × 35 / 24)`。
两侧精确对齐要求 N 为 24 的倍数，所以 **HFP=52 不再具有精确对齐的作用**。

| 请求时钟 / HFP | Bridge 总行长 | Host 总行长 | Bridge 行周期 | Host 行周期 |
| --- | ---: | ---: | ---: | ---: |
| 58 MHz / 52（当前） | 840 | 1225 | 14.000000 μs | 14.000000 μs |
| 60 MHz / 40 | 800 | 1167 | 13.333333 μs | 13.337143 μs |
| 60 MHz / 52 | 812 | 1184 | 13.533333 μs | 13.531429 μs |
| 60 MHz / 56 | 816 | 1190 | 13.600000 μs | 13.600000 μs |
| 60 MHz / 80 | 840 | 1225 | 14.000000 μs | 14.000000 μs |

因此，60 MHz / HFP=56 是接近原来 HFP=52 配置数值的对齐候选，
但相较当前已反馈正常的组合，它会缩短行周期并提高刷新率，不能仅靠算术保证面板稳定。
如果目的是消除请求值与实际值不一致，同时保留当前计算出的行周期，
**60 MHz / HFP=80** 是更直接的候选：Bridge HFP 同为 80，总行长同为 840，
Host HLINE 同为 1225，推算刷新率仍约 54.19 Hz。

这仍不是所有寄存器完全相同的替换：Host HSA/HBP 的换算从
`round(20 × 87.5 / 58) = 30` 变为 `round(20 × 87.5 / 60) = 29`，
因此仍需硬件验证。本文只分析候选参数，没有修改驱动或烧录。

## 当前实现中需要保留的细节

- `3A 77` 随后被厂商表中的 `3A 50` 覆盖，不能据此把主机帧缓冲改成 RGB565；
  当前 HAL 的输入、输出和内存布局都明确使用 RGB888。`50` 在该面板 MIPI 模式下的精确含义未核实。
- `BD 01 → B1 00 → BD 00` 是切换 bank 后写 B1，再切回 bank 0，
  不能把两次 B1 当成同一个 bank 下的覆盖写入。
- 当前末尾顺序是 `3A 50 → B2(12 字节) → 11 → 200 ms → 29 → 80 ms`。
  既有对照记录中，微雪 2.1.0 把第二次 B2 放在 `11 + 200 ms` 之后；本次不调整固件顺序。
- 下方旧参考中的外部驱动命名和对照结论沿用已有项目记录，本次未重新在线核验。

## 验证入口

```powershell
cargo run --release --bin verify_display
```

此命令会通过工程配置的 espflash runner 烧录并打开串口监视；本次总结没有执行它。
## HX8394 initialization command reference

This describes the current src/driver/display.rs, not a proposed new sequence.
Command names are cross-checked against the Waveshare esp_lcd_hx8394 2.1.0
driver and Linux drivers/gpu/drm/panel/panel-himax-hx8394.c:
https://raw.githubusercontent.com/torvalds/linux/master/drivers/gpu/drm/panel/panel-himax-hx8394.c
No complete panel-specific register bit definitions were verified.

## Board control over I2C

The four writes target address 0x45, not HX8394 DCS registers:

| Write | Verified behavior / limit of knowledge |
| --- | --- |
| 95 11 | First board-control configuration write from Waveshare. Individual output bits have not been verified against a schematic. |
| 95 17 | Second board-control configuration write. Do not infer a particular reset or supply signal without the board register map. |
| 96 00 | Write zero to board-control register 96, followed by 100 ms delay. Whether this is brightness or another control requires the board register map. |
| 96 FF | Write FF to the same register, followed by 1000 ms delay. Do not infer its electrical function from the values alone. |

## DCS and vendor commands in current execution order

| Command | Meaning |
| --- | --- |
| 11, wait 120 ms | Exit sleep; allow panel internal circuitry to stabilize. Does not itself stream pixels. |
| 36 08 | MADCTL/address mode: set BGR bit 3, keeping mirror and row/column exchange bits clear. Changed from 00 on 2026-10-27 after RED/BLUE patches were reported swapped; hardware confirmation of this correction is pending. |
| 3A 77 | Set pixel format. Standard encoding selects 24-bit RGB888 for both interface format fields; later overwritten by 3A 50. |
| BA 61 | Initial panel MIPI configuration. Waveshare selects this byte for two lanes. Host lane configuration is separate. |
| B9 FF 83 94 | SETEXTC: unlock extended HX8394 registers. |
| B1, 10 bytes, bank 0 | SETPOWER: panel power-related parameters. Exact voltages cannot be decoded from this table alone. |
| BA, 6 bytes | SETMIPI: full panel-side MIPI interface settings, including the initial 61 byte. |
| B2, 6 bytes | SETDISP: panel display-control parameters. These are not the ESP32 DPI porch registers. |
| B4, 21 bytes | SETCYC: panel driving-cycle parameters. |
| D3, 33 bytes | SETGIP0: gate-in-panel scan-control settings. |
| D5, 44 bytes | SETGIP1: gate-in-panel configuration, including panel scan/output mapping. Exact assignments require panel documentation. |
| D6, 44 bytes | SETGIP2: companion gate-in-panel configuration. |
| E0, 58 bytes | SETGAMMA: grayscale/gamma drive curve settings. Not framebuffer pixel data or a backlight brightness setting. |
| CC 0B | SETPANEL: panel characteristic configuration. Individual bit meanings not verified here. |
| C0 1F 73 | Vendor setting; Linux labels C0 UNKNOWN1. Preserve supplied bytes pending authoritative bit definitions. |
| B6 6B 6B | SETVCOM: common-electrode voltage parameters. Exact voltages require the register transfer function. |
| D4 02 | Vendor setting; Linux labels D4 UNKNOWN3. |
| BD 01 | SETREGBANK: switch to register bank 1. |
| B1 00, bank 1 | Write B1 in bank 1; this does not simply replace bank-0 SETPOWER parameters. Exact bank-1 field not verified. |
| BD 00 | Return to register bank 0. |
| BF, 7 bytes | Vendor setting; Linux labels BF UNKNOWN5. |
| 3A 50 | Overwrite the earlier pixel-format byte. This is not the ordinary RGB888 77 encoding. Standard field interpretation has RGB/DPI field 5 and DBI field 0; actual meaning in this panel's MIPI mode needs HX8394 documentation. Do not infer the DMA buffer layout from this byte alone. |
| B2, 12 bytes | Second SETDISP write: repeat the first six values with six additional values. |
| 11, wait 200 ms | Second exit-sleep command, retained from Waveshare's vendor sequence. |
| 29, wait 80 ms | Display on. Separate from backlight control and from starting host DPI streaming. |

Then drop(dbi) releases the Rust bus borrow; it is not a panel command.
The framebuffer was already filled by the caller; bus.dpi starts continuous pixel transmission.

## Known sequence discrepancy

Current Rust tail: 3A 50 -> B2 (12 bytes) -> 11 -> wait 200 ms -> 29.
Waveshare 2.1.0 tail: 3A 50 -> 11 -> wait 200 ms -> B2 (12 bytes) -> 29.
These orders are not equivalent by inspection. This reference records the
difference; no firmware changes were made as part of this explanation.
Rust also adds 1 ms after ordinary vendor commands whose table delay is 0.
