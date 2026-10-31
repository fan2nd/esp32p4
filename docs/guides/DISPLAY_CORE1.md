# 第二核独立刷屏 demo

增加 Wi-Fi/BLE 的可选对照模式见 [共存验证](../notes/COEXIST_VERIFY.md)，默认不启用。

入口：`src/bin/demos/display_core1.rs`。只运行屏幕与双核心跳，不启动 Wi-Fi、BLE、
USB、音频、触摸或 Embassy executor。

主核先初始化共享 PSRAM 映射，再启动副核，将双缓冲显存的独占引用交给副核。
副核创建 I2C1、初始化面板与 MIPI/VDMA、绘图、提交显存并等待 VSYNC；显示
中断也在副核安装。主核不读写显存。
CPU 启动 guard 在主核循环内持续存活，副核栈为 32 KiB。

画面为深色背景、白色边框、顶部红绿蓝色块，以及往返移动的黄色方块。
每次重绘完整后缓冲，使用内部 RAM 的 u32 行缓冲写入 PSRAM；颜色编码沿用
已验证的 BGR565，DSI 输出 RGB888。两帧占用 3,686,400 字节。
提交后等待三次 VSYNC（可能已挂起的一次及两次新边界），再复用旧前缓冲。

面板时序沿用默认值：720×1280，像素时钟请求 58 MHz、实际约 60 MHz，
HFP52，VFP24，约 56 Hz 扫描；两条 DSI lane，700 Mbit/s/lane。
面板扫描频率不等于软件画面更新率。移动位置按时间计算，速度约 200 像素/秒。

## 构建与烧录

以下 `COMx` 须替换为实际板卡串口。

```powershell
cargo build --release --bin display_core1
espflash flash --port COMx --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/display_core1
```

## 验证结论与限制

独立副核显示与移动方块已通过实机视觉验证，无条纹或闪烁。
该结果不代表与无线、USB、音频集成后的稳定性已验证。
