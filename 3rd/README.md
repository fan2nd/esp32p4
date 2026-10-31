# 独立 crate 集中目录

根目录保留当前 P4 固件 package；其余独立 crate 都放在这里。`3rd` 是统一的存放位置，
不表示每个 crate 都来自第三方。

## 自有组件与示例

- [tiny-ui](tiny-ui/README.md)：无 alloc、no_std 的通用组件框架；只依赖 embedded-graphics。
- [ui-demo](ui-demo/README.md)：依赖 tiny-ui 的页面组合和主机预览；P4 入口在根 crate。

## 有本地补丁的第三方驱动

- [esp-hal 1.2.2](esp-hal/LOCAL_PATCHES.md)：保留 P4 缓存维护串行化、DMA 对齐/同步等补丁。
- [embassy-usb-synopsys-otg 0.4.0](embassy-usb-synopsys-otg/PATCHES.md)：保留 USB FIFO、传输收尾与全速模式补丁。

根 `Cargo.toml` 通过 `[patch.crates-io]` 引用它们，不是可直接删除的下载缓存。
框架 tiny-ui 不依赖这些驱动；复用 UI 时不需要拷贝它们。

## 主机测试包

- `desktop-protocol-tests`
- `hosted-v3-tests`
- `keyboard-host-tests`
- `usb-audio-host-tests`

命令和用途见 [测试导航](../tests/README.md)。不把 target 构建目录加入版本控制。
