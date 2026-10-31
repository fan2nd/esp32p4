# microSD USB 读卡器

`desktop_tiny_ui` 与 `desktop` 共用复合 USB 实现：LUN 0 保留 1 MiB VIA
定义只读盘，LUN 1 为可读写的板载 microSD 卡，不增加接口或端点。
卡内文件系统由电脑管理；固件不挂载文件系统、不自动格式化，也不把设备配置写入卡。
插卡后电脑可显示第二个磁盘；移除卡或 USB 线前先等待复制完成并安全弹出。

## 硬件与实现

NANO-WIFI6-DB 的 SDMMC slot 0 使用 CLK=GPIO43、CMD=GPIO44、D0～D3=GPIO39～42，
GPIO45 低电平供电。SD 卡运行 20 MHz；C5 Wi-Fi 继续使用 slot 1。
两个槽共用控制器，由 esp-hal 的异步锁串行化传输，不绕过现有缓存互斥机制。

`src/driver/sd_card.rs` 使用 Rust `sdio` 与 `block-device-driver`，512 字节对齐缓冲，
读写通过 SDMMC DMA；BOT 每个 USB 包后让出执行权。写成功须等待卡完成编程，
无固件写回缓存。无专用卡检测线，通过主机请求驱动限频探测，I/O 失败撤销介质，
后续请求重新初始化。无卡初始化有超时，不无限阻塞 VIA 盘。

SCSI 为每个 LUN 保留独立 sense、介质变化和弹出状态；支持 READ/WRITE
6/10/12/16、容量查询和同步缓存。VIA 盘始终写保护，SD 卡拒绝越界访问。
USB 仍强制全速，bulk 包 64 字节；当前逐扇区写入偏重并发稳定性，不代表卡的极限速度。

## 实机验证（2026-11-01）

- P4 v3.1 使用 921600 烧录 `desktop_tiny_ui`，Windows 同时挂载 VIA 盘与 SD 卡。
- 本次卡容量 64,088,965,120 字节，原有 exFAT 正常识别，没有格式化。
- 两次各 1 MiB 随机临时文件写入、刷盘和读回一致，随后只删除本次创建的文件。
  第二次读取使用 Windows `FILE_FLAG_NO_BUFFERING` 绕过文件缓存，逐字节校验通过。
- 首次写入并刷盘约 8.15 秒；第二次无缓存读取约 1.89 秒。只是该卡与本次负载下的测量，
  不把 Windows 缓存读回耗时当作设备吞吐率。
- VIA.json SHA-256 与原定义一致。并发播放期间观察到音频 empty/drop/restart 均为 0，
  Wi-Fi 关联与 DHCP 成功；不据此宣称任意负载都无音频间断。
- 所有例程的 release 构建使用 `cargo build --release --locked --bins --features slint-ui`。
  卡热插拔、不同卡型、跨系统兼容及长时间大文件复制仍需实机验证。

## 官方参考

- [Espressif P4 SDMMC Host](https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/sdmmc_host.html)：控制器、槽位与 DMA 约束。
- [Espressif Hosted 与 SD 卡共存示例](https://components.espressif.com/components/espressif/esp_hosted/versions/2.10.0/examples/host_sdcard_with_hosted?language=en)：共享主机的双槽用途；本项目驱动实现为 Rust。
- [USB-IF Bulk-Only Transport 1.0](https://www.usb.org/sites/default/files/usbmassbulk_10.pdf)：多 LUN、CBW/CSW 和错误恢复。
