# USB 音频偶发 DMA 重启排查（2026-10-31）

## 问题范围

音频 DMA 的缓存一致性与应用供数延迟是两个独立问题。内部 SRAM 同样需要
缓存同步，不能仅以缓冲未放在 PSRAM 为由排除 DMA 一致性问题。

## 官方参考与推断

- [Espressif P4 Memory Synchronization](https://docs.espressif.com/projects/esp-idf/en/latest/esp32p4/api-reference/system/mm_sync.html)：
  内部内存和外部 PSRAM 都可能经 CPU 缓存访问；DMA 绕过缓存，旧缓存写回
  会覆盖 DMA 更新。缓存同步必须考虑整个缓存行，而不仅是逻辑对象。
- [Espressif I2S 驱动](https://github.com/espressif/esp-idf/blob/master/components/esp_driver_i2s/i2s_common.c)：
  I2S 的 DMA 缓冲按约束对齐，并在 CPU/DMA 交接处同步缓存；链表通过指针
  连接。该源码仅用于参考，本项目不引入 C 实现。
- [esp-hal DMA 实现](https://github.com/esp-rs/esp-hal/blob/main/esp-hal/src/dma/buffers/mod.rs)：
  本地 1.2.0 的 streaming buffer 会 invalidate 描述符数组，修改已消费描述符
  及旧尾节点，再 writeback。DMA 此时仍会更新其他描述符。

本地 P4 `DmaDescriptor` 原为三个 32 位字、8 字节对齐，结构大小 16 字节，
一条内部 SRAM L1 DCache 的 64 字节缓存行容纳四个描述符。CPU 回收一个
描述符时把该缓存行标脏，随后 DMA 更新邻接描述符，CPU 再写回整行就可能
覆盖邻接描述符的新 owner/状态。这是根据源码与官方缓存模型得到的根因假设，
不能仅凭原理分析声称硬件问题已经解决。

## 单变量修改

仅在 P4 将 `DmaDescriptor` 的对齐和大小设为 64 字节，隔离每个描述符。
三个硬件字段偏移仍为 0、4、8，链表通过 next 指针寻址，不要求节点紧密排列。
增加编译期断言验证这些布局约束。其他芯片保持原有布局；不增大音频数据缓冲，
不暂停显示，也不通过关闭无线来掩盖问题。

TX/RX 各约 32 个描述符，内部 SRAM 描述符开销合计增加约 3 KiB。
该布局经软件编译检查，历史有限硬件验证支持保留补丁，但不构成长期稳定性保证。

## 应用配置

- 桌面 USB PCM 目标水位为 128 采样（8 ms），队列容量为 256；DMA 数据环为 4096 字节。
- 绘制采用 32 行 SRAM 条带与脏行区间；每个物理帧保存自身 UI 状态，避免换帧恢复旧内容。
- 对齐 32 位 store 写入 PSRAM，保留帧边界等待；局部绘制不等于全屏 60 fps。

## 限制

偶发播放停顿与补零尚未解释，缺少主机侧时间戳证据确认来源。
本次没有为该停顿加入试验性修复，也不声称长期零断音。
依赖升级后的声学连续性、触摸响应与复合负载需要重新做板上验证。
