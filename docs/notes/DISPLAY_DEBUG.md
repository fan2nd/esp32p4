# 显示与自动复位技术说明

## 下载接口与复位

当前 runner 使用 `espflash flash --monitor --chip esp32p4`。espflash 4.6.0
默认 `--after hard-reset`，CH343 非 USB-JTAG 路径通过 RTS 脉冲尝试复位。
软件控制值不等于物理 EN/GPIO35 电平，实际行为取决于接口、驱动和板载电路。
烧录成功不保证自动复位成功；必要时松开 BOOT 并按实体 RESET。

若要确定自动复位电气根因，应对照测量 DTR/RTS/EN/GPIO35，不能只凭软件现象
判断芯片修订缺陷或电路损坏。CH343 连接不满足该版本 P4 watchdog-reset 的
USB-OTG 条件，不能直接将其当作替代复位方案。

两板电路与 RC 计算见 [CH343 与自动复位电路对比](CH343_COMPARE.md)。
旧板标称 EN 时间常数约 5.5 ms，新板约 10 ms；此差异本身不证明故障原因。

来源：[espflash 4.6.0 reset.rs](https://github.com/esp-rs/espflash/blob/v4.6.0/espflash/src/connection/reset.rs)、
[connection/mod.rs](https://github.com/esp-rs/espflash/blob/v4.6.0/espflash/src/connection/mod.rs)。

## 烧录工具

`espflash` 与 `cargo-espflash` 是独立工具；本项目 runner 调用前者。
安装或更新时显式使用 Windows 主机 target，避免继承工程裸机 target：

```powershell
cargo +stable install espflash --locked --target x86_64-pc-windows-msvc
espflash --version
Get-Command espflash -All | Select-Object Source
```

工具升级不能代替板上复位验证。版本与工具链要求应查阅
[官方发布说明](https://github.com/esp-rs/espflash/releases)。

## 色序、格式与数据通道

使用纯黑、等值灰和带标签的红绿蓝色块区分色序与黑电平问题。
红蓝通道互换不会改变纯黑或等值灰；深蓝背景偏红不能独立证明黑电平异常。
DCS MADCTL 的标准 BGR 位为 bit 3，但须同时核对面板配置、主机像素打包和桥接格式。

Waveshare 5-DSI-TOUCH-A 使用 HX8394、720×1280；保持两条 DSI 数据通道，
每通道 700 Mbit/s。ESP-IDF 的 `MIPI_DSI_LL_MAX_DATA_LANES` 为 2，HAL
不支持四通道配置。RGB888 每像素三字节，帧大小为宽 × 高 × 3；DMA2D
支持不意味着帧缓冲应改为每像素四字节。

## 时序核对

像素时钟请求 58 MHz，由 240 MHz 时钟整数分频得到实际 60 MHz。
HFP=52 时计算的 HTOTAL 为 840、HLINE 为 1225 个 87.5 MHz 字节时钟：

```text
840 / 60 = 1225 / 87.5 = 14 µs
840 × 175 = 1225 × 120
```

HFP=40 时相应值为 828 与 1207，计算行周期并不完全一致。算术详见
[显示初始化说明](../hardware/DISPLAY_INIT.md)。这只是时序核对，寄存器回读
才可确认硬件实际配置；标称带宽也不能排除瞬时 FIFO 欠载。
改变 porch 同时改变行长与刷新率，不能将改善单独归因于时钟一致性。

## 验证结论与限制

HFP=52 已获得画面正常的实机反馈；完整时序 A/B/A 对照与寄存器回读尚未完成。
未验证四通道配置；自动复位电气根因仍未确定。视觉结果不能代替 DMA/FIFO
状态与供电波形测量。

参考：Waveshare `esp_lcd_hx8394` 2.1.0；
[ESP-IDF MIPI DSI HAL](https://github.com/espressif/esp-idf/tree/master/components/esp_hal_lcd/esp32p4/include/hal)；
[ESP-IDF LCD DSI 实现](https://github.com/espressif/esp-idf/tree/master/components/esp_lcd/dsi)。
