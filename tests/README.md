# 测试入口

根固件以外的所有独立 crate 都位于 `3rd/`，本目录仅保留测试导航。主机测试不是固件烧录程序；根目录 Cargo 配置默认交叉编译，因此必须指定主机 target。以下命令从仓库根目录执行。

```sh
HOST="$(rustc -vV | sed -n 's/^host: //p')"
for suite in desktop-protocol-tests keyboard-host-tests hosted-v3-tests usb-audio-host-tests; do
  cargo test --locked --manifest-path "3rd/$suite/Cargo.toml" --target "$HOST"
done
cargo test --locked --manifest-path 3rd/tiny-ui/Cargo.toml --target "$HOST"
cargo test --locked --manifest-path 3rd/ui-demo/Cargo.toml --target "$HOST"
```

- [desktop-protocol-tests](../3rd/desktop-protocol-tests/)：桌面协议与 Hosted 网络。
- [keyboard-host-tests](../3rd/keyboard-host-tests/)：键盘映射与密码键盘。
- [hosted-v3-tests](../3rd/hosted-v3-tests/)：Hosted 帧与协议。
- [usb-audio-host-tests](../3rd/usb-audio-host-tests/)：USB 音频协议；`capture.py` / `playback.py` 需要音频设备。
- [via_probe.py](../3rd/keyboard-host-tests/via_probe.py)：VIA 实机辅助探测，不能替代主机单元测试。
- [tiny-ui](../3rd/tiny-ui/)：可复用无 alloc UI 框架。
- [ui-demo](../3rd/ui-demo/)：演示应用、主机预览及集成测试。

根 package 的 `verify_*` 是板上固件，在 `src/bin/verify/`；编译不等于已执行硬件验证。不要在不了解接线时运行烧录或实机脚本。
