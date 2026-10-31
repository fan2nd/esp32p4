# 独立 crate 集中目录

根目录保留当前 P4 固件 package；其余独立 crate 都放在这里。`3rd` 是统一的存放位置，
不表示每个 crate 都来自第三方。

## 自有组件

- [tiny-ui](tiny-ui/README.md)：无 alloc、no_std 的通用组件框架；只依赖 embedded-graphics。

Rust UI 演示与 Slint 演示一样位于根 package：`src/bin/demos/rust_ui_demo.rs`
直接依赖 tiny-ui，页面模块为 `src/bin/demos/rust_ui_demo/page.rs`，不另建演示 crate。

完整桌面入口为 `src/bin/apps/desktop_tiny_ui.rs`；页面和硬件适配也在主 package，
同样直接使用 tiny-ui，详见 [组件桌面](../docs/guides/DESKTOP_TINY_UI.md)。

## 有本地补丁的第三方驱动

- [esp-hal 1.2.2](esp-hal/LOCAL_PATCHES.md)：保留 P4 缓存维护串行化、DMA 对齐/同步等补丁。
- [embassy-usb-synopsys-otg 0.4.0](embassy-usb-synopsys-otg/PATCHES.md)：保留 USB FIFO、传输收尾与全速模式补丁。

根 `Cargo.toml` 通过 `[patch.crates-io]` 引用它们，不是可直接删除的下载缓存。
框架 tiny-ui 不依赖这些驱动；复用 UI 时不需要拷贝它们。

## 实机辅助工具

项目自有主机测试包已移除。VIA 与 USB 音频辅助脚本位于 `tools/`，
见 [工具说明](../tools/README.md)。`3rd/` 只保留以上三个实际使用的依赖。
