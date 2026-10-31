# 仓库结构与迁移

本次整理调整目录、Cargo 入口注册和文档导航，并将 UI 框架与演示应用分离。
保留所有应用、演示、验证程序。根固件以外的独立 crate 统一放在 `3rd/`。
本次另将 esp-hal 从 1.2.0 升级到 1.2.2，保留 P4 缓存/DMA 补丁；USB OTG 0.4.0 已是最新稳定发布，保留其全速补丁。

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
- 设计、排障方法与验证限制放到 `docs/notes/`。
- 当前树不收录原始运行日志；文档仅保留配置、分析及简洁验证结论。
- 原 `官方资料/` 全部放到 `docs/reference/`，PDF、位图和摘录内容保持不变。
- 原 `partitions-keyboard.csv` 放到 `config/partitions-keyboard.csv`，分区内容保持不变。

根 README 和 [文档索引](README.md) 提供新路径。仓库外已收藏的旧 GitHub 文件链接
不会自动跳转，可通过这些索引查找，或查看迁移前提交。

## 保留的边界

- `assets/` 与 `ui/` 仍供原有固件使用，未删除字体、图像或授权文件。
- `3rd/esp-hal/`、`3rd/embassy-usb-synopsys-otg/` 包含实际依赖的本地补丁，不是可以随意清理的下载缓存。
- `3rd/tiny-ui/` 提供独立 UI 框架；`3rd/ui-demo/` 是依赖该框架的演示包，其主机程序是预览，不是 P4 固件。
- `3rd/*-tests/` 是主机测试与实机辅助脚本，`tests/README.md` 保留测试导航；`src/bin/verify/` 是板上验证固件。
- `target/` 是忽略的本地构建产物，没有加入版本控制。

## 前次纯目录整理的软件验证（2026-10-31，25e9a3d）

验证环境为 Linux x86_64、Rust 1.99.1。

- `cargo build --release --locked --bins`：12 个非 Slint 固件完整编译和链接通过。
- `cargo build --release --locked --bins --features slint-ui -j 1`：全部 14 个入口完整编译和链接通过。
  初次并行 Slint LTO 构建被环境 SIGKILL；串行重试通过，没有据此修改功能或编译优化配置。
- `cargo check --release --locked --bins --all-features -j 1`：全部 feature 启用的固件编译检查通过。
- 主机测试：desktop 2、keyboard 7、hosted 4、usb_audio 2、Rust UI 9，共 24 项通过。
- Rust UI 主机包 fmt、严格 Clippy、HTTP 输入/帧 smoke 通过。
- Cargo metadata 确认 14 个 bin 名称及原 feature 门控未变。
- 仓库内非第三方依赖 Markdown 相对链接检查通过；官方资料和分区表与迁移前逐字节一致。
- 既有 Rust 模块未做全库格式化，未声称全库 lint 或真机测试通过。

目录整理不构成新的真机验证，显示颜色、触摸、音频、无线及跨核共存仍以各指南的原有证据为准。

## 独立框架、3rd 归档与依赖升级验证（2026-10-31）

本次状态区别于上面的前次纯目录整理：

- 根固件以外的 8 个独立 crate 均位于 `3rd/`；原有程序入口名称不变。
- `tiny-ui` 是可复用的 no_std / 无 alloc 组件框架；`ui-demo` 只负责应用组合。
- 原 `ui/demo.slint` 的 360×640 展示页对应为 Rust 组件页面；外观差异与未实现能力见 UI 指南。
- `esp-hal` 从 1.2.0 更新为官方 1.2.2，当前 crates.io 最新稳定发布；USB OTG 0.4.0 已是最新，无需升级。
  HAL 的三个本地补丁 Rust 文件在官方 1.2.0/1.2.2 中未发生变化，因此保留原补丁内容；
  其余上游文件更新至 1.2.2。依据与校验见 `3rd/esp-hal/LOCAL_PATCHES.md`。
- 最终代码冻结后 `cargo build --release --locked --bins --features slint-ui -j 1`：14 个固件完整编译链接通过。
- 最终代码 `cargo check --release --locked --bins --all-features -j 1`：通过。
- 主机测试：框架 17、页面集成 11、原四组协议测试 15，共 **43 项通过**。
- UI 两个 crate 的 fmt 和主机严格 Clippy 通过；框架严格 RISC-V no_std Clippy 通过。
- UI 核心及组合页 RISC-V / Cortex-M no_std 编译检查通过。
- HTTP 帧/状态往返、滑条、下拉捕获、拖动/点选、复位、键盘 smoke 通过；JS 语法检查通过。
- 非第三方依赖 Markdown 本地链接有效；官方资料、分区数据未改动。
- 独立代码复核并回归了极小滑条裁剪、程序切焦点取消捕获、列表越界滚动命中等问题，无未解决代码阻碍。

真实面板/触摸、实际浏览器操作、帧率/cache/DMA 时序、音频/无线共存均未在本次做新实机验证。
依赖升级只经软件验证，不继承旧版本硬件结果。此记录不代表仓库 CI 已执行或通过。
官方 HAL 1.2.2 的 `ld/esp32/linkall.x` 末尾原有空行按原字节保留；仅这一上游文件会产生 git diff 的空白提示。

## 发布文档范围

按用户要求，当前树移除原始运行日志文件与文档中的原始运行记录、设备标识及运行期地址。
保留开发板配置、用法、设计分析和简洁验证结论，包括尚未解决的音频停顿等限制。
运行历史、排查过程时间线与日志行为叙述也不纳入发布文档。
没有重写 Git 历史，也未改动固件的诊断日志代码。本次文档清理不构成新的硬件验证。
