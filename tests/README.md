# 测试入口

根固件以外的所有独立 crate 都位于 `3rd/`，本目录保留测试导航及组件展示和完整桌面的页面测试，不额外建立演示 crate。主机测试不是固件烧录程序；根目录 Cargo 配置默认交叉编译，因此必须指定主机 target。以下命令从仓库根目录执行。

```sh
HOST="$(rustc -vV | sed -n 's/^host: //p')"
for suite in desktop-protocol-tests keyboard-host-tests hosted-v3-tests usb-audio-host-tests; do
  cargo test --locked --manifest-path "3rd/$suite/Cargo.toml" --target "$HOST"
done
cargo test --locked --manifest-path 3rd/tiny-ui/Cargo.toml --target "$HOST"
python3 tools/test_rust_ui_demo.py
python3 tools/test_tiny_desktop.py
```

- [desktop-protocol-tests](../3rd/desktop-protocol-tests/)：桌面协议与 Hosted 网络。
- [keyboard-host-tests](../3rd/keyboard-host-tests/)：键盘映射、密码键盘与两个 UI 共用的宏控制器。
- [hosted-v3-tests](../3rd/hosted-v3-tests/)：Hosted 帧与协议。
- [usb-audio-host-tests](../3rd/usb-audio-host-tests/)：USB 音频协议；`capture.py` / `playback.py` 需要音频设备。
- [via_probe.py](../3rd/keyboard-host-tests/via_probe.py)：VIA 实机辅助探测，不能替代主机单元测试。
- [tiny-ui](../3rd/tiny-ui/)：可复用无 alloc UI 框架。
- [rust_ui_demo.rs](rust_ui_demo.rs)：直接测试根 bin 的 `src/bin/demos/rust_ui_demo/page.rs`，保留 11 项页面集成测试；运行 [test_rust_ui_demo.py](../tools/test_rust_ui_demo.py)，无需独立演示 crate。

`tiny-ui` 包含 17 项框架测试；页面测试脚本使用主机目标执行，不烧录固件，也不提供 HTTP/浏览器预览。

根 package 的 `verify_*` 是板上固件，在 `src/bin/verify/`；编译不等于已执行硬件验证。不要在不了解接线时运行烧录或实机脚本。

- [tiny_desktop.rs](tiny_desktop.rs)：完整桌面的四页导航、Wi-Fi 命令、密码输入、宏键布局和局部/双缓冲绘制；使用 [test_tiny_desktop.py](../tools/test_tiny_desktop.py) 直接测试主 crate 页面源码，不新增 crate。
