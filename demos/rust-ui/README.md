# Tiny Rust UI：滑条与按钮

第一版不是 Slint 包装层，而是纯 Rust 控件/输入/绘制内核。复用 `embedded-graphics` 的 Rectangle、Circle、Text。核心 `#![no_std]`，不使用 `alloc`；默认 feature 为空。

## 运行桌面演示

在本目录执行（需要稳定版 Rust）。仓库根配置默认使用 P4 目标，因此主机命令显式指定 target；
以下以 Linux x86_64 为例，其他主机请替换为 `rustc -vV` 中的 host：

```sh
cargo run --target x86_64-unknown-linux-gnu --locked --features host --bin demo
```

浏览器打开 http://127.0.0.1:7878 ，Ctrl-C 停止服务。只有 `host` 演示使用 `std`/`Vec`。Rust 负责全部控件逻辑及像素绘制，HTML/JS 只负责输入转发和显示 RGBA 像素；无需 SDL、C/C++ 图形库或 Node。服务仅监听 loopback，是单用户调试工具，不用于生产网络服务。不要同时开多个演示标签页，它们共用一份 UI 状态。

- 鼠标/触摸：点击或拖动滑条，范围 0–100；按钮重置为 50。
- Tab / Shift-Tab：切换滑条与按钮焦点。
- 左右方向键：滑条每次增减 1，支持按住重复。
- Enter / 空格：按下按钮，松开时触发；失焦或取消时不触发。
- 滑条捕获拖动：移出范围继续追踪并夹紧到端点。
- 按钮：移出变为未按下，移回可恢复，外部释放或 Cancel 不触发。

导出无需浏览器的参考图片：

```sh
cargo run --target x86_64-unknown-linux-gnu --locked --features host --bin demo -- --snapshot demo.ppm
```

## 核心 API

- `Ui::new()`：固定两控件状态；首次 dirty 为全屏。
- `handle(Event)`：处理 `Down/Move/Up/Cancel`、`KeyDown/KeyUp`，返回 `ValueChanged(u8)` 或 `Clicked`。
- `set_value(u8)`：业务层更新滑条，自动夹紧并标记 dirty。
- `render(&mut DrawTarget<Color = Rgb565>)`：按脏矩形裁剪后，从背景重画场景，返回实际区域；成功后清空 dirty，错误时保留。
- `dirty()`：读取本次逻辑变化区域。
- `render_full()`：全量参考绘制或初始化新缓冲，不消费 dirty。
- `render_with_extra_damage(target, extra)`：额外修复旧 framebuffer 内容，返回实际合并区域。

UI 核心不处理 Wi-Fi、DMA 和等待。应用收到 `Clicked` 再执行业务动作；此演示执行 `set_value(50)`。实体按键驱动应先去抖，再映射为 `KeyDown/KeyUp`。触摸驱动只跟踪一个触点，并在失联、页面退出等情况下发送 `Cancel`。

## 内存与刷新

逻辑画布为 320×240，RGB565。核心没有 framebuffer，也不拥有堆；目标由调用者提供。完整 RGB565 framebuffer 为 153600 字节，双缓冲为 307200 字节，不能放到小容量任务栈上。

本版用一个合并后的脏矩形，适合两个控件。值变化只重画滑条，焦点切换合并旧/新控件区域。绘制会遍历固定场景，但裁剪限制实际像素写入；它不是成熟的布局/控件树系统。描边放在控件内，避免脏区外残影。

双整帧缓冲严格 A/B 交替时：

1. 在渲染前保存 `logical = ui.dirty()`。
2. `ui.render_with_extra_damage(back, previous_logical)`。
3. 成功提交后 `previous_logical = logical`，不要保存返回的合并区域，否则历史会持续扩大。
4. 等显示切换完成再写旧前缓冲；驱动负责 VSync、DMA 生命周期、cache clean。

第一次全屏 dirty 会在第二次渲染中修复另一块未初始化的缓冲。无变化时不必换帧；如果渲染错误、交换失败、丢弃已渲染帧或改变缓冲顺序，应恢复历史或全量初始化。核心不自动管理硬件提交。

## 验证

```sh
cargo fmt --check
cargo test --target x86_64-unknown-linux-gnu --locked --all-features
cargo clippy --target x86_64-unknown-linux-gnu --locked --all-targets --all-features -- -D warnings
rustup target add riscv32imafc-unknown-none-elf
cargo check --locked --lib --no-default-features --target riscv32imafc-unknown-none-elf
cargo build --target x86_64-unknown-linux-gnu --locked --features host
python3 scripts/smoke_host.py target/x86_64-unknown-linux-gnu/debug/demo
```

已执行：9 项测试通过，覆盖滑条夹紧/取消、按钮点击/移出/取消、键盘焦点与重复激活、无变化不重绘、绘制失败保留 dirty、所有 0–100 值和事件序列的局部/全量像素一致性、A/B 双缓冲历史修复。fmt、Clippy、RISC-V 与 Cortex-M no_std 编译检查通过。localhost HTTP 输入/帧往返测试通过，参考 PPM 已目视检查。

限制：无真实屏幕、触摸或实体按键实测；浏览器自动化启动被当前容器 socket 权限限制，因此未声称通过浏览器实交互验证。浏览器适配层是简单串行请求演示，尚未针对高频触摸做有限队列/Move 合并；核心为同步事件处理。字体只使用内置 ASCII，未实现中文、布局引擎、滚动手势仲裁、通用控件树、动画。
