# Slint 第二核控件演示

2026-10-31：独立演示已在 ESP32-P4-NANO-WIFI6-DB v3.1 烧录并验证。
用户测试滑条、下拉框和滚动列表后反馈“没问题”。本例未启用 USB 音频、键盘或无线，
独立演示的验证不代表复合负载表现；后续复合迁移记录见 [DESKTOP.md](DESKTOP.md)。

## 构建与烧录

```powershell
cargo build --release --bin slint_demo --features slint-ui
espflash flash --port COM6 --baud 921600 --chip esp32p4 --partition-table partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/slint_demo
```

`slint-ui` 为可选特性；`slint_demo` 和迁移后的 `desktop` 都需要它，其他独立例程不需要。已验证上述构建和不带特性的
`cargo build --release --bins` 均成功。首次构建需要下载并编译 UI 编译器及其字体、图片处理依赖。
目标与构建依赖检查未发现新增 cc/cmake 或 C/C++ 图形后端；使用 Rust MCU 软件渲染器。
Cargo.lock 包含其他可选平台的解析记录，不代表这些后端进入固件。

## 文件与职责

- `ui/demo.slint`：页面声明、滑条、模式选择、30 项列表和复位按钮。
- `src/bin/slint_demo.rs`：分配内存、启动第二核、GT911 事件和换帧。
- `src/slint_backend.rs`：与复合程序共用 Slint Platform、堆及 SRAM 行渲染实现。
- `build.rs`：仅启用特性时，将页面、字体等资源编译并嵌入 Rust 固件。

滑条只改变演示数值，不控制硬件音量；模式和列表项目也是本地演示数据。
复位按钮恢复滑条数值和列表选中状态，不重启芯片。

## 内存及双核约束

- Slint 1.18.1：`no_std`、`unsafe-single-threaded`、`libm`、`renderer-software`，禁用默认特性。
- 所有 Slint 调用和触摸处理只在第二核执行，符合其单线程配置约束。
- 主核运行日志队列消费者和统计任务；第二核日志通过既有队列提交，串口只由主核输出。
- PSRAM 开头为两帧 720×1280×2 字节显存，后接独立 4 MiB 堆；初始化在第二核启动前完成。
- 第二核栈 64 KiB；SRAM 行缓冲为 360 个 RGB565 像素，共 720 字节。
- 逻辑画布 360×640，每个像素放大到 2×2，输出 BGR565；按对齐 32 位 store 写 PSRAM。
- 面板沿用已验证的低刷新时序，VFP=1023，约 31 Hz。

堆预留量不等于实际占用。本次采样约 29–56 KiB，弹出控件等会临时增加占用；
这里只是观测值，不是全生命周期最大值，也不代表完整复合 UI 的内存需求。

## 按需重绘

`draw_if_needed` 控制重绘触发，`SwappedBuffers` 追踪当前及上一帧的变化区域。
`render_by_line` 只对这些区域回调，SRAM 行像素转换后写入后台帧。
不每轮清屏、不复制整帧、不无条件请求重绘。

`commit()` 沿用现有驱动的缓存写回；仍等待三个 VSYNC 通知后才复用旧前台缓冲，
以覆盖可能已挂起的第一次通知及两次新帧边界。显示等待仍会限制交互帧率，
绘制时间不能当作按下到显示完成的总延迟。持续 MIPI 扫描读取显存也不会停止。

列表明确启用 `mouse-drag-pan-enabled`，因为裸机 Pointer 事件用左键表示触点。
按下、移动、松开、离开事件均交给 Slint；丢失释放报告超过 500 ms 时强制释放，防止控件一直拖动。

## 实测日志

| 场景 | 绘制耗时 | 逻辑像素数量 |
| --- | --- | --- |
| 初始全屏 | 114,744 µs | 230,400 |
| 滑条变化采样 | 13,560 µs | 3,814 |
| 交互局部更新采样 | 16,947 µs | 11,064 |
| 列表区域更新采样 | 43,439 µs | 82,656 |

静止时帧计数保持 1；操作结束后又连续保持 133，说明空闲时没有持续重绘。
日志已收到滑条数值变化，以及滚动后第 16–19 项的选中事件。
固件 app 大小 939,328 字节。实测没有把绘制、缓存写回和 VSYNC 等待混算。

本轮未改变原复合程序的显示或音频实现。后续集成应单独验证持续音频播放下的滚动、弹出和切页。
Slint 授权与框架取舍见 [选型记录](UI_FRAMEWORK_OPTIONS.md)。
