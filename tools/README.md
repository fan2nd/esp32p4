# 开发工具

从仓库根目录运行，先阅读脚本的参数和输入要求。

## 资源生成

- `build_cjk_font.py`：生成 `assets/cjk16.bin`；字体授权见 `assets/CJK-FONT-LICENSE.txt`。
- `build_pet_bubble.py`：生成宠物气泡图像资源。

上述资源工具不属于固件构建的必要步骤。现有生成资源保留在 `assets/`，无需为了编译重新生成。

## 实机辅助

- `via_probe.py`：读取实机 VIA 接口，需要 `hidapi`。默认只读；`--roundtrip` 会临时修改 L3 最后一个键并恢复，不发送键盘输入。
- `usb_audio_playback.py`：Windows 定向播放低幅度 440 Hz 音频，使用 Python 标准库，不改变默认音频设备。
- `usb_audio_capture.py`：Windows 从 P4 麦克风读取短 PCM16 片段并报告电平，不保存录音；复用同目录播放工具的格式定义。

```sh
python tools/via_probe.py
python tools/usb_audio_playback.py --device "P4 Desktop"
python tools/usb_audio_capture.py --device "P4 Desktop"
```

这些工具需要真实硬件，不是单元测试或离线回归脚本。连接 USB 前先阅读
[供电安全说明](../docs/guides/USB_AUDIO.md)，VIA 用法见 [键盘指南](../docs/guides/KEYBOARD.md)。
项目自有测试与主机测试包已移除；验证采用构建、静态检查与手动硬件验收，见 [项目约束](../AGENTS.md)。
