# 复合 USB 扩展评估

2026-11-01。触控板已完成 HID 相对鼠标方案及 tiny-ui 页面接入，全部例程构建通过，待实机验证；网卡与 BLE 适配器仍为设计评估。用户已取消 USB 显示器需求；保留 USB 触控板、Wi-Fi 共享网卡及 BLE 适配器。板载屏幕与现有桌面 UI 保留。

## 当前边界

现有设备含键盘、Consumer、VIA、扬声器、麦克风及双 LUN MSC。
项目配置 EMBASSY_USB_MAX_INTERFACE_COUNT=8，已使用 7 个接口；这是软件配置，
不是芯片只允许八种功能。新增类需要重新预算端点、FIFO、OUT 缓冲、描述符和任务调度。
现有 HS 控制器强制以 Full Speed 12 Mbit/s 运行；此前音频在此模式下验证稳定。
恢复 High Speed 480 Mbit/s 需验证音频服务间隔、反馈、包长、描述符及底层驱动，不能只改速度开关。

## 功能路线

| 功能 | 实现路线 | 边界 |
| --- | --- | --- |
| USB 触控板 | 屏幕触摸转 HID；先相对鼠标移动/点击/滚轮，再评估 Precision Touchpad | 精准触控板须完整多指报告、Feature 报告及主机兼容验证，普通鼠标报告不等于精准触控板 |
| USB 网卡共享 Wi-Fi | Windows 11 CDC-NCM，USB 子网 + DHCP + IPv4 NAT + DNS 转发 | 电脑识别以太网接口，Wi-Fi 由板子管理；不是电脑原生无线网卡，也不是透明二层桥接；embassy-net 接口本身不自动提供 NAT 路由器 |
| USB 蓝牙适配器 | USB Bluetooth HCI 转发到 C5 Hosted HCI | C5 仅 BLE，不支持经典蓝牙/A2DP；Windows 对该 LE-only 控制器的绑定和初始化仍需实测。转发模式下电脑独占 HCI，板上 BLE 主机不能同时发命令 |

普通 STA 的三地址 Wi-Fi 链路不能把任意电脑 MAC 当作透明网桥直接转发。
默认采用路由/NAT 实现上网；若要求电脑与路由器同网段，需要另行设计、验证 AP 支持及转发策略。
NCM 在 Windows 11 有系统驱动，Windows 10 需另定兼容方案。

建议推进顺序：HID 触控板 → NCM/NAT → BLE HCI 独立验证。
不再开发 USB 显示器或 Windows 显示驱动；是否恢复 HS 由网络吞吐需求及音频兼容验证决定。全栈继续遵守纯 Rust、不新增项目测试脚本约束；
Windows 驱动如采用 Rust，需要单独评估 WDK/UMDF/IddCx 绑定、签名和部署，不复制 C/C++ 示例实现。

## 官方来源

- [Windows Precision Touchpad 协议](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-protocol-implementation)
- [Windows USB 内置类驱动（NCM、HID、Bluetooth）](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/supported-usb-classes)
- [Windows 间接显示驱动模型](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview)
- [ESP32-C5 产品能力](https://www.espressif.com/en/products/socs/esp32-c5)
- [ESP32-P4 数据手册](https://documentation.espressif.com/esp32-p4_datasheet_en.html)
