# UI demo：用独立组件复刻 Slint 展示页

此 crate 是**应用示例**，可复用框架在 [`../tiny-ui`](../tiny-ui/README.md)。
这里拥有页面布局、组件 ID、Volume/Mode/Selected 状态和 Reset 行为，而不实现组件内部的输入/绘制逻辑。
页面以 [`ui/demo.slint`](../../ui/demo.slint) 为参考：360×640、音量滑条、模式下拉、可滑动点选列表、复位按钮。

## 运行

从本目录执行：

```sh
HOST="$(rustc -vV | sed -n 's/^host: //p')"
cargo run --locked --target "$HOST" --features host --bin demo
```

打开 http://127.0.0.1:7878 ，Ctrl-C 结束。只监听本机 loopback，为单用户开发预览，不用于生产网络服务。
多个标签页共享同一 UI 状态。只有 `host` 程序依赖 `std`/`Vec`，Rust 完成所有绘图和状态更新，
浏览器 HTML/JS 仅转发输入和显示 RGBA 像素，不需要 SDL/C/C++ 图形库。
根 Cargo 配置默认目标是 P4，所以主机命令必须明确传 `--target "$HOST"`。

- 拖动滑条：0–100，初始 35；移出后继续捕获并夹紧。
- 模式下拉：四种模式，选择关闭；外部点击只关闭，不穿透其他控件。
- 列表：30 行，拖动滚动，点选更新标签；滑动不会触发选择。
- 复位：音量回 35，列表取消选中，保留模式和滚动位置，与 Slint 示例一致。
- Tab / Shift-Tab：焦点切换；方向键调节/选择，Enter / 空格激活，Escape 关闭/取消。
- 页面/画布失焦发送 Cancel；单指捕获，不把第二根手指加入当前手势。

生成实际软件渲染截图（PPM）：

```sh
cargo run --locked --target "$HOST" --features host --bin demo -- --snapshot demo.ppm
```

## 在固件中使用

`Ui` 组合框架组件；`handle(Event)` 更新本示例业务状态，返回 `Action` 供应用记录或执行外部效果。
示例不实际改变音频音量。`state()` 和 `scroll_offset()` 可观察状态。
`render` / `dirty` / `render_full` / `render_with_extra_damage` 委托独立框架，遵守同样的脏区和 A/B 历史规则。

P4 适配在根 crate 的 `src/bin/demos/rust_ui_demo.rs`，显示、触摸、DMA 不属于框架。
360×640 逻辑 RGB565 完整帧需 460800 字节；框架本身不分配该缓冲，不要将它放到小任务栈。

## 测试

```sh
HOST="$(rustc -vV | sed -n 's/^host: //p')"
cargo fmt --check
cargo test --locked --target "$HOST" --all-features
cargo clippy --locked --target "$HOST" --all-targets --all-features -- -D warnings
cargo check --locked --lib --no-default-features --target riscv32imafc-unknown-none-elf
cargo build --locked --target "$HOST" --features host
python3 scripts/smoke_host.py "target/$HOST/debug/demo"
```

测试覆盖初始状态、拖动/取消、按钮激活、下拉捕获、滚动与点选仲裁、复位策略、
键盘焦点、局部与全量像素一致性、A/B 历史修复、绘制错误和无效脏区。
浏览器 HTTP `/state` 是仅供本地主机测试的状态观察端点。

边界：ASCII 位图字体、无动画/惯性/自动布局，组件是固定容量 Scene 内的实例。
与 Slint 的字体/皮肤不是逐像素等价。未实测真实触摸/显示；未做真实浏览器自动化。
浏览器适配仍是简单串行 HTTP 事件队列，高频 Move 合并/限流不是本版功能。
