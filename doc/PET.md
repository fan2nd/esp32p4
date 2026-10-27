# ESP32-P4 鲸鱼娘桌宠

`pet` 是面向当前 720×1280 HX8394 双 lane DSI 屏幕的独立程序。角色素材基于用户提供的参考图生成，并参考[鲸鱼娘社区资产库](https://treapgogo.github.io/deepseek-whale-girl/index.html)；该站点是社区项目，并非 DeepSeek 官方。

## 运行

```powershell
cargo run --release --bin pet
```

命令使用工程 runner 烧录并打开串口。新板如果烧录后没有自动启动，需要按一下 RESET。原来的网络程序和屏幕测试仍可分别使用 `--bin main` 和 `--bin verify_display`。

## 静态画面

启动后显示大号抬手打招呼的静图，角色为 700×653 像素、与头顶的“你好，有大白饭吗”气泡一起在屏幕居中。气泡由 `tools/build_pet_bubble.py` 生成。`assets/whale-girl-wave.png` 是原始透明图，`assets/whale-girl-wave-large.rgb` 是与浅蓝背景合成的 RGB888 素材，直接嵌入固件；程序使用一块 PSRAM 帧缓冲，无需运行时解码。当前没有动画或触摸交互。



