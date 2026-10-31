# P4+C5 USB 麦克风 + 扬声器

## Windows 麦克风手动测试

1. 保持当前 USB Audio 固件和两根 USB 线连接，先暂停电脑播放，避免扬声器声音
   被麦克风重新拾取。
2. 打开 Windows“设置 → 系统 → 声音 → 输入”，选择
   `麦克风 (P4 USB Microphone + Speaker)`，对板载麦克风说话，观察输入电平。
   如果格式可选，使用单声道、16-bit、16000 Hz。
3. 在 Windows“录音机/声音录制器”中选择该输入设备；若应用没有设备选择项，
   将 P4 麦克风设为默认输入，再关闭并重新打开应用。录制约 5～10 秒语音。
4. 停止录音后，用电脑自身扬声器或耳机回放，检查语音是否清晰、有无静音、
   断续或变速。这样可将麦克风问题与板载扬声器播放问题分开验证。
5. 若无法开始录音，检查 Windows 麦克风隐私权限以及应用的麦克风访问权限。
   若已开始录音但无输入电平，检查接口是否启用，并结合实际电平和回放判断。

扬声器已完成听音验证；麦克风尚待上述实测确认。

目标板：ESP32-P4-NANO-WIFI6-DB。固件 `usb_audio` 让 P4 成为电脑的
USB Audio Class 1.0 设备，使用板载 ES8311 模拟麦克风、H4 扬声器及 NS4150B
功放。不是 USB Host，不接外置 USB 声卡，C5 不参与音频传输。

当前使用 **UAC1 全速模式（12 Mbit/s）**，仍连接 J2 的 HS 控制器/PHY，
没有改接 GPIO24/25。16 kHz、16-bit、单声道双向有效数据合计仅 64 kB/s。
高速 UAC1 在本机 Windows 上能枚举但不能正常播放，因此本例程不宣称支持
高速音频；后续高速实现需要单独验证协议、反馈格式与等时传输。

## 接线先决条件

USB 数据走 **Type-A J2 的专用 USB HS D+/D−**；Type-C H5 仍用于供电、烧录和
UART 调试。不要用 GPIO24/25 的 USB FS 通道替代 J2。

本仓库官方原理图第 3 页：U2 DIO7003HEST5 的 IN 接 VCC_5V，OUT 接
VBUS_OUT/J2；EN 由 R2=10 kΩ 上拉，R11 下拉未装，没有软件控制 GPIO。
所以切到 USB Device 模式不会切断 J2 的 5V 输出。

