# USB 触控板

`desktop_tiny_ui` 主页的 `TOUCHPAD` 打开触控板页，`BACK` 返回主页。
本版使用标准 USB HID 相对鼠标报告，电脑使用系统鼠标驱动；不是 Windows Precision Touchpad，
没有系统级三指/四指手势。现有宏键盘、VIA、音频和双 LUN 读卡器保留。

## 操作

- 中央区域单指移动：移动电脑指针。
- 单指轻点：左键单击；双指轻点：右键单击。
- 双指上下移动：滚轮，每 32 个物理像素产生一档；向上划为正滚轮。
- 底部 LEFT / RIGHT：按住对应鼠标键，松手释放。
- 拖动：一根手指按住 LEFT，另一根手指在中央区域移动。

轻点门限为 250 ms、累计移动不超过 12 个物理像素。两指变成一指时不立即恢复
鼠标移动，待全部抬起后开始新手势，避免滚动结束时指针跳动。切换触点集合时重置
坐标基准。离开页面、500 ms 无有效触摸报告或 USB 断开时取消手势，取消不产生轻点。

## 架构与资源

- `src/desktop/touchpad.rs`：手势状态、区域定义和六字节 HID 报告描述符。
- `src/desktop/page.rs`：主页入口及静态触控板页面。
- `src/desktop/display.rs`：Core 1 将完整多点 GT911 报告送入手势处理。
- `src/desktop/mod.rs`：32 项跨核报告队列；拥塞时丢弃旧运动，并先释放再提交最新状态。
- `src/bin/apps/desktop_runtime.rs`：Core 0 独立 HID future 发报告，不在 USB 中断内处理手势。

报告为按钮位 + 有符号 16 位 X/Y 相对位移 + 有符号 8 位滚轮，无 Report ID。
增加一个 HID 接口和一个 interrupt IN 端点（6 字节，1 ms）；现有接口编号保持不变。
接口总计 8/8，IN 端点含 EP0 总计 8 个；不增加 OUT 暂存。
Slint 桌面共用 USB 后端，但本轮触控板页面只接入当前使用的 tiny-ui 桌面。

手指移动不触发整页重绘；USB 收发不持有跨核锁等待。USB 配置、挂起及断开时清空
待发鼠标队列，重新就绪先发全释放报告，避免积压位移在重连时执行。

## 验证

`cargo build --release --locked --bins --features slint-ui` 全部例程构建通过，
`git diff --check` 通过。2026-11-01 已通过 COM6、921600 烧录并自动启动。
Windows 将接口 MI_07 识别为 HID-compliant mouse，状态 OK；VIA v13 四层键位只读
探测通过，VIA FAT 与 SD exFAT 两个卷健康挂载，Wi-Fi 关联与 DHCP 正常。
启动后的持续音频播放统计 empty/drop/restart 均为 0。
实际移动、点击、双指滚动及拖动手势等待用户操作确认；长时间并发负载尚未验证。

协议参考：[Microsoft HID 驱动支持](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/supported-usb-classes)。
精确触控板是另一个完整协议：[Precision Touchpad](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-protocol-implementation)。
