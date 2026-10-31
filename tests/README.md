# 测试入口

这些是独立主机测试包，不是固件烧录程序。根目录 Cargo 配置默认交叉编译，因此必须指定主机 target。

```sh
HOST="$(rustc -vV | sed -n 's/^host: //p')"
for suite in desktop keyboard hosted usb_audio; do
  cargo test --locked --manifest-path "tests/$suite/Cargo.toml" --target "$HOST"
done
cargo test --locked --manifest-path demos/rust-ui/Cargo.toml --target "$HOST"
```

- `desktop/`：桌面协议与 Hosted 网络。
- `keyboard/`：键盘映射与密码键盘。
- `hosted/`：Hosted 帧与协议。
- `usb_audio/`：USB 音频协议；`capture.py` / `playback.py` 需要音频设备。
- `keyboard/via_probe.py`：VIA 实机辅助探测，不能替代主机单元测试。
- `../demos/rust-ui/tests/`：输入状态机与局部/双缓冲绘制一致性。

根 package 的 `verify_*` 是板上固件，在 `src/bin/verify/`；编译不等于已执行硬件验证。不要在不了解接线时运行烧录或实机脚本。
