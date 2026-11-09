# PPA 支持核对

2026-11-10 核对官方 esp-hal main，提交 `e045405e6f036a7d8b2ef22bc877457dc3e6eba2`。

官方 [PR #6476](https://github.com/esp-rs/esp-hal/pull/6476) 于 2026-11-05 合并，新增的是 ESP32-S31 的阻塞式 SRM（缩放、旋转、镜像及颜色转换），不是 ESP32-P4 驱动。

该提交的 [P4 芯片配置](https://github.com/esp-rs/esp-hal/blob/e045405e6f036a7d8b2ef22bc877457dc3e6eba2/esp-metadata/devices/esp32p4/soc.toml) 中，`device.pixel_accelerator.support_status` 仍为 `not_supported`；S31 为 `partial`。`ppa` 模块受 `pixel_accelerator_driver_supported` 条件控制。

工程目前采用本地修补的 esp-hal 1.2.2。直接升级 main 不会为 P4 提供可用 PPA API。此次仅核对兼容性，没有更改 HAL、烧录 PPA 固件或宣称获得性能收益。

P4 硬件有 PPA，参考 [官方 P4 PPA 文档](https://docs.espressif.com/projects/esp-idf/en/latest/esp32p4/api-reference/peripherals/ppa.html)。若继续实验，需要先做纯 Rust 的 P4 寄存器、时钟及 DMA2D 适配，再用独立例程验证 RGB565 拷贝、2 倍缩放、PSRAM 缓存一致性和 DSI 并发；不能直接移用 S31 的 HP_SYS 寄存器路径。

PPA 可以减少 CPU 像素处理工作，但仍读写内存，不能据此保证减少 PSRAM 总线争用。现有双缓冲及缓存互斥修复需要保留。按当前内存约束，不恢复整幅逻辑画面缓存，也不分配大型内部 SRAM 中间缓冲。
