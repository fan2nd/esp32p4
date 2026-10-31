# SDIO 与 MIPI 共存故障：缓存维护缺少跨核互斥

日期：2026-10-31。硬件：ESP32-P4-NANO-WIFI6-DB，P4 v3.1，C5 Hosted 3.0.10。

## 根因与证据

两个外设可以同时工作。本次故障来自当前 esp-hal P4 缓存维护入口没有统一
跨核互斥：副核 MIPI VDMA 中断维护显示描述符，主核 SDIO 维护收发缓冲和描述符，
两者会并发调用共享缓存控制器的 ROM 写回/失效操作。

实测 SDIO `IDSTS=0x2210`：DU 位 4 与 AIS 位 9 置位，FBE 位 2 未置位。
这不是“总线致命错误”。官方 TRM 说明 DU 在 DMA 看到 OWNER=0 时触发。
CPU 设置 OWNER 并不等于 DMA 已能看到该值，描述符必须正确完成缓存写回。

官方实现、源码差异及 A/B 结果共同定位到缓存维护的并发问题；没有直接记录
每个重叠 ROM 操作的寄存器访问，因此不声称已观察到具体哪个地址/长度寄存器
被覆盖。不能据此将所有复合固件问题都归结于该原因。

## 官方资料

1. [ESP32-P4 TRM](https://documentation.espressif.com/esp32-p4_technical_reference_manual_en.pdf)，
   下载版本 Pre-release v0.7，3701 页。第 57 章 SDHOST：
   - 第 3359 页，表 57.8-1：DES0 bit31 OWNER，DMA 完成后清零。
   - 第 3363 页，57.9.5：DMA 取到 OWNER=0 描述符时挂起并发出 DU。
   - 第 3389 页，寄存器 57.29：IDSTS.DU bit4、FBE bit2、AIS bit9。
   本地原件：`官方资料/esp32-p4_technical_reference_manual_en.pdf`。
2. [官方 Memory Synchronization 指南](https://docs.espressif.com/projects/esp-idf/en/latest/esp32p4/api-reference/system/mm_sync.html)：
   P4 的 CPU 缓存与 DMA 不自动保持一致，内部 RAM 和 PSRAM 都需要正确同步；
   CPU 更新数据后须在 DMA 读取前做 C2M；API 声明线程安全。
3. [ESP-IDF esp_cache_msync 实现](https://github.com/espressif/esp-idf/blob/master/components/esp_mm/esp_cache_msync.c)：
   写回与失效调用均受到同一 `s_spinlock` 临界区保护。只参考实现，不引入 C 代码。
4. [esp-hal 官方 P4 缓存实现](https://github.com/esp-rs/esp-hal/blob/main/esp-hal/src/soc/esp32p4/mod.rs)：
   查询时可见的上游实现直接调用 Cache_WriteBack_Addr / Cache_Invalidate_Addr。
   本项目原 1.2.0 registry 源码也没有统一锁。网页 main 会变化，复现应以本地
   vendored 版本和其 `.cargo_vcs_info.json` 为准。
5. [P4 v3.x 官方升级说明](https://documentation.espressif.com/esp32-p4-chip-revision-v3.x_user_guide_en.html)：
   v3.x 有内存架构和寄存器变化，不能直接套用旧版 P4 固件假设。

## 修复及对照

`vendor/esp-hal/src/soc/esp32p4/mod.rs` 增加共享 `CACHE_OP_LOCK`，完整保护
数据写回、数据失效和指令缓存失效的 ROM 调用，使用 esp-sync RawMutex 处理
跨核互斥与本核中断。未降低 SDIO 时钟、MIPI 时钟，也未停用任一外设。

- 修复前：MIPI 开启时反复 DU、Io、checksum/timeout；停止 CPU 重绘仍会失败。
- 加锁：动态画面累计超过 900 帧，Wi-Fi 多轮扫描成功；Windows Bleak 收到
  `44:BD:8D:D0:A5:3E / P4-WiFi-BLE` 实际广播，没有观察到 DU。
- 反向对照：仅去掉三个缓存操作的互斥，再次出现 `IDSTS=0x2210`、DmaError、
  后续连续 Timeout / HeartbeatStopped。
- 恢复互斥后再次正常；进一步去掉“先等显示两帧再启动 SDIO”的实验延时，
  两模块并行启动仍成功；最终版本连续四轮 Wi-Fi 扫描各返回 16 个 AP，
  刷新达到 997 帧，未观察到 DU/Io/checksum 错误，再次通过 Windows BLE 空口扫描。
  计数日志保存在 `doc/SDIO_MIPI_VERIFY.log`（保存到第三轮，退出监视时又收到第四轮）。
  用户随后确认修复版黄色方块正常移动、显示正常，无条纹或闪烁。

编译验证包括 display_core1 + verify-radio，以及 desktop、wifi_ble、usb_audio。
主机包池测试通过；它验证缓冲独占/归还，不用于证明硬件缓存互斥，后者由
上述实机正反对照验证。
当前板上验证的是动态显示 + 无线组合，不包含 USB/音频。完整 desktop 的红屏
是否也由该问题导致，需要单独验证；全帧 cache writeback 持锁时长对音频实时性
也不在本次结论范围内。
