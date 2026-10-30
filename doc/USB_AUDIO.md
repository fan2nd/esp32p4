# P4+C5 USB 麦克风 + 扬声器

目标板：ESP32-P4-NANO-WIFI6-DB。固件 `usb_audio` 让 P4 成为电脑的
USB Audio Class 1.0 设备，使用板载 ES8311 模拟麦克风、H4 扬声器及 NS4150B
功放。不是 USB Host，不接外置 USB 声卡，C5 不参与音频传输。

## 接线先决条件

USB 数据走 **Type-A J2 的专用 USB HS D+/D−**；Type-C H5 仍用于供电、烧录和
UART 日志。不要用 GPIO24/25 的 USB FS 通道替代 J2。

本仓库官方原理图第 3 页：U2 DIO7003HEST5 的 IN 接 VCC_5V，OUT 接
VBUS_OUT/J2；EN 由 R2=10 kΩ 上拉，R11 下拉未装，没有软件控制 GPIO。
所以切到 USB Device 模式不会切断 J2 的 5V 输出。

**禁止使用 VBUS 连通的普通 A-A 线把 J2 直接连接电脑。** 实验连接必须用经
万用表确认 VBUS 隔断的数据线/转接板，保留 D+/D−/GND；板子由 Type-C 单独
供电。这里的“VBUS 隔断”不是切断数据的充电用 USB data blocker。
不要把 5V 直接接到 P4 GPIO，也不要用普通杜邦线长距离传输 USB HS。

当前为自供电实验固件，HAL 配置关闭 VBUS detection；板子没有接入电脑 VBUS
检测信号，因此不满足正式产品的 VBUS 检测要求。量产需改为合规的设备侧接口、
处理供电隔离和主机 VBUS 检测，并验证拔插行为。

## 构建和烧录

```sh
cargo build --release --bin usb_audio
cargo run --release --bin usb_audio
```

通过 Type-C 烧录，硬件接线核对完成后再连接 J2 数据通道。电脑应显示
`P4 USB Microphone + Speaker`，输入输出均选择 **16000 Hz、16-bit、单声道**。
先以低音量播放并录音验证；同时录放时麦克风会拾取板载扬声器，没有回声消除。

这是独立固件入口；原有默认显示/网络固件及 `verify_audio` 保持原样。
烧录 `usb_audio` 会替换板上原先固件，不会同时运行显示和网络功能。

## 实现

- 一个 AudioControl 接口、两个 AudioStreaming 接口，各有零带宽 alt 0 和
  传输 alt 1。播放 OUT、录音 IN、显式反馈 IN 共三个等时端点。
- 固定 16 kHz PCM16；采样率 GET_CUR/MIN/MAX/RES 和 SET_CUR 均验证请求。
  未宣告硬件音量/静音 Feature Unit，可在主机应用调整音量。
- I2S0 TX 产生 GPIO12 BCLK、GPIO10 WS、GPIO13 MCLK；RX 使用内部时钟共享
  `signal_loopback`，GPIO9 输出、GPIO11 输入；GPIO7/8 配置 ES8311。
- USB 单声道播放复制到两个 I2S 槽；录音取左槽。左槽是否匹配实际板上 ADC
  配置需上板确认，可结合 `verify_audio` 左右通道幅度日志定位。
- USB HS 每 125 us、FS 回退每 1 ms 调度；读取 DWC2 DSTS.ENUMSPD 选择节拍。
  播放反馈分别为 HS 16.16 / FS 10.14 格式，依据缓冲水位微调主机发送速率。
- 录音包长度根据缓冲水位调整，避免独立 USB/Codec 时钟漂移造成持续溢出。
  缓冲不足时补零；满时丢弃最旧样本，限制延迟。
- 功放在停止播放、总线暂停或 100 ms 未收到播放包时关闭。重置、取消配置、
  alt 切换清理缓存；暂停保留 alt 状态以允许恢复。
- RX 使用当前 HAL 的有限 EOF 机制，耗尽后重新启动，可能在边界丢样；连续
  录音质量必须实测。TX 欠载时记录日志并重启。此版本不是无丢样音频的承诺。
- 裸机单线程协作轮询，无 PSRAM 依赖；当前轮询占用 CPU，不适合作为现有网络
  executor 的阻塞任务直接调用。实验 VID/PID `CAFE:4011` 和固定序列号需在
  正式发布前替换为合法分配值与设备唯一序列号。

## 验证状态与步骤

已做交叉编译/链接和主机端控制逻辑单元测试，**尚未烧录、枚举或测试真实音频**。
单元测试可在主机运行（将 target 换成本机 `rustc -vV` 的 host）：

```sh
cargo test --manifest-path tests/usb_audio/Cargo.toml --target aarch64-apple-darwin
```

上板验收：

1. 确认 VBUS 隔断，再检查电脑是否同时列出输入、输出接口与 16 kHz 格式。
2. 单独录音、单独播放，再同时运行至少 10 分钟，检查持续噪声、周期性断音、
   速度漂移及 `TX DMA underrun` 日志。
3. 开关录音/播放、睡眠恢复、拔插数据线，检查没有旧音频重播且功放及时关闭。
4. 分别在 HS 与 FS 集线器下测试，检查包大小、反馈格式和系统兼容性。

参考：[Embassy UAC1](https://docs.embassy.dev/embassy-usb/0.5.1/default/class/uac1/index.html)、
[微雪板卡资料](https://docs.waveshare.com/ESP32-P4-NANO-WIFI6-DB)、
本仓库 `官方资料/ESP32-P4-NANO-WIFI6-DB-官方原理图及位图.pdf` 第 3 页。
