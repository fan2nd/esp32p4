# 仓库结构与迁移

本次整理调整目录、Cargo 入口注册和文档导航，并将 UI 框架与演示应用分离。
保留所有应用、演示、硬件验证程序。`3rd/` 仅保留 tiny-ui 与两个实际使用的本地补丁依赖。
本次另将 esp-hal 从 1.2.0 升级到 1.2.2，保留 P4 缓存/DMA 补丁；USB OTG 0.4.0 已是最新稳定发布，保留其全速补丁。

## 固件入口

根目录仍是一个固件 package。`Cargo.toml` 关闭自动发现并显式列出 15 个 bin，
名称与 feature 要求保持不变，既有 `cargo ... --bin <name>` 命令仍然适用。
不指定 `--bin` 时，`cargo run` 现在明确选择基础 `main`。

- `src/bin/apps/`：`main`、`desktop`、`desktop_tiny_ui`、`keyboard`、`usb_audio`。
- `src/bin/demos/`：`rust_ui_demo`、`slint_demo`、`display_core1`、`wifi_ble`、`pet`。
- `src/bin/verify/`：`verify_display`、`verify_touch`、`verify_audio`、`verify_usb`、`verify_hosted`。

`desktop` 与 `desktop_tiny_ui` 共用 `src/bin/apps/desktop/runtime.rs` 的 Core 0 服务，
分别选择 Slint 与纯 Rust 组件 UI 后端，详见 [纯 Rust 完整桌面](guides/DESKTOP_TINY_UI.md)。

共享板级配置、驱动、异步任务及库模块仍在原位置。固件原先的源文件路径
`src/bin/<name>.rs` 改成上述分类路径；外部 IDE 配置若硬编码了源文件路径需同步。

## 文档与资源

- 原 `doc/` 的操作指南放到 `docs/guides/`。
- 引脚表与初始化参考放到 `docs/hardware/`。
- 设计、排障方法与验证限制放到 `docs/notes/`。
- 当前树不收录原始运行日志；文档仅保留配置、分析及简洁验证结论。
- 原 `官方资料/` 全部放到 `docs/reference/`，PDF、位图和摘录内容保持不变。
- 原 `partitions-keyboard.csv` 放到 `config/partitions-keyboard.csv`，分区内容保持不变。

根 README 和 [文档索引](README.md) 提供新路径。仓库外已收藏的旧 GitHub 文件链接
不会自动跳转，可通过这些索引查找，或查看迁移前提交。

## 保留的边界

- `assets/` 与 `ui/` 仍供原有固件使用，未删除字体、图像或授权文件。
- `3rd/esp-hal/`、`3rd/embassy-usb-synopsys-otg/` 包含实际依赖的本地补丁，不是可以随意清理的下载缓存。
- `3rd/tiny-ui/` 提供独立 UI 框架；根 bin `src/bin/demos/rust_ui_demo.rs` 直接依赖框架，页面模块在 `src/bin/demos/rust_ui_demo/page.rs`，不另设演示 crate。
- 项目自有测试、`tests/` 与独立主机测试包已移除；实机辅助脚本放到 `tools/`，`src/bin/verify/` 保留板上验证固件。
- `target/` 是忽略的本地构建产物，没有加入版本控制。

## 当前软件验证与运行方式

验证环境为 Linux x86_64、Rust 1.99.1。

- `cargo build --release --locked --bins --features slint-ui -j 1`：全部 15 个固件入口完整编译链接通过。
- `cargo check --release --locked --bins --all-features -j 1`：全特性编译检查通过。
- tiny-ui 格式检查、主机严格 Clippy 与 RISC-V no_std 编译检查通过。
- 仓库内非第三方依赖 Markdown 相对链接检查通过。

根 `rust_ui_demo` bin 与 `slint_demo` 一样直接组织演示，依赖 `3rd/tiny-ui`，
组合页面位于 `src/bin/demos/rust_ui_demo/page.rs`。没有独立 HTTP/浏览器预览包。
现有截图仅作渲染参考。

以上不代表仓库 CI、全库 lint 或真机验证通过。显示颜色、真实触摸、音频/无线共存、
帧率及 cache/DMA 时序仍需板上验证；依赖升级不继承旧版本硬件结论。

## 发布文档范围

按用户要求，当前树移除原始运行日志文件与文档中的原始运行记录、设备标识及运行期地址。
保留开发板配置、用法、设计分析和简洁验证结论，包括尚未解决的音频停顿等限制。
运行历史、排查过程时间线与日志行为叙述也不纳入发布文档。
没有重写 Git 历史，也未改动固件的诊断日志代码。本次文档清理不构成新的硬件验证。

## 项目约束与测试清理

[AGENTS.md](../AGENTS.md) 明确禁止添加单元测试。已有项目自有测试、测试专用包和脚本已移除；
第三方驱动的上游嵌入测试不做无关修改。后续采用构建、静态检查与手动硬件验证，
不把历史验证结果当作当前改动的实测结果。
VIA、USB 音频播放/采集辅助工具迁到 `tools/`，用法见 [工具说明](../tools/README.md)。
