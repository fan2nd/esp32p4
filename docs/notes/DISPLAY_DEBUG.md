# Display bring-up findings

## 2026-10-27：烧录后出现 Commands 提示的含义

`Flashing has completed!` 表示烧录流程完成；随后的 `Commands: CTRL+R / CTRL+C`
是 espflash 交互式串口监视器固定打印的快捷键提示，并不是等待输入的错误，
也不能单凭这两行证明程序没有运行或自动复位没有发生。
[espflash 4.6.0 监视器源码](https://github.com/esp-rs/espflash/blob/v4.6.0/espflash/src/cli/monitor/mod.rs)
在交互模式下无条件输出这些提示，并在 Ctrl+R 时调用复位函数。

当前 runner 的默认 after 是 hard-reset。对 CH343 USB-UART，
[4.6.0 reset.rs](https://github.com/esp-rs/espflash/blob/v4.6.0/espflash/src/connection/reset.rs)
的 hard-reset 和监视器 Ctrl+R 均使用 `reset_after_flash` 的非 USB-JTAG 分支，
即仅切换 RTS 并等待 100 ms；此分支不重新明确设置 DTR。
API 的 true/false 不能直接当作物理引脚电平，实际芯片 EN 和 GPIO35 取决于控制线和板载电路。

历史上 espflash 曾有 [Windows USB-JTAG 下载后不自动复位问题 #592](https://github.com/esp-rs/espflash/issues/592)，
已于旧版修复；这是不同接口、不同版本的报告，不能直接视为本板相同根因。
[PR #999](https://github.com/esp-rs/espflash/pull/999) 也涉及 Windows 进入下载时的控制线兼容，
该修改已在 4.5.0 发行记录中，但不等于 4.6.0 在当前 CH343/新板上没有复位问题。

本机之前对同一新板做过实测：默认 flash+monitor 和程序化发送 Ctrl+R 均未捕获启动日志；
关闭 espflash 串口后，使用 .NET 明确设置 DTR/RTS 并脉冲 RTS，捕获到了正常启动及 display ready。
这提示串口会话/控制线状态可能有关，但程序化 Ctrl+R 的按键是否被交互监视器收到未独立证实，
也没有 EN/GPIO35 波形，尚不能断言根因。用户现用默认 runner，测试工具和复位电路的变化需另行记录。
在真实终端按 Ctrl+R 后观察启动日志、按实体 RESET 对照，并检查上屏内容；
如果日志显示 SPI_FAST_FLASH_BOOT 则已进入 Flash 启动，若显示 DOWNLOAD 或 waiting for download
则在下载模式；完全无日志仍可能是复位、串口状态或日志时机问题，不能单凭空白定性。

## 升级 espflash

2026-10-27 检查：本机 `espflash` 是 4.6.0，路径为
`C:\Users\admin\.cargo\bin\espflash.exe`，由 Cargo 安装；该版本与当时
[官方最新发布](https://github.com/esp-rs/espflash/releases)一致。
`cargo-espflash` 是独立工具，本机为 4.3.0；当前 runner 调用的是 `espflash`。

以后发布新版后，可在 PowerShell 执行：

```powershell
cargo +stable install espflash --locked --target x86_64-pc-windows-msvc
espflash --version
Get-Command espflash -All | Select-Object Source
```

显式指定 Windows 主机 target 是为了避免工程 `.cargo/config.toml` 的 P4 裸机目标
与 PC 上运行的烧录工具混淆；项目 target 不需要更改。已有同版一般不必强制重装。
官方 4.6.0 从源码安装需要 rustc 1.95.0 或更新，本机查询为 1.99.0。
若未来报 Rust 版本过低，可先执行 `rustup update stable`，再重新执行安装命令；
更新 stable 也会影响本项目使用的 stable 工具链。

只需要 `cargo espflash` 子命令时才另行安装 `cargo-espflash`；升级该工具不会升级
本项目 runner 所调用的 `espflash`。升级不能保证解决现有复位问题，应重新验证。

2026-10-27 补充：已根据两块板官方原理图完成
[CH343 与自动复位电路对比](CH343_COMPARE.md)。两板 CH343P 与双晶体管拓扑相同，
旧板 EN 还并联 RESET 框内的 R40=10 kΩ、C67=100 nF；
新板未画出该额外支路。标称 EN RC 时间常数分别约 5.5 ms / 10 ms，
尚不能据此断定复位失败根因，需测量 DTR/RTS/EN/GPIO35 波形。

## 2026-10-27：新板自动复位实测与兼容处理

用户确认新旧板都用 CH343；新板下载前不需按键，下载后监视器仅显示
`Commands: CTRL+R / CTRL+C`，按实体 RESET 后才运行。

本机环境：Windows，espflash 4.6.0；COM6 为 CH343（VID 1A86 / PID 55D3），
驱动 1.6.2023.5（2023-05-10）；启动日志确认芯片 v3.1。

已完成的测试（未擦除分区或修改 eFuse）：

1. .NET SerialPort 显式设 DTR=false、RTS=false，再将 RTS=true 保持 100 ms、
   RTS=false，采集到 `SPI_FAST_FLASH_BOOT`、`display: dpi streaming`、
   `verify_display: display ready`。说明板上复位路径至少在这个状态下有效。
2. espflash board-info 可自动进下载模式、加载 stub，并调用 HardReset。
3. 用现有 verify_display ELF 执行 espflash flash --monitor，重现监视器无启动输出；
   发送 Ctrl+R 也没有采集到输出。
4. 改为 --after watchdog-reset 仍无启动输出。4.6.0 的 connection/mod.rs 中，
   P4 仅在 is_using_usb_otg 为真时调用 rtc_wdt_reset；CH343 UART 不适用，
   因此不能把该选项当成当前连接的有效替代方案。
5. 烧录后关闭 espflash 句柄，使用 .NET 明确设置控制线并复位，可采集到 bootloader 日志。
   如果再切换到 espflash monitor，未继续收到应用日志；保持同一 .NET 句柄监视则能完整启动。
   这支持排查 Windows/驱动/工具对串口控制线状态的处理，但不是已测得 EN 或 BOOT 的电平波形。
6. 曾新增 flash.ps1 并接入 Cargo runner：烧录用 --after no-reset-no-stub，随后重开串口、
   明确释放 DTR、输出 RTS 脉冲、保持同一串口句柄监视。脚本直跑一次与
   cargo run --release --bin verify_display 一次均无需实体按键，完整收到 display ready。
   验证结束后已退出监视器并释放串口。

当前结论：问题已缩小至默认 espflash 复位/串口状态路径与该新板组合的不兼容，
已验证兼容处理可以自动启动。不能认定 CH343 换型号、芯片 v3.1 缺陷、
复位电路损坏或某一具体 DTR 电平就是根因。新旧板的电路、驱动设备配置、EN/BOOT
实际波形尚未对比；若要确定最终电气根因，需要在失败时测量 EN 和 GPIO35。

按用户要求，runner 已恢复为 `espflash flash --monitor --chip esp32p4`，
不再调用 flash.ps1；该历史实验脚本目前不在项目中，原始 UART 监视不支持地址解码和 Ctrl+R。
显示初始化、时钟、Gamma 和帧缓冲未因 runner 恢复而修改。

源码依据：

- [espflash 4.6.0 reset.rs](https://github.com/esp-rs/espflash/blob/v4.6.0/espflash/src/connection/reset.rs)：
  CH343 非 USB-Serial/JTAG 分支仅输出 RTS=true / 100 ms / RTS=false。
- [espflash 4.6.0 connection/mod.rs](https://github.com/esp-rs/espflash/blob/v4.6.0/espflash/src/connection/mod.rs)：
  P4 watchdog-reset 的 USB-OTG 条件。

下面的默认配置说明也适用于恢复后的 runner。

## espflash 下载后自动复位（本机 4.6.0）

已核对本机 `espflash flash --help`：`--after` 默认是 `hard-reset`，
因此当前 runner `espflash flash --monitor --chip esp32p4` 会在烧录后尝试硬复位并运行程序，
随后打开串口监视。无需额外参数；需要明确写出时可用 `--after hard-reset`。
自动复位依赖所用下载接口与板上复位电路的连接；烧录成功不保证复位电路一定动作正常。
如果烧录后仍需手动 RESET，应核对串口/USB 接口、BOOT 按键是否松开及复位日志，
不能先认定是显示初始化卡住。该命令也支持 `--after watchdog-reset`，作为复位机制的另一个选项，
本项目尚未验证其在当前新板上的效果。本次只读取帮助，没有烧录或操作复位。

## 2026-10-27：更换 v3.1 主控板后深色背景偏红

### 色块反馈与修正候选

用户确认 RED 色块显示蓝色、BLUE 色块显示红色，证实当前显示路径的红蓝顺序相反。
这解释了原深蓝底色为何看起来偏红，但不能据此归因于芯片修订版本。
驱动现将 DCS MADCTL 从 `36 00` 改为 `36 08`，设置标准 BGR 位（bit 3），
保留 RGB888 帧缓冲布局、镜像/轴交换位、厂商表和全部时序。
这是面板侧修正候选，尚待重新烧录、确认 RED/GREEN/BLUE 标签与颜色相符；
如果面板在该视频模式下未采用此位，需要继续检查面板色序设置和主机数据打包。
前面的“尚未修改”描述记录的是此前排查阶段。

- 用户反馈换板后屏幕黑色背景有点红；已确认运行 `verify_display`，屏幕、排线、电源是否保持不变尚未确认。
- 当前 `.cargo/config.toml` 已设置最低芯片版本 `301`，runner 不再固定 `--min-chip-rev 3.2`。
  这是构建配置核对结果，不证明板上固件一定由当前配置重新构建烧录。
- 当前显示驱动仍是 58 MHz 请求值、PLL_F240M、HFP=52、RGB888，未切换至此前讨论的 60 MHz 候选配置。
- 反馈时 `verify_display` 的背景是 RGB `(0x12, 0x1C, 0x2C)`，即 `(18,28,44)` 深蓝色，并非纯黑。
  主程序 `main` 则用 `framebuffer.fill(0)` 输出 RGB `(0,0,0)`。
- 若红蓝通道互换，深蓝测试底色可能呈现偏红；但纯黑 `(0,0,0)` 交换通道仍然为黑，
  因此需要先确认画面入口，并比较纯黑、等值灰、红绿蓝测试色，不能仅凭深色背景判断通道顺序。
- 目前没有证据把偏红归因于芯片 v3.1，也没有修改 Gamma、VCOM、像素格式或时序。
  README 的 3.2 构建说明和 DISPLAY_INIT 的旧版本说明尚未随用户本地配置更新，阅读时以当前配置为准。
- 为区分通道顺序和黑电平偏色，已将 `verify_display` 改为纯黑背景，并添加带文字标签的
  RED/GREEN/BLUE/WHITE/GRAY32/GRAY128 色块；灰色各通道值相等。
  红蓝色块对调而灰阶正常支持红蓝通道顺序异常；纯黑或等值灰偏红则不能仅靠通道交换解释。
  此变更尚未烧录和取得视觉反馈，屏幕初始化参数保持原样。

## Latest verification

- User reports HFP 52 displays correctly; current `src/driver/display.rs` uses 52.
- The HFP 40/52 arithmetic and its relation to spatial corruption are explained
  in [DISPLAY_INIT.md](../hardware/DISPLAY_INIT.md). The observations and proposed test below
  are historical stages preceding that successful HFP 52 report; the complete
  A/B/A test and hardware timing readback are not recorded as completed.
- Four-lane feasibility checked against ESP-IDF:
  `components/esp_hal_lcd/esp32p4/include/hal/mipi_dsi_ll.h` defines
  `MIPI_DSI_LL_MAX_DATA_LANES 2`. `esp_lcd_mipi_dsi_bus.c` rejects lane
  counts above that value with `ESP_ERR_INVALID_ARG`. esp-hal 1.2.0
  likewise exposes only one or two lanes. No four-lane firmware was flashed;
  forcing a register value would not provide four physical data lanes.

- Panel: Waveshare 5-DSI-TOUCH-A, HX8394, 720 x 1280.
- Keep two DSI data lanes at 700 Mbps per lane. Waveshare component
  `esp_lcd_hx8394` 2.1.0 specifies this configuration; esp-hal 1.2.0
  `DataLanes` exposes only `_1` and `_2`. Four lanes are not an available
  software setting for this setup.
- RGB888 framebuffer uses three bytes per pixel. ESP-IDF
  `esp_lcd_panel_dpi.c` allocates width * height * bits_per_pixel / 8;
  DMA2D support does not imply four-byte pixels.
- User confirmed flicker stopped with PLL_F240M. Spatial corruption remains:
  approximately 15 mm of a 110 mm long edge is correct, followed by an
  estimated one-pixel displacement every four lines. Photo orientation does
  not establish panel scan orientation.
- Requested DPI clock 58 MHz is realized as 60 MHz (240 / 4). Current HAL
  compensation makes bridge HTOTAL 828; host HLINE is 1207 byte clocks at
  87.5 MHz. These imply 13.8 us and 13.794286 us per line, respectively,
  a difference of about 0.343 pixel clocks per line. This is a hypothesis
  to test, not a confirmed cause. No porch or lane settings changed here.
- Nominal bandwidth alone cannot exclude transient FIFO underrun. No
  underrun measurement has yet been captured.

## Proposed A/B/A timing check (not yet flashed)

Keep panel commands, framebuffer, lane count/rate and DPI clock unchanged.
Compare requested HFP 40 with 52, then restore 40 to test reproducibility.
With HFP 52 the HAL computes bridge HTOTAL 840 and host HLINE 1225:
840 / 60 = 1225 / 87.5 = 14 us. Integer cross-check:
840 * 175 == 1225 * 120. Only HFP changes, but this also lengthens the
line and lowers refresh rate slightly, so improvement alone does not prove
clock mismatch rather than reduced throughput pressure. Record whether
the correct region extends across the screen and whether diagonal drift
disappears. Stop serial monitoring at `app: display ready`; visual outcome
comes from the user. If unchanged, inspect FIFO underrun/overflow and DMA
status instead of continuing blind porch changes. Register readback is
needed to confirm calculated timing matches the hardware.

Sources: local esp-hal 1.2.0 `src/mipi_dsi/{mod,dpi,vdma}.rs`, Waveshare
component `include/esp_lcd_hx8394.h`, and Espressif ESP-IDF master commit
`4d59230ddff16327812782151ef0afef202dc6d7`,
`components/esp_hal_lcd/mipi_dsi_hal.c` and
`components/esp_lcd/dsi/esp_lcd_panel_dpi.c`.