隔断是为了避免 J2 的板载 5 V 输出与电脑另一 USB 口的 5 V 输出并接：
两侧电压及上电、断电时序可能不同，可能产生电流或触发保护。控制器切换到
Device 模式只改变 USB 通信角色，不等于切断板上的电源开关。隔断 VBUS 不是
为了 USB 音频协议或提升识别率；是否会出现问题取决于实际电源和保护电路。
两路电源来自同一台电脑时，风险通常小于独立电源，但各 USB 口可能经过独立
开关和走线，仍不能仅据此确认并联无影响。DIO7003 数据手册说明它具有反向
电压保护；输出高于输入约 175 mV 持续约 4 ms（典型值）时关断，检测期间仍
可能有反向电流。因此本板并非完全没有保护，也不能仅靠该保护保证所有状态安全。
详见 [DIO7003 数据手册](https://www.dioo.com/diooAdmin/api/upload/img/1666944942721.pdf)。
当前隔断线方案只用于实验，程序无法感知主机 VBUS；正规自供电设计应单独检测主机 VBUS。

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
  配置需上板确认，可使用 `verify_audio` 检查左右通道幅度。
- USB 每 1 ms 调度，名义每包 16 个采样；UAC1 播放反馈为 3 字节 10.14
  格式，依据缓冲水位微调主机发送速率。
- 录音包长度根据缓冲水位调整，避免独立 USB/Codec 时钟漂移造成持续溢出。
  缓冲不足时补零；满时丢弃最旧样本，限制延迟。
- 功放在停止播放、总线暂停或 100 ms 未收到播放包时关闭。重置、取消配置、
  alt 切换清理缓存；暂停保留 alt 状态以允许恢复。
- RX 使用当前 HAL 的有限 EOF 机制，耗尽后重新启动，可能在边界丢样；连续
  录音质量必须实测。TX 欠载时重启。此版本不是无丢样音频的承诺。
- 裸机单线程协作轮询，无 PSRAM 依赖；当前轮询占用 CPU，不适合作为现有网络
  executor 的阻塞任务直接调用。实验 VID/PID `CAFE:4011` 和固定序列号需在
  正式发布前替换为合法分配值与设备唯一序列号。

## 验证状态与步骤

固件构建、USB 枚举与扬声器播放验证通过。
麦克风录音、双工及长时间稳定性尚未验收，不能用扬声器测试结果代替。

### DAC 增益

ES8311 DAC 音量寄存器为 `0x32`。
按 [Espressif ES8311 驱动的增益范围](https://github.com/espressif/esp-adf/blob/release/v2.x/components/esp_codec_dev/device/es8311/es8311.c)，
寄存器值与增益关系为 `-95.5 + 0.5 × value` dB：`0xFF` 为 +32 dB，
约 39.8 倍幅度增益，并不是 0 dB 的满量程设置。普通 PCM 因此容易在 DAC
路径削波。初始化改为 `0xBF`（0 dB），电脑音量继续由主机控制。
此修复也影响共用该初始化的 `verify_audio`；不改变麦克风增益。
DAC 设为 0 dB 后的播放已通过听音验证。
高音量下的扬声器、功放和供电失真仍需听音验证，不能仅凭寄存器修正认定消失。

修复内容（均为 Rust）：

- 裸机 `block_on` 入口显式开启全局中断。
- 原 HAL 的 RX FIFO 额外预留为 31 个 32 位字，不足以覆盖 P4 的 16 个硬件
  OUT 端点。按控制器开销计算改为至少 `15 + 2 × 16 = 47` 个字，再加各 OUT
  端点数据空间。纯 EP0 诊断版的 RX FIFO 从 47 增至 63 个字。
- EP0 最后一个 IN 数据包发出前，重新使能接收零长度 OUT 状态包，避免后续
  控制传输因 OUT 端点未重装而停住。
- OUT 数据包消费后重新设置 EPENA；处理错过微帧的等时 IN/OUT，避免端点
  永久忙碌；关闭 OUT 端点前先完成全局 OUT NAK，在中断屏蔽期间主动处理
  RX FIFO 状态，防止 Windows 反复切换 alt setting 时卡死。
- 例程限制 HS PHY 协商全速，反馈端点改为 UAC1 的 3 字节格式。仅高速枚举
  成功并不意味着高速音频已实现。

通过 Cargo patch 固定使用 `3rd/embassy-usb-synopsys-otg`，不修改全局 Cargo
缓存，也不引入 C。补丁说明和许可证在该目录；FIFO 分配回归测试通过。
单元测试可在主机运行（将 target 换成本机 `rustc -vV` 的 host）：

```sh
cargo test --manifest-path 3rd/usb-audio-host-tests/Cargo.toml --target x86_64-pc-windows-msvc
```

上板验收：

1. 核对实际供电与 USB 数据接线，再检查电脑是否同时列出输入、输出接口与 16 kHz 格式。
2. 单独录音、单独播放，再同时运行至少 10 分钟，检查持续噪声、周期性断音、
   速度漂移及 DMA 欠载。
3. 开关录音/播放、睡眠恢复、拔插数据线，检查没有旧音频重播且功放及时关闭。
4. 在不同 USB 口/集线器下测试全速设备的包大小、反馈和系统兼容性。

Windows 定向播放测试（Python 标准库，不改变系统默认音频设备）：

```sh
python 3rd/usb-audio-host-tests/playback.py
```

它向 P4 输出 3 秒、5% 幅度的 440 Hz 测试音。主机完成通知仍需配合实际听音，
不能只凭脚本退出判定扬声器正常。

### 最小枚举诊断

新增 `src/bin/verify/verify_usb.rs`，只运行相同的 USB HS 驱动和一个 vendor 接口，
不启动 ES8311、I2S 或音频任务。测试 VID/PID 为 `CAFE:4012`；即使 Windows
没有对应类驱动，只要能读出此 VID/PID，就说明设备描述符枚举已成功。

vendor 接口没有类驱动时可显示错误状态，这与设备描述符读取失败不同。

参考：[Embassy UAC1](https://docs.embassy.dev/embassy-usb/0.5.1/default/class/uac1/index.html)、
[微雪板卡资料](https://docs.waveshare.com/ESP32-P4-NANO-WIFI6-DB)、
本仓库 `docs/reference/ESP32-P4-NANO-WIFI6-DB-官方原理图及位图.pdf` 第 3 页。
