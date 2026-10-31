# 无堆分配的 Rust 滑条与按钮 demo

此例程保留原有 `desktop` / Slint 程序，另提供 `rust_ui_demo` 固件入口。
UI 核心是 `demos/rust-ui` 内的 `tiny-ui-demo`：`#![no_std]`、无 `alloc`、无全局分配器，
只依赖 `embedded-graphics 0.8`。它不是完整 GUI 框架，也不引入 C/C++ UI 库。

## 固件

```sh
cargo build --release --bin rust_ui_demo
cargo run --release --bin rust_ui_demo
```

无需开启 `slint-ui`。沿用项目的 stable 工具链、`riscv32imafc-unknown-none-elf` 目标、
最小芯片版本 3.1、现有 Waveshare 720×1280 MIPI 面板初始化和 GT911 驱动。
构建和闪存前请阅读根目录说明，并确认板型与引脚适配；没有接入真实硬件的环境不能验证显示/触摸效果。

- 320×240 逻辑画布放大两倍，居中于物理 `(40,400)` 至 `(679,879)`，画布外为黑色。
- 拖动滑条设置 0–100；点击 RESET 按钮恢复 50。数值只是示例状态，不控制实际音量或背光。
- 核 1 独占 UI、触摸、显示和帧缓冲；核 0 每两秒打印心跳及更新次数。
- 固定 PSRAM 双缓冲共 `720 * 1280 * 2 * 2 = 3,686,400` 字节；核 1 栈为 32 KiB。
  无堆分配并不等于无需显存；这里使用静态大小的 PSRAM 映射切片。
- RGB565 逻辑颜色转换成现有面板验证过的 BGR565 小端字节。显示底层保持不变。
- 只在 UI 脏区出现时绘制/提交；修复当前脏区和前一次逻辑脏区，防止交替缓冲恢复旧画面。
  启动时两块缓冲都完整初始化。缓存维护由现有 `dpi.commit()` 处理，并保留三次 VSYNC 等待。
  局部绘制不意味着局部 DMA 扫描，也不意味着底层只写回脏区缓存。
- 手势捕获固定 GT911 contact ID，不把拖动转移给第二根手指。空报告产生 Up；无新报告不等于松开。
  超过 500 ms 无有效报告或原触点消失时发 Cancel，不合成点击，等全松开后重新接受按下。

## 桌面与测试

主机预览、键盘输入以及测试均隔离在独立包中，不拉入板级依赖：

```sh
cd demos/rust-ui
cargo test --target x86_64-unknown-linux-gnu
cargo run --target x86_64-unknown-linux-gnu --features host --bin demo
```

根目录 Cargo 配置默认交叉编译，所以运行主机命令时必须明确指定本机 target（上例为 Linux x86_64；
其他主机请以 `rustc -vV` 的 host 为准）。桌面程序具体操作和验证项见
[独立 demo 说明](../demos/rust-ui/README.md)。这些命令中的键盘输入是主机 demo 能力，固件没有添加 USB 键盘输入。

## 验证边界

主机测试检查滑条范围、按键焦点/激活、触摸取消和部分绘制与完整绘制的像素一致性，
包括交替双缓冲历史修复。交叉编译只验证目标代码能构建，不证明面板颜色、触摸定位、
缓存一致性、显示帧率或 USB 音频共存已在实机验证。此独立固件不启动音频/无线/USB 服务。

2026-10-31 软件验证（Rust 1.99.1，Linux x86_64）：

- `cargo check --release --bin rust_ui_demo`：通过。
- `cargo build --release --locked --bin rust_ui_demo`：通过，包含 P4 目标链接。
- 新固件文件 rustfmt、独立包 fmt：通过。
- 独立包 `cargo test --target x86_64-unknown-linux-gnu --locked --all-features`：9 项通过。
- 独立包同主机 target 的 Clippy（`--all-targets --all-features -- -D warnings`）：通过。
- 本地主机 HTTP 帧/事件、滑条、焦点、重置、箭头输入 smoke：通过。
- 实际面板/GT911、真实浏览器交互、时序/性能：未验证，不据软件测试推断实机成功。
