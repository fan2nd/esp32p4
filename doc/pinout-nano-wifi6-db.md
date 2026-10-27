# ESP32-P4-NANO-WIFI6-DB 板载外设引脚分配

整理日期：2026-10-27。对应当前新板型号，不是旧款 ESP32-P4-ETH。
旧板分配保留在 [pinout.md](pinout.md)。本文只整理硬件连接，不修改驱动。

## 整理依据和编号约定

- [微雪官方文档：ESP32-P4-NANO-WIFI6-DB](https://docs.waveshare.net/ESP32-P4-NANO-WIFI6-DB/)。
- [官方原理图](https://github.com/waveshareteam/ESP32-P4-NANO-WIFI6-DB/blob/main/hardware/schematics/ESP32-P4-NANO-WIFI6-DB.pdf)：本次下载并检查第 3 页电路图，核对排针、DSI/CSI、USB-UART 和复位电路。
- [官方仓库硬件指南](https://github.com/waveshareteam/ESP32-P4-NANO-WIFI6-DB/blob/main/docs/en/hw-reference/user-guide-esp32-p4-nano-wifi6-db.md)。
- 旧板对比依据本工程既有 `doc/pinout.md`，不是把旧板连接器编号套到新板。

除明确写出 C5 外，GPIO 编号均为 ESP32-P4 侧。DSI、CSI 和 USB HS
差分信号属于专用引脚，不是任意 GPIO；电源网络也不是 GPIO。
P1/P2 编号按原理图，每行奇数在左、偶数在右，不能按 GPIO 数值推断排针位置。
官方指南的实物朝向是 Type-C 在上、RJ45 在下，左侧为 P1、右侧为 P2。
接线仍应核对实物 pin 1 标记和观察面，避免背面视角造成镜像。

## 总览

| 板载资源 | ESP32-P4 连接 / 功能 |
| --- | --- |
| 共享 I2C | SDA GPIO7，SCL GPIO8 |
| Type-C USB-UART | CH343P；P4 TX=GPIO37、RX=GPIO38；DTR/RTS 经双晶体管接 EN/BOOT |
| USB HS OTG | Type-A J2，专用 USBD_P/USBD_N；同时引出到 P2 |
| TF/MicroSD | SDMMC D0～D3=GPIO39～42，CLK=43，CMD=44，电源控制=45 |
| 100M Ethernet | IP101GRI；GPIO28～31、34～35、49～52 |
| DSI 显示 | J1，15 针，2-lane D-PHY，加共享 I2C |
| CSI 摄像头 | J3，15 针，2-lane D-PHY，加共享 I2C/SCCB |
| 音频 | ES8311 + NS4150B；I2S GPIO9～13，功放使能 GPIO53 |
| ESP32-C5 无线协处理器 | SDIO GPIO14～19，EN GPIO54；另有 GPIO6 接 C5 IO2 |
| 扩展排针 | P1、P2，各 2×13，2.54 mm，共 52 针 |
| BOOT / RESET | BOOT=GPIO35，RESET=ESP_EN，按下接地 |
| 32.768 kHz 晶振 | GPIO0/XTAL_32K_N、GPIO1/XTAL_32K_P |
| RTC 电池 | H3，ESP_VBAT 供电路径；官方要求使用可充电 RTC 电池 |
| 电源指示灯 | LED1 接电源网络，不作为普通 GPIO LED 使用 |

## 共享 I2C

| 信号 | GPIO | 排针 | 连接 |
| --- | --- | --- | --- |
| ESP_I2C_SDA | GPIO7 | P1-3 | ES8311、外接屏背光/触摸、摄像头 SCCB |
| ESP_I2C_SCL | GPIO8 | P1-5 | 同上 |

| 设备 | 7-bit 地址 | 说明 |
| --- | --- | --- |
| ES8311 | 0x18 | 0x30 是 8-bit 写地址，不能直接当作 7-bit 地址传给 HAL |
| GT911 | 0x5D / 0x14 | 外接兼容触摸屏；RST、INT 未接 P4，使用轮询 |
| 屏幕背光控制器 | 0x45 | 官方文档明确亮度寄存器为 0x96，值 0～255 |
| 摄像头 SCCB | 由模块决定 | 不能把某一摄像头地址当成主板固定地址 |

总线共享，一个应用中应统一管理初始化和访问。CSI 原理图上 SCL/SDA
各有 2.2 kΩ 上拉；外接设备自带上拉时需要一起考虑。
官方资料确认 0x96 的背光用途，但不据此推断板控 0x95 每个位的功能。

## TF / MicroSD

| 功能 | SD1 接点 | GPIO / 网络 | 说明 |
| --- | ---: | --- | --- |
| D0 | 7 | GPIO39 | SDMMC 数据 0 |
| D1 | 8 | GPIO40 | SDMMC 数据 1 |
| D2 | 1 | GPIO41 | SDMMC 数据 2 |
| CD/D3 | 2 | GPIO42 | 4-bit 模式下的数据 3，不等同于独立卡检测 GPIO |
| CLK | 5 | GPIO43 | SDMMC 时钟 |
| CMD | 3 | GPIO44 | SDMMC 命令 |
| VDD | 4 | SD1_VDD | 经 Q1 AO3401 开关由 ESP_3V3 供电 |
| VSS | 6 | GND | 地 |
| 电源使能 | — | GPIO45 / P2-19 | **低电平打开** |

信号上拉接 ESP_LDO_VO4；GPIO39～48 的电压域和 LDO4 配置需要匹配。
P2-2 也引出了 ESP_LDO_VO4，不可把它当成固定 5 V 或独立普通 GPIO。

## Ethernet：IP101GRI RMII

官方配置 PHY 地址为 1，REF_CLK 为外部输入 50 MHz。

| 信号 | GPIO | P4 侧方向 / 用途 |
| --- | --- | --- |
| CRS_DV / RXDV | GPIO28 | 输入，载波/接收有效 |
| RXD0 | GPIO29 | 输入，接收数据 0 |
| RXD1 | GPIO30 | 输入，接收数据 1 |
| MDC | GPIO31 | 输出，管理时钟 |
| TXD0 | GPIO34 | 输出，发送数据 0 |
| TXD1 | GPIO35 | 输出，发送数据 1；同时是 BOOT/启动绑带信号 |
| TX_EN | GPIO49 | 输出，发送使能 |
| REF_CLK | GPIO50 | 输入，PHY 提供的 50 MHz 参考时钟 |
| PHY_RESET | GPIO51 | 输出，PHY 硬件复位 |
| MDIO | GPIO52 | 双向，PHY 管理数据 |

BOOT 与 TXD1 复用，外部电路和上电电平可能影响启动；不能只按普通发送数据脚处理。

## MIPI DSI：J1，15PIN

这是新板原理图的 **J1 15 针**接口；旧板 `pinout.md` 的 DSI J2 编号不适用。

| J1 针号 | 信号 | ESP32-P4 侧 |
| ---: | --- | --- |
| 1 | GND | 地 |
| 2 | DSI_D1_N | 专用 data lane 1 N |
| 3 | DSI_D1_P | 专用 data lane 1 P |
| 4 | GND | 地 |
| 5 | DSI_CLK_N | 专用 D-PHY 时钟 N |
| 6 | DSI_CLK_P | 专用 D-PHY 时钟 P |
| 7 | GND | 地 |
| 8 | DSI_D0_N | 专用 data lane 0 N |
| 9 | DSI_D0_P | 专用 data lane 0 P |
| 10 | GND | 地 |
| 11 | ESP_I2C_SCL | GPIO8 |
| 12 | ESP_I2C_SDA | GPIO7 |
| 13 | GND | 地 |
| 14 | ESP_3V3 | 3.3 V |
| 15 | ESP_3V3 | 3.3 V |

LCD_RST 没有独立 P4 GPIO；兼容屏背光由共享 I2C 0x45 控制。
GT911 的 TP_RST/TP_INT 未接 P4。驱动选择仍取决于实际接入的屏幕，
新板支持多种屏幕不意味着本项目 HX8394 参数可用于其他面板。

## MIPI CSI：J3，15PIN

| J3 针号 | 信号 | ESP32-P4 侧 / 说明 |
| ---: | --- | --- |
| 1 | GND | 地 |
| 2 | CSI_D0_N | 专用 data lane 0 N |
| 3 | CSI_D0_P | 专用 data lane 0 P |
| 4 | GND | 地 |
| 5 | CSI_D1_N | 专用 data lane 1 N |
| 6 | CSI_D1_P | 专用 data lane 1 P |
| 7 | GND | 地 |
| 8 | CSI_CLK_N | 专用 D-PHY 时钟 N |
| 9 | CSI_CLK_P | 专用 D-PHY 时钟 P |
| 10 | GND | 地 |
| 11 | CSI_IO0 | 原理图 R46 10 kΩ 上拉至 3.3 V，未见直连 P4 GPIO |
| 12 | CSI_IO1 | R47 标为 NC，未见直连 P4 GPIO |
| 13 | ESP_I2C_SCL | GPIO8，SCCB 时钟 |
| 14 | ESP_I2C_SDA | GPIO7，SCCB 数据 |
| 15 | ESP_3V3 | 3.3 V |

官方 BSP 不驱动摄像头 XCLK/RESET，不应把 CSI_IO0/1 自行指定成某个 P4 GPIO。

## Audio：ES8311 + NS4150B + MIC

| 功能 | GPIO | 连接 / 方向 |
| --- | --- | --- |
| I2S_MCLK | GPIO13 | P4 → ES8311 MCLK |
| I2S_SCLK | GPIO12 | P4 → ES8311 SCLK |
| I2S_WS | GPIO10 | P4 → ES8311 LRCK |
| I2S_DOUT | GPIO9 | P4 → ES8311 DSDIN，播放数据 |
| I2S_DIN | GPIO11 | ES8311 ASDOUT → P4，采集数据 |
| PA_CTRL | GPIO53 / P2-14 | NS4150B 使能，**高电平打开** |
| Codec 控制 | GPIO7 / GPIO8 | 共享 I2C |

方向以 P4 为准：Codec 的 DOUT 对应 P4 DIN，Codec DIN 对应 P4 DOUT。
板载 MIC 接模拟音频路径，不额外占 P4 数字 GPIO；扬声器接口为 H4。

## ESP32-C5 无线协处理器

P4 本身不提供无线射频；板载 C5 通过 ESP-Hosted SDIO 提供 Wi-Fi/蓝牙。

| 功能 | P4 GPIO | 连接 / 说明 |
| --- | --- | --- |
| SDIO D0 | GPIO14 | P4 ↔ C5 |
| SDIO D1 | GPIO15 | P4 ↔ C5 |
| SDIO D2 | GPIO16 | P4 ↔ C5 |
| SDIO D3 | GPIO17 | P4 ↔ C5 |
| SDIO CLK | GPIO18 | P4 → C5 |
| SDIO CMD | GPIO19 | P4 ↔ C5 |
| C5_CHIP_PU | GPIO54 / P2-13 | 高使能，低复位 |
| C5_IO2 | GPIO6 / P2-12 | 经 R18 0 Ω 接 C5 IO2，不当作无负载空闲 GPIO |
| C5_U0RXD | P2-20 | C5 专用下载 RX，不是 P4 GPIO38 |
| C5_U0TXD | P2-22 | C5 专用下载 TX，不是 P4 GPIO37 |
| C5_BOOT | P2-24 | C5 下载绑带信号，不是 P4 BOOT GPIO35 |

P4 应用与 C5 固件是两个独立烧录目标。GPIO14～19 和 GPIO54 相较旧板已增加板载用途。

## USB、UART 与自动下载/复位

| 接口 / 功能 | 信号 | P4 侧 / 原理图连接 |
| --- | --- | --- |
| Type-C H5 调试/下载 | CH343P TXD（U5-4） | GPIO38 / P4 UART0_RX |
| Type-C H5 调试/下载 | CH343P RXD（U5-5） | GPIO37 / P4 UART0_TX |
| 自动下载/复位 | CH343P DTR（U5-12）、RTS（U5-13） | 经 U6 EMH4T2R 双晶体管连接 ESP_EN、GPIO35 |
| EN 上拉/滤波 | R39=10 kΩ，C32=1 μF | ESP_EN 上拉至 ESP_3V3，电容接地 |
| BOOT 上拉/可选滤波 | R40=4.7 kΩ，C37=NC/100 nF | GPIO35 上拉；C37 实际是否贴装需看实物 |
| Type-A J2 | VBUS_OUT | 经 USB 电源开关供电 |
| Type-A J2 / P2-21 | USBD_P | P4 专用 USB HS DP |
| Type-A J2 / P2-23 | USBD_N | P4 专用 USB HS DM |
| P1-18、P1-19 | GPIO24、GPIO25 | 分别可复用 USB1P1_N0、USB1P1_P0 |
| P1-21、P1-22 | GPIO26、GPIO27 | 分别可复用 USB1P1_N1、USB1P1_P1 |

**CH343P 自身的 TX/RX 与 P4 的 TX/RX 方向相反。** Type-C 是 USB-UART，
不是 Type-A 的原生 USB HS 数据通道；P2 的 USBD_P/N 也不能与 GPIO24～27 的
USB1P1 复用信号混为一谈。

新板和旧板均为 CH343。新板原理图证明存在自动下载电路，但不能单凭器件型号
断言两板的复位瞬态一致。当前 Windows/espflash 自动复位实测及兼容处理见
[DISPLAY_DEBUG.md](DISPLAY_DEBUG.md)，尚未确认最终电气根因。

## 按键、晶振和电源指示

| 器件 | 连接 | 说明 |
| --- | --- | --- |
| BOOT / Key1 | GPIO35 | 按下接地，参与启动模式选择；与 RMII TXD1 复用 |
| RESET / Key2 | ESP_EN | 按下接地复位，不占普通 GPIO |
| 32.768 kHz 晶振 | GPIO0、GPIO1 | 分别 XTAL_32K_N/P，经 R32/R36；改普通 GPIO 前应处理晶振连接 |
| GPIO36 | P1-26 | 启动绑带脚，原理图有外部上拉 |
| LED1 | VCC_5V 电源支路 | 电源指示，不是可编程 GPIO LED |

## 扩展排针 P1（左侧，2×13）

| 奇数针 | 信号 | 偶数针 | 信号 |
| ---: | --- | ---: | --- |
| 1 | ESP_3V3 | 2 | VCC_5V |
| 3 | GPIO7 / SDA | 4 | VCC_5V |
| 5 | GPIO8 / SCL | 6 | GND |
| 7 | GPIO23 | 8 | GPIO37 / UART0_TX |
| 9 | GND | 10 | GPIO38 / UART0_RX |
| 11 | GPIO5 | 12 | GPIO4 |
| 13 | GPIO20 | 14 | GND |
| 15 | GPIO21 | 16 | GPIO22 |
| 17 | ESP_3V3 | 18 | GPIO24 / USB1P1_N0 |
| 19 | GPIO25 / USB1P1_P0 | 20 | GND |
| 21 | GPIO26 / USB1P1_N1 | 22 | GPIO27 / USB1P1_P1 |
| 23 | GPIO32 | 24 | GPIO33 |
| 25 | GND | 26 | GPIO36 / 启动绑带 |

## 扩展排针 P2（右侧，2×13）

| 奇数针 | 信号 | 偶数针 | 信号 |
| ---: | --- | ---: | --- |
| 1 | VCC_5V | 2 | ESP_LDO_VO4 |
| 3 | GND | 4 | GND |
| 5 | ESP_3V3 | 6 | GPIO0 / XTAL_32K_N |
| 7 | GND | 8 | GPIO1 / XTAL_32K_P |
| 9 | GPIO3 | 10 | GND |
| 11 | GPIO2 | 12 | GPIO6 / C5_IO2 |
| 13 | GPIO54 / C5_EN | 14 | GPIO53 / PA_CTRL |
| 15 | GPIO47 | 16 | GPIO48 |
| 17 | GPIO46 | 18 | GND |
| 19 | GPIO45 / SD 电源 | 20 | C5_U0RXD |
| 21 | USBD_P | 22 | C5_U0TXD |
| 23 | USBD_N | 24 | C5_BOOT |
| 25 | GND | 26 | GND |

## GPIO 占用和复用速查

本表覆盖 GPIO0～54；“引出”不等于“没有板载负载”。

| GPIO | 板载用途 / 扩展位置 |
| --- | --- |
| 0、1 | 32 kHz 晶振；P2-6、P2-8 |
| 2、3 | 扩展 P2-11、P2-9 |
| 4、5 | 扩展 P1-12、P1-11 |
| 6 | C5 IO2；P2-12 |
| 7、8 | 共享 I2C；P1-3、P1-5 |
| 9～13 | ES8311 I2S，顺序为 DOUT、WS、DIN、SCLK、MCLK |
| 14～17 | C5 SDIO D0～D3 |
| 18、19 | C5 SDIO CLK、CMD |
| 20、21、22、23 | 扩展 P1-13、P1-15、P1-16、P1-7 |
| 24、25、26、27 | 扩展 P1-18、P1-19、P1-21、P1-22；USB1P1 复用 |
| 28～31 | Ethernet CRS_DV、RXD0、RXD1、MDC |
| 32、33 | 扩展 P1-23、P1-24 |
| 34、35 | Ethernet TXD0、TXD1；35 同时为 BOOT |
| 36 | 启动绑带，P1-26 |
| 37、38 | UART0 TX、RX；P1-8、P1-10 |
| 39～44 | SD D0、D1、D2、D3、CLK、CMD |
| 45 | SD 电源使能低有效；P2-19 |
| 46、47、48 | 扩展 P2-17、P2-15、P2-16，属于 LDO4 电压域 |
| 49～52 | Ethernet TX_EN、REF_CLK、RESET、MDIO |
| 53 | 音频功放使能高有效；P2-14 |
| 54 | C5 使能/复位；P2-13 |

## 与旧 ESP32-P4-ETH 的主要差异

| 项目 | 旧板：ESP32-P4-ETH | 新板：ESP32-P4-NANO-WIFI6-DB |
| --- | --- | --- |
| I2C / 音频 / Ethernet / SDMMC 核心 GPIO | 见旧 pinout.md | 大部分保持相同；新板 SD 电源 GPIO45 明确低有效 |
| 扩展排针 | P1，2×20，40 针 | P1/P2，各 2×13，不能按旧针号接线 |
| DSI | J2，旧表按 22 针信号和屏蔽脚整理 | J1，15 针，接点顺序不同 |
| CSI | J1，旧表按 22 针信号和屏蔽脚整理 | J3，15 针，接点顺序不同 |
| 原生 USB HS | J3，4 针接口 | J2 Type-A，另并接 P2-21/23 |
| 无线协处理器 | 旧分配中没有板载 C5 | ESP32-C5；占 GPIO14～19、54，另连接 GPIO6 |
| GPIO0/1 | 旧表未列入 P1 | 新板接 32 kHz 晶振，并引出 P2 |
| GPIO36 | 测试点 TP5 | 引出至 P1-26，仍需注意启动绑带 |
| UART 桥 | CH343P | CH343P；新板复位电路为 U6 EMH4T2R |

当前 Rust 驱动使用的 I2C7/8、I2S9～13、PA53 和 RMII 分配可据此逐项对照。
显示偏色与芯片修订版本不应由 IO 表直接推断；面板色序的实测记录另见 DISPLAY_DEBUG。
官方不同页面存在 PSRAM 容量描述不一致，本文不据此判断容量；以芯片料号和实际启动检测为准。
