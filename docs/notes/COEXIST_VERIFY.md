# 显示与无线共存验证

缓存维护跨核互斥修复及依据见 [缓存同步说明](SDIO_MIPI_CACHE_FIX.md)。

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

组合模式使用主核 Embassy executor 执行时间驱动与无线任务。
主核先初始化 PSRAM，再启动显示副核；显示与无线无需额外启动等待。
画面更新循环、颜色、分辨率和同步 VSYNC 沿用独立成功版本；主核不写显存。

以下 `COMx` 须替换为实际板卡串口。

```powershell
cargo build --release --bin display_core1 --features verify-radio
espflash flash --port COMx --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/display_core1
```

## 验证结论与边界

独立对照确认缓存维护互斥对显示与无线共存是必要修复；动态显示、Wi-Fi 扫描及 BLE 空口扫描已通过组合验证。
静态或关闭显示模式用于隔离负载，不代表完整桌面已通过验证。
USB、音频、触摸及长期复合负载稳定性必须独立验收，不能仅以无线初始化成功推定。
