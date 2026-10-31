# 仓库结构与迁移

本次整理只调整目录、Cargo 入口注册和文档导航。保留所有应用、演示、验证程序，
不更改公共模块 API，不重新拆分大量 crate，也不修改第三方驱动实现。

## 固件入口

根目录仍是一个固件 package。`Cargo.toml` 关闭自动发现并显式列出 14 个 bin，
名称与 feature 要求保持不变，既有 `cargo ... --bin <name>` 命令仍然适用。
不指定 `--bin` 时，`cargo run` 现在明确选择基础 `main`。

- `src/bin/apps/`：`main`、`desktop`、`keyboard`、`usb_audio`。
- `src/bin/demos/`：`rust_ui_demo`、`slint_demo`、`display_core1`、`wifi_ble`、`pet`。
- `src/bin/verify/`：`verify_display`、`verify_touch`、`verify_audio`、`verify_usb`、`verify_hosted`。

共享板级配置、驱动、异步任务及库模块仍在原位置。固件原先的源文件路径
`src/bin/<name>.rs` 改成上述分类路径；外部 IDE 配置若硬编码了源文件路径需同步。

## 文档与资源

- 原 `doc/` 的操作指南放到 `docs/guides/`。
- 引脚表与初始化参考放到 `docs/hardware/`。
- 设计、历史排障与验证分析放到 `docs/notes/`。
- 原始日志放到 `docs/logs/`，内容保持不变。
- 原 `官方资料/` 全部放到 `docs/reference/`，PDF、位图和摘录内容保持不变。
- 原 `partitions-keyboard.csv` 放到 `config/partitions-keyboard.csv`，分区内容保持不变。

根 README 和 [文档索引](README.md) 提供新路径。仓库外已收藏的旧 GitHub 文件链接
不会自动跳转，可通过这些索引查找，或查看迁移前提交。

## 保留的边界

- `assets/` 与 `ui/` 仍供原有固件使用，未删除字体、图像或授权文件。
- `vendor/` 包含实际依赖的本地补丁，不是可以随意清理的下载缓存。
- `demos/rust-ui/` 保持独立包，可单独做主机测试；其主机程序是预览，不是 P4 固件。
- `tests/` 是主机测试与实机辅助脚本，`src/bin/verify/` 是板上验证固件。
- `target/` 是忽略的本地构建产物，没有加入版本控制。

## 软件验证（2026-10-31）

验证环境为 Linux x86_64、Rust 1.99.1。

- `cargo build --release --locked --bins`：12 个非 Slint 固件完整编译和链接通过。
- `cargo build --release --locked --bins --features slint-ui -j 1`：全部 14 个入口完整编译和链接通过。
  初次并行 Slint LTO 构建被环境 SIGKILL；串行重试通过，没有据此修改功能或编译优化配置。
- `cargo check --release --locked --bins --all-features -j 1`：全部 feature 启用的固件编译检查通过。
- 主机测试：desktop 2、keyboard 7、hosted 4、usb_audio 2、Rust UI 9，共 24 项通过。
- Rust UI 主机包 fmt、严格 Clippy、HTTP 输入/帧 smoke 通过。
- Cargo metadata 确认 14 个 bin 名称及原 feature 门控未变。
- 仓库内非 vendor Markdown 相对链接检查通过；官方资料、原始日志和分区表与迁移前逐字节一致。
- 既有 Rust 模块未做全库格式化，未声称全库 lint 或真机测试通过。

目录整理不构成新的真机验证，显示颜色、触摸、音频、无线及跨核共存仍以各指南的原有证据为准。
