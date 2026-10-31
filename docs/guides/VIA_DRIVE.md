# VIA 定义只读盘

`desktop` 与 `desktop_tiny_ui` 共用的复合 USB 提供 `P4 VIA` 只读盘。
盘内 `VIA.json` 来自 `assets/p4-desktop-via.json`，与桌面的 `CAFE:4014`
VIA Raw HID 接口配套。启用 VIA 的 Design 页后手动选择盘中的 JSON；
挂载磁盘不会自动导入布局，也不会代替浏览器的 HID 授权。

这是 1 MiB 的合成 FAT12 卷，使用 VFAT 长文件名保留 `.json` 扩展名。
盘容量不是设备 Flash 容量。仅公开编译时内嵌的定义，不提供键位备份、
固件升级或用户配置访问；不新增分区，也不允许格式化、写文件或删除文件。
修改定义需要重新构建固件。主机可能尝试写索引，设备会返回写保护错误。

## USB 资源与执行方式

使用 USB HS 控制器及 PHY，强制全速运行；bulk 最大包长 64 字节。

| 资源 | 分配 |
| --- | --- |
| 接口 | UAC1 3 个、HID 3 个、MSC 1 个，共 7/8 |
| IN 端点 | EP0、反馈、麦克风、键盘、Consumer、Raw HID、MSC，共 7 个 |
| OUT 端点 | EP0、扬声器、Raw HID、MSC，共 4 个 |
| FIFO | RX 47 + 16 + 9 + 8 + 16，TX 7 × 16，共 208/896 words |
| OUT 暂存 | EP0 64 + 音频 34 × 8 + Raw HID 32 + MSC 64 = 432/512 bytes |

MSC 在 Core 0 与既有 USB/audio futures 协作运行，每个 bulk 包后让出执行权。
只保留一个 512 字节扇区和短命令缓冲，不分配堆、不复制整盘到 RAM。
磁盘读取不调用 Flash 存储驱动，音频、HID 和原 VIA 持久化路径保持独立。

## 协议与边界

`src/driver/usb_msc/` 分离 FAT 合成、逐 LUN SCSI、BOT 和总线控制桥。
MSC 使用 SCSI transparent、Bulk-Only Transport；LUN 0 为 VIA 只读盘，
LUN 1 为 [microSD 读卡器](SD_READER.md)，共用接口和端点。支持常见容量、
查询、读写块、sense、模式页、弹出命令；仅 VIA 盘写入返回 DATA PROTECT。
不支持的命令返回 ILLEGAL REQUEST，越界读取被拒绝。

无效 CBW 不产生 CSW，必须先 BOT reset 再清除两个端点的 halt。
阶段错误返回 phase-error CSW 并等待恢复。BOT reset 会取消当前传输，
在 EP0 应答前清理 MSC 队列和专用 IN FIFO，保留 halt/PID；主机的
CLEAR_FEATURE 再重置 DATA0。USB 新配置会话重新载入虚拟介质。
控制桥共享总线所有权，依赖 Embassy 在处理控制请求前取消 bus poll 的约定。

重新烧录会使整个复合 USB 设备断开并重新枚举，音频会短暂中断。
不要对虚拟盘进行格式化；实际挂载行为取决于主机 USB/文件系统支持。
跨 Windows/Linux/macOS 的错误恢复和长期音频负载仍需分别做硬件验收。

协议依据：[USB-IF BOT 1.0](https://www.usb.org/sites/default/files/usbmassbulk_10.pdf)。
现有 Embassy 0.5.1 不含 MSC；本模块是项目内的有限 SCSI/BOT 实现，未复制新的第三方库。
底层 DWC2 继续使用已保留许可证的本地补丁依赖，详见
[驱动补丁](../../3rd/embassy-usb-synopsys-otg/PATCHES.md)。

## 2026-11-01 枚举修复与实机验证

在 P4 v3.1 / Windows 上复现 WIP 分支的未知 USB 设备问题，串口捕获到
Embassy USB 0.5.1 `src/lib.rs` 字符串描述符编码路径的
`control buffer too small` panic。新增产品名
`P4 Desktop Keyboard + Audio + VIA Drive` 编码后需要 80 字节，而共享
运行代码只分配了 64 字节控制请求缓冲。

控制缓冲改为 256 字节，覆盖 USB 字符串描述符容量及当前 Embassy 的边界检查。
EP0 最大包长仍为 64 字节，由协议栈分包；不需要减少 MSC 端点、改 VID/PID，
也不改变音频、无线和屏幕参数。用于复现的直接串口 panic 输出已撤回，继续
遵守既有日志输出路径。未新增单元测试或测试脚本。

已验证：

- `desktop_tiny_ui` release 构建、烧录和启动；Slint `desktop --features slint-ui`
  release 构建通过（本轮未烧录 Slint）。
- Windows 正常识别复合设备、MSC、HID 和音频接口，挂载 `P4 VIA` FAT 盘。
- 读取盘内 `VIA.json`（382 字节），SHA-256 与编译输入文件完全一致。
- 现有 VIA 探针读取四层键码、宏容量和协议版本成功，未修改键位。
- USB-A 实际拔插后再次配置成功，磁盘重新挂载，JSON 哈希与 VIA 探针再次通过；
  用户确认屏幕和声音正常。
- 主机三秒音频播放完成；观察到的播放统计 empty/drop/restart 均为 0。
- 麦克风接口可采集三秒、48,000 个样本，但存在削波，不能据此确认录音音质；
  本次未调整麦克风增益。

跨系统兼容性、BOT 异常恢复及长期并发负载尚未完成验证。
