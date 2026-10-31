# SDIO 与 MIPI 共存故障：缓存维护缺少跨核互斥

日期：2026-10-31。硬件：ESP32-P4-NANO-WIFI6-DB，P4 v3.1，C5 Hosted 3.0.10。

## 设计问题

副核 MIPI VDMA 中断与主核 SDIO 都会维护 DMA 描述符和数据缓存。
共享缓存控制器的 ROM 操作必须跨核互斥；CPU 设置 OWNER 并不意味着 DMA
已看见新值，还需要正确缓存同步。描述符不可用与总线致命错误是不同状态。
本补丁只覆盖缓存操作互斥，不据此保证所有复合负载问题均已解决。

## 官方资料

1. [ESP32-P4 TRM](https://documentation.espressif.com/esp32-p4_technical_reference_manual_en.pdf)，
   下载版本 Pre-release v0.7，3701 页。第 57 章 SDHOST：
   - 第 3359 页，表 57.8-1：DES0 bit31 OWNER，DMA 完成后清零。
   - 第 3363 页，57.9.5：DMA 取到 OWNER=0 描述符时挂起并发出 DU。
   - 第 3389 页，寄存器 57.29：IDSTS.DU bit4、FBE bit2、AIS bit9。
   本地原件：`docs/reference/esp32-p4_technical_reference_manual_en.pdf`。
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

## 补丁与验证边界

`3rd/esp-hal/src/soc/esp32p4/mod.rs` 的 `CACHE_OP_LOCK` 完整保护数据写回、
数据失效和指令缓存失效的 ROM 调用，使用 esp-sync RawMutex 处理跨核互斥与本核中断。
不降低 SDIO/MIPI 时钟，也不停用外设。

历史组合验证支持保留该补丁，但不代替当前依赖版本的实机回归。
主机包池测试只验证缓冲独占与归还，不能证明硬件缓存互斥。
USB/音频复合负载、缓存持锁时长对实时音频的影响及长期稳定性仍需单独验证。
