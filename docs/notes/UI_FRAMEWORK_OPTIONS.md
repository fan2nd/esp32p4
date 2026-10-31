# UI 结构及控件框架选型

调研日期：2026-10-31。基于提交 29852c1，仅做结构分析和选型，没有更换固件或引入依赖。

## 当前结构

- `src/desktop.rs`：跨核状态 `View`、网络命令 `Command`、键盘报告队列及短临界区访问。
- `src/desktop_ui.rs`：第二核的页面状态、控件绘制、触摸命中判断、键盘交互和刷新循环。
  `Page` 分为 Home、Settings、Password、Keyboard；`Ui` 保存页码、密码、键盘层和按键状态。
- `embedded-graphics` 负责文字、矩形、圆角等绘图；当前 `button()` 只是绘制函数，没有统一控件对象、事件分发或布局系统。
- 逻辑画布 360×640，放大两倍输出到 720×1280。内部 SRAM 使用 32 行 RGB888 条带，转换为显示需要的 BGR565 后写 PSRAM 双缓冲。
- 局部刷新按页面和变化状态手动计算脏行，每个后台缓冲单独保存已绘状态，提交后等待帧边界。
- 主核负责 USB、音频、无线、网络和时间；第二核负责显示和触摸。两边通过状态快照、命令及报告队列协作。

目前 Wi-Fi 列表是每页六项的翻页列表，尚无拖动滚动。非键盘操作主要在首次按下时触发，
没有统一 Down/Move/Up/Cancel 事件、拖动捕获、焦点或弹出层体系。

## 能否继续增加控件

可以，但要同时增加交互状态和重绘机制，不能只增加绘制函数。

| 控件 | 必要能力 |
| --- | --- |
| 滑条 | 捕获触点，连续 Move 更新数值，范围限制，松开结束；只重绘轨道和滑块变化区域 |
| 下拉框 | 展开状态、弹出层优先命中、点击外部关闭；关闭后恢复被遮挡内容，正确更新两个缓冲 |
| 滚动列表 | 滚动偏移、视口裁剪、可见项绘制、拖动阈值区分点击和滑动；惯性滚动可后加 |

若继续自研，先统一输入事件和控件几何定义，避免绘图与命中坐标各写一份；
让控件产生失效区域，取代页面专用的固定脏行。当前主循环同时处理输入和等待显示，
长绘制期间不会继续采集触摸，需要调整调度。无需为了分层拆出许多薄封装或单文件目录。

## 候选方案

### 延续 embedded-graphics

保留现有显示链路、静态内存和简单依赖，适合只增加几个明确用途的控件。
代价是布局、滚动、弹出层、事件分发和失效区域都由项目维护。

### Slint：建议优先做独立验证

官方提供 Rust `no_std` MCU 接口及软件渲染器，需要全局堆分配器。
`.slint` 描述编译为 Rust；应禁用默认桌面特性，选择裸机软件渲染配置并检查最终目标依赖，保持项目禁止 C 的要求。
现成控件包括 Slider、ComboBox、ListView、ScrollView。ListView 仅实例化可见项，适合 Wi-Fi 列表。

需要自己接入 P4 的 Platform、计时、GT911 指针事件和显示提交；官方通用 MCU 支持不代表本板后端已经适配。
单线程配置要求所有 Slint 调用限定在第二核，不能从主核或中断调用；主核继续提供状态和处理业务命令。
软件渲染器提供 `SwappedBuffers`，可跟踪双缓冲需要的重绘区域，但缓存维护和安全换帧仍由显示后端负责。
需验证颜色顺序、字体、图片、内存峰值及刷新带宽；不能将运行时占用宣传值当作完整 720p 应用内存预算。

许可证有 GPLv3 和商业等选择，官方 Royalty-Free 桌面许可排除嵌入式系统。
项目分发或闭源商业化前应按官方条款选择许可，不能默认“Rust 开源库等于任意闭源免费使用”。

### bugadani/embedded-gui：备选参考

