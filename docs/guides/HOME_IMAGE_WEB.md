# 网页更新主页图片

设备连接 Wi-Fi 后，在同一局域网的浏览器打开 `http://主页显示的IP/`。
网页由 P4 本地提供，不需要外网或第三方服务。选择图片、查看预览，然后点击保存到主页。
状态显示保存成功后可断电；恢复默认图片也保存到 Flash，重启后继续使用默认鲸鱼娘。
当前只支持 `desktop_tiny_ui` 的主页，Slint 后端未分配图片播放内存，网页会报告存储不可用。

## 图片格式与限制

- 静图：PNG、JPEG、WebP；动图：GIF。其他动画格式暂不支持。
- 网页在浏览器解码、处理 GIF 帧合成/透明/处置方式，等比居中缩放，并转换为完整帧 RGB565 RLE。
- 两档精度 350×326 / 175×163；两者都铺到同一主页图片区域，低精度档使用最近邻放大。
- 源文件最大 8 MiB，GIF 最多 300 帧、画布面积不超过 2048×2048。
- 单份转换后载荷最多 4 MiB 减 4 KiB。复杂动图可能比原 GIF 大，网页会拒绝超限并提示降低精度或裁短。
- 动画间隔最短 80 ms，快速帧在浏览器中合并采样；板上按时间跳过过期帧，循环播放，离开主页暂停解码。
- 上传不成功或数据无效不会替换当前图片。网页恢复默认会清除当前选择，不修改原始编译内嵌资源。

## 存储与并发

`config/partitions-keyboard.csv` 将应用区设为 `0x10000..0x700000`（约 6.94 MiB），
新增 `images` 数据分区 `0x700000..0xf00000`，8 MiB 分为两个 4 MiB 槽。
原 `kbdmap` 与 `settings` 地址不变，未自动格式化 SD 卡。首次启用需随固件烧录新分区表。
不要再用旧的整片大应用分区布局刷入大于新应用区的固件，否则可能覆盖图片。

每槽首 4 KiB 为提交头：magic、序号、长度、载荷 CRC32 和头 CRC32。
先擦写非活动槽，读回校验载荷，再最后写提交头；重启优先使用序号较新的有效槽，损坏则尝试另一槽。
恢复默认用长度为零的提交记录表示，保留同样的提交语义。

PSRAM 使用两个 4 MiB 上传/播放压缩数据缓冲及一个 350×326 RGB565 解码画布；
不与 MIPI 双缓冲、音频 SRAM 或 SD 卡读卡器空间重叠。旧播放缓冲须经显示核释放才能重新上传。
UI 只重绘主页图片区，不在 USB 中断内解码 GIF，不挂载正在由电脑管理的 SD 文件系统。

Flash 操作由原配置保存任务统一拥有，使用同一显示核 SRAM park 握手。
保存期间暂停旧动图推进，避免每次 Flash 握手等待整帧绘制。每个 4 KiB 扇区擦除/编程后解除 park 并让出调度；擦除仍可能阻塞音频，
不能保证保存图片期间完全无卡音。正常播放不写 Flash。

## HTTP 与安全边界

- GET `/` / `/app.js`：本地网页资源；GET `/state`：可用、忙、完成序号及图片字节数。
- POST `/image`：`application/octet-stream` 的 P4IMAGE1 数据；POST `/default`：相同类型、空正文。
- 头部上限 2 KiB，要求 Content-Length，拒绝 chunked 与重复长度；请求超时受限。
- 校验所有帧尺寸、数量、时长、RLE 行程总像素和文件尾，再进入 Flash 保存。
- 修改请求检查 Origin，无跨域授权。未实现登录或 HTTPS，仅用于可信局域网，不应暴露到公网。

## 构建与依赖

固件继续为纯 Rust；浏览器端使用 gifuct-js 2.1.2 与 js-binary-schema-parser 2.0.3（MIT）。
网页源码、已打包 JS 和两份许可证在 `assets/web/`；固件内嵌打包资源，不依赖电脑运行 Node。
修改网页 JS 后，用固定版本 esbuild 0.28.2 打包：在临时构建目录安装上述依赖和 esbuild，
复制 `home-image.js`，执行 `esbuild home-image.js --bundle --minify --outfile=项目/assets/web/home-image.bundle.js`。

验证采用 release 构建、静态检查和真实硬件 HTTP/重启操作，不新增单元测试或测试脚本。
2026-11-01 已通过：全部例程 release 构建、分区表解析、921600 烧录；
浏览器静图转换和上传（160,572 字节）、八帧 GIF 转换和上传（13,552 字节）；
复位及重新烧录后日志恢复相同 GIF 载荷。HTTP 无效图像返回 422、超限返回 413、
重复 Content-Length 返回 400、跨域修改返回 403，均不提交新图片。

初次保存曾出现整屏红色：原 DPI 驱动未恢复 invalid-LLI 状态。
补丁启用 bit 13 事件，重置 VALID 并写回 SRAM 后发出 BLK_TFR_RESUMEREQ。
重测大图保存实际记录六次恢复，用户确认显示恢复鲸鱼娘与顶部状态。
补丁详见 `3rd/esp-hal/LOCAL_PATCHES.md`，参考官方
[DW-GDMA LL](https://github.com/espressif/esp-idf/blob/master/components/esp_hal_dma/esp32p4/include/hal/dw_gdma_ll.h)。

保存时观测到音频补零，音频 DMA 重启计数为零；保存完成后恢复正常。
保存期间暂停动画、采用 4 KiB 批量编程后，160,572 字节图片实测保存耗时 2,128 ms，
期间恢复一次 invalid-LLI，网页确认保存成功。此次电脑未发送音频，不能因此宣称擦写无卡音。
新版启动日志确认恢复此前静图；VIA v13 读取通过，VIA FAT 盘与 SD exFAT 盘均正常枚举。
尚未验证真实断电中断写入、全部 GIF 处置组合及长时间最大容量动画。

参考：[Embassy TCP API](https://docs.rs/embassy-net/0.9.1/embassy_net/tcp/index.html)、
[gifuct-js 源码及帧处置说明](https://github.com/matt-way/gifuct-js)。
