# 显示与无线共存逐步验证（2026-10-31）

**后续已定位并修复缓存维护跨核互斥缺失，见 [根因、官方依据与 A/B 验证](SDIO_MIPI_CACHE_FIX.md)。**
下文保留修复前实验过程，不代表当前固件仍有相同错误。

目标：以单独验证成功的 `display_core1` 为底座，先加 Wi-Fi/BLE，再决定是否
继续加入 USB/音频。本轮已在第一步复现 SDIO 异常，因此未继续增加其他外设。

## 实现

Wi-Fi/BLE 工作负载从 `src/bin/demos/wifi_ble.rs` 原样移入 `src/task/wifi_ble.rs`，
独立例程和组合例程共用同一实现，避免复制协议实现造成差异。
`display_core1` 默认保持原独立刷屏功能，可用以下 feature 选择对照组：

| feature | 显示行为 | 无线行为 |
| --- | --- | --- |
| 无 | 原移动色块、同步 VSYNC、双缓冲 | 无 |
| verify-radio | 持续重绘及 DMA 扫描 | Wi-Fi 扫描 + BLE 广播 |
| verify-static | 完成两帧后停止 CPU 重绘/commit，保持 DPI/VDMA | 同上 |
| verify-stop-display | 完成两帧后 drop DPI，关闭 VDMA/DSI | 同上 |

组合模式使用主核 Embassy executor 执行时间驱动、统一日志、无线任务和心跳。
主核先初始化 PSRAM，再启动显示副核。排查中曾等待副核两帧后配置 SDIO；
修复后已去掉该等待，最终版同时启动显示和无线。
画面更新循环、颜色、分辨率和同步 VSYNC 沿用独立成功版本；主核不写显存。

```powershell
cargo build --release --bin display_core1 --features verify-radio
espflash flash --port COM6 --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/display_core1
```

切换 feature 后必须重新构建再烧录，输出 ELF 路径相同。`stage=6` 为静态扫描，
`stage=7` 为显示已关闭，这两组 `frames=2` 不再增长是预期行为。

## 观察结果

1. 显示与无线同时启动：副核帧数持续增长，主核每 2 秒心跳正常，但 C5 private
   INIT 后多次 `sdio: rx failed: Io`，未观察到无线完整初始化。
2. 等显示完成两帧再启动无线：仍反复出现 SDIO Io 错误，另出现 `bad checksum`；
   Wi-Fi 初始化和版本读取曾成功，BLE `0x2008` 设置广播数据超时，之后报告
   `HeartbeatStopped`。每 2 秒画面更新约 13～14 帧，未出现两核同时停住。
3. 停止 CPU 重绘，保留 DPI/VDMA：Wi-Fi 初始化和 BLE 广播设置成功，但随后
   仍反复出现 SDIO Io 错误。因此 CPU 全屏写入/commit 不是错误出现的必要条件。
4. 关闭 DPI/VDMA：Wi-Fi 初始化、版本读取、BLE 广播设置成功；一轮 Wi-Fi 扫描
   返回 16 个 AP，观察窗口内没有 SDIO Io 或 checksum 错误。

这里的 BLE 广播成功指控制器命令完成；本轮关闭显示对照组未另外做电脑端空口
扫描。此前独立 wifi_ble 的空口广播已验证，不能用此前结果代替本轮验证。

## 结论与边界

显示扫描开启与 SDIO 异常有明确关联，且无需 USB、音频或触摸参与即可复现。
但本轮没有复现原 desktop 的“纯红屏且通信停住”全部症状，不能认定两者根因相同。
也不能把结果简单等同于 PSRAM 带宽不足：显示 DMA、中断中的描述符维护、
缓存操作、总线仲裁及时钟/电源变化仍需区分。

检查本地 HAL 可见 VDMA ISR 会写回描述符缓存，SDIO 也有缓存维护路径；它们
是否存在并发问题尚未证实。曾尝试链接器 wrap 缓存入口做互斥对照，但 ELF
符号显示 ROM 链接脚本覆盖了 wrapper，实验没有实际启用保护，已删除该实现，
不将其结果用于排除或确认缓存冲突。

随后为 verify-radio 启用 esp-hal/log-04，已获取更具体错误：首先出现
`DmaError`，上层对应 `sdio: rx failed: Io`，继而出现大量 `Timeout`。
HAL 将 SDHOST IDMAC 的 FBE（总线致命错误）或 DU（描述符不可用）均映射为
DmaError，当前日志尚未区分这两个状态。下一步应抓取 IDSTS 原始位及描述符
状态，再对 DMA/缓存/总线提出单变量修正；先不继续叠加 USB/音频。

恢复动态组合版后，用户确认黄色方块正常移动、颜色正常。最后板上是启用 HAL
日志的动态 verify-radio 版本（338,272 字节），仍可复现 SDIO DMA 错误，未宣称
共存已修复。串口监视退出以释放 COM6。