作者定位为实验性 `no_std`、`no_alloc` Rust GUI，有布局、基础控件和事件系统，支持 embedded-graphics 后端。
与当前技术栈接近，但不宜未经兼容性及功能验证就替换主界面。
这里特指 bugadani 的 GitHub 项目，不把同名 crates.io 包视作同一实现。

## 建议验证路径

保留现有复合程序，先做独立 Slint 例程：一个音量滑条、一个下拉框和一个可拖动的列表。
沿用第二核 UI、RGB565 双缓冲及现有缓存互斥修复，先验证画面和触摸，再加回主核持续 USB 音频。
评估按下到反馈延迟、滚动帧耗时、堆峰值、音频丢包/补零及 DMA 重启。
通过后再迁移主页和设置，避免一次更换 UI、音频和显示底层。

当前记录中，按键局部绘制约 41–44 ms，含提交等待约 77–86 ms，全页绘制仍约 190–220 ms。
换框架解决的是控件组织和交互能力，是否提速必须实测；PSRAM 带宽、缓存同步和帧等待不会因此消失。
相关记录见 `AUDIO_DMA_CACHE_FIX.md`。用户暂缓排查的 USB 收包停顿不属于本次选型范围。

## Slint 是否每帧全屏重绘

不需要。需要区分面板持续扫描、UI 是否发生变化，以及变化后重绘多大区域。

- `MinimalSoftwareWindow::draw_if_needed` 仅在窗口需要重绘时调用绘制回调；静态界面可以不重新绘制。
- 软件渲染器支持脏区域跟踪。按键高亮、时间文字变化只需重绘受影响区域，实际范围由布局、效果及缓冲策略决定。
- `NewBuffer` 每次绘制全窗口；`ReusedBuffer` 假定复用同一张完整帧；`SwappedBuffers` 适用于交替双缓冲，需要补绘最近两帧的变化区域。
- 例如先修改 A 按键，再修改 B 按键：第二次后台帧可能还没有 A 的新状态，所以需要补绘 A 和 B，而不是只画 B，也不必重绘整屏。
- 滑动列表时，列表视口内大部分像素都会变化，通常需要持续重绘视口；可见项实例化节省对象和布局成本，并不等于只绘制新露出的一行。动画同样会连续触发更新。
- MIPI/VDMA 仍按面板时序读取现有前台帧。停止 UI 重绘可以减少 CPU 绘制和 PSRAM 写入，但不会停止扫描造成的 PSRAM 读取。

移植时要使缓冲交替与 `SwappedBuffers` 假设一致，保留缓冲内容，不能每轮先清屏或无条件请求全屏重绘。
SRAM 条带只是临时工作区，不能当作保存完整上一帧的 `ReusedBuffer`；最终像素保存在 PSRAM 帧缓冲。
缓存写回也必须覆盖后台缓冲的实际全部修改，包括双缓冲历史补绘，不能只依据当前控件变化范围。

参考：[按需绘制](https://docs.slint.dev/latest/docs/rust/slint/platform/software_renderer/struct.MinimalSoftwareWindow)、
[软件渲染器](https://docs.slint.dev/latest/docs/rust/slint/platform/software_renderer/struct.SoftwareRenderer)、
[缓冲策略](https://docs.slint.dev/latest/docs/rust/slint/platform/software_renderer/enum.RepaintBufferType)。

## 官方和作者资料

- [Slint MCU Rust 移植指南](https://docs.slint.dev/latest/docs/rust/slint/docs/mcu/)
- [Slint 嵌入式平台](https://docs.slint.dev/latest/docs/slint/guide/platforms/embedded/)
- [Slider](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/basic-widgets/slider/)
- [ComboBox](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/basic-widgets/combobox/)
- [ListView](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/views/listview/)
- [双缓冲重绘策略](https://docs.slint.dev/latest/docs/rust/slint/platform/software_renderer/enum.RepaintBufferType)
- [Slint 许可与价格](https://slint.dev/pricing)
- [Slint 许可说明](https://slint.dev/blog/making-slint-desktop-ready)
- [bugadani/embedded-gui](https://github.com/bugadani/embedded-gui)
