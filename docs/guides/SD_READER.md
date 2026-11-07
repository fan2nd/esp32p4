# microSD USB 读卡器

`desktop_pocket_ui`、`desktop_tiny_ui` 与 `desktop` 共用复合 USB 实现：LUN 0 保留 1 MiB VIA
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
USB 使用 HS，bulk 包 512 字节；SD 每批最多 16 扇区（8 KiB），每批完成释放
共享传输引擎，避免长时间独占 Wi-Fi 总线。缓冲位于主核任务内部 SRAM，仍无延迟写回。

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

## 双槽是否冲突（2026-11-02）

ESP32-P4 是一个 SDMMC Host 提供两个槽位，并非两个完全独立的传输控制器。
本项目 slot 0 接 microSD，slot 1 接 C5；可以共存，但共享传输引擎，事务需要排队。
本地 `3rd/esp-hal/src/sdmmc/mod.rs` 使用 `ENGINE` 异步互斥锁，取得锁后应用
对应槽位的时钟与总线宽度。因此正常调用现有驱动不会因为同时请求而直接覆盖彼此寄存器。

大量 SD 卡读写仍可能增加 Wi-Fi/BLE 传输延迟，慢卡写入或长事务尤其需要关注。
这是共享资源竞争，不等于引脚冲突，也不保证两个槽位各自同时达到满速。
把两个调用任务分到两个 CPU 核也不会增加 SDMMC 传输引擎数量。
显示 DMA 与 SDMMC 是不同外设；它们仍可能在内存带宽、缓存维护或系统调度上互相影响，
不能只凭卡顿就认定是双槽冲突。本次只核对官方文档和现有实现，没有新增并发压力测试。

## 升级前速度限制（2026-11-07，a60b45a）

用户在 `a60b45a` 实机验证后确认功能正常，但反馈读卡器较慢。本轮未进行吞吐测量，以下为代码确认的限制：

- `src/bin/apps/desktop_runtime.rs` 设置 `force_full_speed = true`，使用 HS 控制器但将整个复合设备限制为 12 Mbit/s，原始位速率换算只有 1.5 MB/s，实际文件吞吐还要扣除协议和其他接口开销。此配置用于保留已验证的全速 UAC1 音频行为。
- `src/driver/usb_msc/mod.rs` 的 bulk 端点为 64 字节；每次只读取一个 512 字节扇区，再逐包等待 USB 发送。写入同样先收齐单扇区再等待 SD 写完，没有多扇区批处理或 SD/USB 双缓冲流水线。
- SD 卡当前运行在 20 MHz，且与 C5 共用 SDMMC 传输引擎。无线事务可能增加等待，但尚未测量其实际影响比例。

优先在全速模式中优化多扇区与有界缓冲，避免长时间占用共享 SDMMC 引擎；若需要明显超过全速上限，则必须迁移整个复合设备至高速模式，并一起适配和验证音频端点、反馈、MSC 包长及描述符。不能只取消 `force_full_speed` 就认定音频和读卡器都已适配高速。

## HS 与批量传输实测（2026-11-07）

- 复合桌面升级至 UAC2 + HS，DSTS.ENUMSPD 日志确认为 0（实际 HS）。
  保留 VID/PID、接口编号、两个 LUN；MSC bulk 包长 512 字节。
- 仅改 HS、仍逐扇区时，1 MiB 文件写入 0.050 MiB/s、无缓存读取 0.136 MiB/s，
  校验一致。可见接口速率不是唯一瓶颈，不能把 HS 标称速率当作文件吞吐。
- 改为 SD 多块命令，每批最多 16 扇区后：1 MiB 写入 1.488 MiB/s、读取 3.759 MiB/s；
  8 MiB 写入 1.219 MiB/s、读取 2.612 MiB/s，两次均逐字节校验一致。
- Windows 使用独立随机命名临时文件，NO_BUFFERING 读写，写入另设 WRITE_THROUGH
  并计入 FlushFileBuffers 耗时；只删除本次创建的文件，没有格式化或改动原有文件。
- 测量期间持续播放音频，日志 empty/drop/restart 均为 0，Wi-Fi 自动关联、DHCP 和天气刷新正常。
  此结果仅代表本卡和本轮负载；未验证长期复制、卡热插拔或 macOS。
- HS 模式需高速主机连接，当前未提供全速回退描述符。音频协议和参考见 `USB_AUDIO.md`。
- 最终全部 15 个 bin 的 release 构建（`slint-ui,pocket-ui`）通过；
  `cargo clippy --release --locked --lib --features pocket-ui` 通过，保留 38 项警告，非无警告检查。

## 后续优化顺序（2026-11-07）

以下为代码检查后的建议，尚未实现或测量收益：

1. 先记录每批 SD 读写耗时与 USB 收发等待，区分卡端、USB 和调度开销。
2. 使用两块内部 SRAM 缓冲，将下一批 SD 读取与上一批 USB 发送重叠；写入时将下一批
   USB 接收与上一批 SD 写入重叠。保持顺序、错误取消和 BOT residue 正确，写完成后才回复成功。
   两块 8 KiB 缓冲合计 16 KiB，比当前增加 8 KiB；需检查内部 SRAM 余量。
3. 对比 8/16/32 KiB 批量，在吞吐和 Wi-Fi/BLE 等待延迟之间选择，不能无界独占共享控制器。
4. SD 侧从 20 MHz 尝试协商 40 MHz 高速模式；先核对卡能力及 CMD6 切换，不能只提高时钟。
   官方支持 40 MHz SD High Speed，但本板走线、卡和当前 Rust 驱动仍需实测，吞吐不保证翻倍。
5. 更深层为 USB bulk 多包排队/控制器 DMA：当前每次仅设置 pktcnt=1，CPU 搬运 FIFO，
   且写 FIFO 时进入临界区。此项需修改 USB 驱动并回归音频、HID、MSC reset 和缓存一致性，
   优先级在上述流水线之后。不能简单去掉临界区或 yield 来追求吞吐。

## 官方参考

- [Espressif P4 SDMMC Host](https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/peripherals/sdmmc_host.html)：控制器、槽位与 DMA 约束。
- [Espressif Hosted 与 SD 卡共存示例](https://components.espressif.com/components/espressif/esp_hosted/versions/2.10.0/examples/host_sdcard_with_hosted?language=en)：共享主机的双槽用途；本项目驱动实现为 Rust。
- [USB-IF Bulk-Only Transport 1.0](https://www.usb.org/sites/default/files/usbmassbulk_10.pdf)：多 LUN、CBW/CSW 和错误恢复。
