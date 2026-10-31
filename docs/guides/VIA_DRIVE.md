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

`src/driver/usb_msc/` 分离 FAT 合成、只读 SCSI、BOT 和总线控制桥。
MSC 为单 LUN、SCSI transparent、Bulk-Only Transport；支持常见容量、
查询、读块、sense、模式页、弹出命令，所有写入返回 DATA PROTECT。
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
现有 Embassy 0.5.1 不含 MSC；本模块是项目内的有限只读实现，未复制新的第三方库。
底层 DWC2 继续使用已保留许可证的本地补丁依赖，详见
[驱动补丁](../../3rd/embassy-usb-synopsys-otg/PATCHES.md)。
