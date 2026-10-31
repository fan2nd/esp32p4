# ESP32-P4-ETH 板载外设引脚分配

本文件对应旧板。新板 ESP32-P4-NANO-WIFI6-DB 的分配、P1/P2 逐针表和差异对照见
[pinout-nano-wifi6-db.md](pinout-nano-wifi6-db.md)。

整理依据：

- 本地文件：[ESP32-P4-ETH 官方原理图及位图](../reference/ESP32-P4-ETH-官方原理图及位图.pdf)。
- 微雪 wiki：`https://www.waveshare.com/wiki/ESP32-P4-ETH`。
- 微雪产品页：`https://www.waveshare.com/esp32-p4-eth.htm`。

说明：

- PDF 文件名是 `datasheet.pdf`，但内容是 ESP32-P4-ETH 原理图和 PCB 布局；布局页丝印标注 `Rev1.1`。
- 表中 `GPIOxx` 均为 ESP32-P4 侧编号；USB、DSI、CSI 的高速差分脚是专用管脚，不按普通 GPIO 编号。
- 多个板载外设共用或占用的 GPIO 已在备注中标出。

## 总览

| 板载资源 | ESP32-P4 连接 |
| --- | --- |
| 默认 I2C 总线 | SDA `GPIO7`，SCL `GPIO8` |
| Type-C USB-UART 调试/下载 | CH343P `TXD -> GPIO38`，`RXD -> GPIO37`，DTR/RTS 参与 `ESP_EN`/`GPIO35` 自动下载 |
| 原生 USB HS OTG 4pin 接口 `J3` | `USBD_N`/`USBD_P` 连接 ESP32-P4 专用 `DM`/`DP` 管脚 |
| 40pin 扩展排针 USB1P1 | `GPIO24/USB1P1_N`，`GPIO25/USB1P1_P` |
| TF/MicroSD | `GPIO39` 到 `GPIO45` |
| 100M Ethernet, IP101GRI | `GPIO28`、`GPIO29`、`GPIO30`、`GPIO31`、`GPIO34`、`GPIO35`、`GPIO49`、`GPIO50`、`GPIO51`、`GPIO52` |
| MIPI DSI 屏幕接口 `J2` | 2-lane DPHY + I2C `GPIO7/GPIO8` |
| MIPI CSI 摄像头接口 `J1` | 2-lane DPHY + I2C `GPIO7/GPIO8` |
| ES8311 Codec + NS4150B 功放 + MIC | I2S `GPIO9` 到 `GPIO13`，PA_CTRL `GPIO53`，控制 I2C `GPIO7/GPIO8` |
| BOOT 按键 | `GPIO35`，按下拉低 |
| RESET 按键 | `ESP_EN`，按下复位 |
| Power LED | 电源指示，不接软件 GPIO |

## 默认 I2C 总线

| 信号 | GPIO | 连接/用途 |
| --- | --- | --- |
| `ESP_I2C_SDA` | `GPIO7` | DSI/CSI 连接器、ES8311 Codec 控制、40pin 排针 `P1-17` |
| `ESP_I2C_SCL` | `GPIO8` | DSI/CSI 连接器、ES8311 Codec 控制、40pin 排针 `P1-16` |

CSI 区域原理图上还给这两根线放了 `2.2K` 上拉；DSI、CSI、音频 Codec 使用同一组板级 I2C 时，地址和上拉要一起考虑。

## TF / MicroSD

| TF 卡功能 | TF1 管脚 | ESP32-P4 GPIO/网络名 | 备注 |
| --- | ---: | --- | --- |
| `D0` | 7 | `GPIO39` | SDMMC 数据线 |
| `D1` | 8 | `GPIO40` | SDMMC 数据线 |
| `D2` | 1 | `GPIO41` | SDMMC 数据线 |
| `CD/D3` | 2 | `GPIO42` | SDMMC `D3`，也可作为卡检测相关脚 |
| `CLK` | 5 | `GPIO43` | SDMMC 时钟 |
| `CMD` | 3 | `GPIO44` | SDMMC 命令 |
| `VDD` | 4 | `SD1_VDD` | 由 Q1 电源开关从 `ESP_3V3` 供电 |
| 电源开关控制 | - | `GPIO45` | 接 Q1 gate，原理图默认下拉 `10K` |

`GPIO39` 到 `GPIO44` 的上拉电阻接到 `ESP_LDO_VO4`，这也是 ESP32-P4 的 `GPIO39~GPIO48` 电压域。调 SDMMC 高速/电压切换时，需要确认这个电压域的配置。

## Ethernet: IP101GRI RMII

