# PocketJS + MicroTS 复合桌面

## 入口与实现范围

新增 `desktop_pocket_ui`，使用 `pocket-ui` feature。保留 `desktop_tiny_ui` 和 Slint `desktop`。
三个入口复用 `src/bin/apps/desktop/runtime.rs`：USB 键盘/VIA、喇叭、麦克风、触控板、
双 LUN MSC、Wi-Fi/BLE、网页图片、天气、KV 和手动校时的服务不另写一套。

本版是共用页面模型的 PocketJS 渲染适配，不是把全部业务重写成 TypeScript：
- `tiny_desktop_page.rs` 继续维护页面状态、坐标、触摸捕获、滑动列表、密码键盘及业务动作。
- `assets/ui/pocket/Desktop.vue` 和 `Desktop.d.ts` 是 MicroTS 视图与类型契约；
  官方编译器产生 `gen/*.rs` 和 `styles.bin`，通过 PocketJS retained core 和软件渲染器绘制按钮、卡片、列表边框、滚动指示条和滑条。
- 时钟 Inter 数字、状态图标、SSID/CJK、天气、键名和动态主页图片仍共用已有覆盖层，保持资源和显示效果。
- `src/pocket_desktop.rs` 将页面控件状态传入生成视图。触摸仍由原有交互状态机处理，
  不同时运行第二个事件系统；MicroTS 在本版负责 AOT 视图，不运行 JS 引擎。

固件全部是 Rust，不链接 QuickJS、ESP-IDF 或 C/C++。Node/Bun/Python 只用于电脑端资源生成。
官方 MicroTS 说明：https://pocketjs.dev/docs/microts/
源码来源及固定 revision 见 `3rd/pocketjs/UPSTREAM.md`。

## 内存和刷新

Core 1 使用原有 RGB565 双缓冲和约 30.85 Hz 时序。4 MiB UI 堆放在两个扫描缓冲与
`home_image::ARENA` 之后，检查 PSRAM 容量，不覆盖图片上传区。
PocketJS 先将脏矩形栅格化到 360×640 RGB565 工作缓冲，再按原有对齐 32 位写入路径
扩展到 720×1280。两个交替扫描缓冲的历史损伤仍由现有代码合并。
时钟/图片/触摸没有变化时不会持续重绘。没有声称刷新性能与原版一致，需实机对比。

## 构建与生成

普通构建直接使用已提交的生成文件，无需 Node/Bun：

```powershell
cargo build --release --locked --bin desktop_pocket_ui --features pocket-ui
espflash -S flash --port COM6 --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/desktop_pocket_ui
```

修改 Vue 或类型声明之后：

```powershell
Push-Location assets/ui/pocket
npm ci --ignore-scripts
Pop-Location
./tools/build_pocket_ui.ps1
```

编译器锁定 `@pocketjs/framework@0.16.0`，Bun 锁定 1.4.4。
脚本支持 `-Bun` 指定可执行文件；Windows 默认能找到 npm 可选包中的 Bun 二进制，
不依赖 postinstall。不要手改 `gen`。

控件字体为 Inter 16 px、2 倍采样，字号对应 PocketJS font slot 2。来源、许可与现有
`assets/CLOCK-FONT-LICENSE.txt` 一致。重建命令：

```powershell
python tools/build_pocket_font.py path/to/Inter.ttf
```

需要 Pillow；字体源 SHA-256：`29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031`。

## 验证记录（2026-11-04）

- P4 release 构建和启用 `slint-ui,pocket-ui` 的所有例程构建通过。
- 用实际 Painter 和生成视图进行电脑端离线渲染，目视确认按钮字体、位置、滑条及边框滚动条。
- 未新增单元测试或本地测试脚本；离线预览程序只在临时目录运行。
- 921600 烧录成功；初版内联保存渲染器状态时启动停住，改为 Box 将长期状态放入 UI 堆后启动正常。
- 串口确认 DPI streaming、USB configured、SD ready、Wi-Fi 自动重连/DHCP 和天气刷新成功。
- VIA v13 的只读查询通过，网页 GET / 返回 200。
- 用户确认主页、SET 切页、Wi-Fi 列表滑动、音量滑条和键盘无缺字、错位或明显卡顿。
- 本轮未单独做音频播放/录音压力验证。