| RMII/PHY 功能 | ESP32-P4 GPIO | 备注 |
| --- | --- | --- |
| `TXD0` | `GPIO34` | RMII 发送数据 0 |
| `TXD1` | `GPIO35` | RMII 发送数据 1；同一 GPIO 也接 BOOT 按键 |
| `TX_EN` | `GPIO49` | RMII 发送使能 |
| `RXD0` | `GPIO29` | RMII 接收数据 0 |
| `RXD1` | `GPIO30` | RMII 接收数据 1 |
| `CRS_DV / RXDV` | `GPIO28` | RMII 载波/接收数据有效 |
| `REF_CLK` | `GPIO50` | 50 MHz reference clock，PHY 外接 25 MHz 晶振并倍频产生 |
| `MDC` | `GPIO31` | PHY 管理接口时钟 |
| `MDIO` | `GPIO52` | PHY 管理接口数据 |
| `RESET` | `GPIO51` | IP101GRI 复位控制 |

原理图上 `TXER`、`COL`、`RXER`、`CRS` 等还作为 PHY strap/可选网络出现；ESP-IDF 侧最关键的是上表这些 RMII/MDC/MDIO/RESET 引脚。

## MIPI DSI 屏幕接口 J2

| J2 管脚 | 信号 | ESP32-P4 侧 |
| ---: | --- | --- |
| 1 | `ESP_3V3` | 3.3 V |
| 2 | `ESP_I2C_SDA` | `GPIO7` |
| 3 | `ESP_I2C_SCL` | `GPIO8` |
| 13 | `DSI_CLK_P` | 专用 DSI DPHY 时钟 P |
| 14 | `DSI_CLK_N` | 专用 DSI DPHY 时钟 N |
| 16 | `DSI_D1_P` | 专用 DSI DPHY data lane 1 P |
| 17 | `DSI_D1_N` | 专用 DSI DPHY data lane 1 N |
| 19 | `DSI_D0_P` | 专用 DSI DPHY data lane 0 P |
| 20 | `DSI_D0_N` | 专用 DSI DPHY data lane 0 N |
| 23, 24 | Shield/GND | 外壳/屏蔽地 |

ESP32-P4 芯片侧的 DSI 专用管脚：`DSI_DATAP0`、`DSI_DATAN0`、`DSI_CLKP`、`DSI_CLKN`、`DSI_DATAP1`、`DSI_DATAN1`、`DSI_REXT`。`DSI_REXT` 通过 `4.02K 1%` 电阻到地。

## MIPI CSI 摄像头接口 J1

| J1 管脚 | 信号 | ESP32-P4 侧/备注 |
| ---: | --- | --- |
| 1 | `ESP_3V3` | 3.3 V |
| 2 | `ESP_I2C_SDA` | `GPIO7` |
| 3 | `ESP_I2C_SCL` | `GPIO8` |
| 4 | `CSI_IO1` | 原理图只看到连接器/可选上拉网络，未看到直连 ESP32-P4 GPIO |
| 5 | `CSI_IO0` | `10K` 上拉到 `ESP_3V3`，未看到直连 ESP32-P4 GPIO |
| 13 | `CSI_CLK_P` | 专用 CSI DPHY 时钟 P |
| 14 | `CSI_CLK_N` | 专用 CSI DPHY 时钟 N |
| 16 | `CSI_D1_P` | 专用 CSI DPHY data lane 1 P |
| 17 | `CSI_D1_N` | 专用 CSI DPHY data lane 1 N |
| 19 | `CSI_D0_P` | 专用 CSI DPHY data lane 0 P |
| 20 | `CSI_D0_N` | 专用 CSI DPHY data lane 0 N |
| 23, 24 | Shield/GND | 外壳/屏蔽地 |

ESP32-P4 芯片侧的 CSI 专用管脚：`CSI_DATAP0`、`CSI_DATAN0`、`CSI_CLKP`、`CSI_CLKN`、`CSI_DATAP1`、`CSI_DATAN1`、`CSI_REXT`。`CSI_REXT` 通过 `4.02K 1%` 电阻到地。

## Audio: ES8311 + NS4150B + MIC

| 音频功能 | ESP32-P4 GPIO | 连接 |
| --- | --- | --- |
| `MCLK` | `GPIO13` | ES8311 master clock |
| `SCLK` | `GPIO12` | ES8311 I2S bit clock |
| `ASDOUT / DOUT` | `GPIO11` | ES8311 到 ESP32-P4 的 I2S 数据 |
| `LRCK / WS` | `GPIO10` | I2S word select |
| `DSDIN / DIN` | `GPIO9` | ESP32-P4 到 ES8311 的 I2S 数据 |
| `PA_CTRL` | `GPIO53` | NS4150B 功放使能，高电平有效 |
| Codec I2C `CDATA/CCLK` | `GPIO7/GPIO8` | ES8311 控制接口 |

板载 MIC 接 ES8311 的模拟麦克风通道；扬声器座 `H6` 接 NS4150B 输出，不直接占用额外 GPIO。

## USB

| 接口/功能 | 信号 | ESP32-P4 侧 |
| --- | --- | --- |
| Type-C USB-UART | CH343P `TXD` | `GPIO38` |
| Type-C USB-UART | CH343P `RXD` | `GPIO37` |
| 自动下载/复位 | CH343P `DTR/RTS` | 通过 U5 控制 `ESP_EN` 和 `GPIO35` |
| 4pin USB HS OTG `J3-1` | `VCC_5V` | 5 V |
| 4pin USB HS OTG `J3-2` | `USBD_N` | ESP32-P4 专用 `DM` |
| 4pin USB HS OTG `J3-3` | `USBD_P` | ESP32-P4 专用 `DP` |
| 4pin USB HS OTG `J3-4` | `GND` | GND |
| 40pin 排针 USB1P1 | `USB1P1_N/P` | `GPIO24/GPIO25` |

CH343P 的 `TXD/RXD` 是桥芯片侧方向；写固件时若按 ESP32-P4 UART 方向理解，通常是 `GPIO37` 做 TX、`GPIO38` 做 RX。

## 按键和指示灯

| 板上器件 | 连接 | 备注 |
| --- | --- | --- |
| `RESET` / `Key2` | `ESP_EN` | 按下拉低复位，`10K` 上拉到 `ESP_3V3` |
| `BOOT` / `Key1` | `GPIO35` | 按下拉低进入下载模式；与 Ethernet `TXD1` 共用 |
| `LED1 PWR` | 电源指示 | 不接软件 GPIO |

## 40pin 扩展排针 P1

原理图标注为 `PICO-2*20 IO`。这张表按 P1 管脚编号列出，不代表这些 GPIO 都空闲；其中 I2C、USB1P1、ESP_EN 等会与板载功能复用。

| P1 | 信号 | P1 | 信号 |
| ---: | --- | ---: | --- |
| 1 | `GPIO54` | 21 | `GPIO48` |
| 2 | `GPIO19` | 22 | `GPIO47` |
| 3 | `GND` | 23 | `GND` |
| 4 | `GPIO18` | 24 | `GPIO46` |
| 5 | `GPIO17` | 25 | `GPIO33` |
| 6 | `GPIO16` | 26 | `GPIO32` |
| 7 | `GPIO15` | 27 | `GPIO27` |
| 8 | `GND` | 28 | `GND` |
| 9 | `GPIO14` | 29 | `GPIO26` |
| 10 | `GPIO6` | 30 | `ESP_EN` |
| 11 | `GPIO5` | 31 | `GPIO23` |
| 12 | `GPIO4` | 32 | `GPIO22` |
| 13 | `GND` | 33 | `GND` |
| 14 | `GPIO3` | 34 | `GPIO21` |
| 15 | `GPIO2` | 35 | `GPIO20` |
| 16 | `ESP_I2C_SCL` / `GPIO8` | 36 | `ESP_3V3` |
| 17 | `ESP_I2C_SDA` / `GPIO7` | 37 | `3V3_EN` |
| 18 | `GND` | 38 | `GND` |
| 19 | `USB1P1_N` / `GPIO24` | 39 | `VCC_5V` |
| 20 | `USB1P1_P` / `GPIO25` | 40 | `VCC1_5V` |

另外，`GPIO36` 在原理图中引出到测试点 `TP5`，不在 P1 表内。

## 占用/复用提醒

| GPIO/接口 | 现有连接 |
| --- | --- |
| `GPIO7/GPIO8` | 板级默认 I2C，总线连接 DSI、CSI、ES8311 和 P1 |
| `GPIO9~GPIO13` | ES8311 I2S |
| `GPIO24/GPIO25` | P1 上的 `USB1P1_N/P` |
| `GPIO28~GPIO31` | Ethernet RMII/管理接口 |
| `GPIO34/GPIO35` | Ethernet TXD0/TXD1；`GPIO35` 同时接 BOOT |
| `GPIO37/GPIO38` | CH343P USB-UART |
| `GPIO39~GPIO45` | TF/MicroSD；`GPIO45` 控 SD 电源开关 |
| `GPIO49~GPIO52` | Ethernet TX_EN/REF_CLK/RESET/MDIO |
| `GPIO53` | 音频功放 `PA_CTRL` |
| `GPIO54` | P1 扩展排针 pin 1 |
