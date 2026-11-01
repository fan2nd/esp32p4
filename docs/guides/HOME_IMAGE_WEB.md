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


## 网页天气设置

主页图片网页下方新增天气设置：搜索城市并选择结果，或手动填写名称、纬度、经度；
点击“保存天气位置”后等待“已保存到 Flash”。“关闭天气”清除已保存位置。
城市搜索由浏览器请求 Open-Meteo 地理编码服务（只在点击搜索时发送城市名称）；
保存后 P4 独立请求天气，不依赖浏览器保持打开。

Core 0 使用独立 Embassy TCP 任务查询 `api.open-meteo.com`，联网或更换位置后立即更新，
成功后每 15 分钟更新，失败后每 60 秒重试。关闭 Wi-Fi 时显示离线，避免将旧天气当成当前数据。
请求有 15 秒总超时、4 KiB 响应上限，不持有跨核锁等待网络；Core 1 仅重绘主页天气文字区域。
板端采用 HTTP/1.0 关闭连接分帧，仅接受 200、无压缩且非 chunked 的 JSON 响应。
这是明文天气查询，仅作日常参考，不用于安全或告警决策。
当前温度和 WMO 天气码由 JSON 反序列化，数值范围检查后才发布；天气状况在屏幕上使用英文。

配置保存在现有 sequential-storage KV 中，schema 从 v2 升为 v3，显式迁移 v1/v2，
保持 Wi-Fi 凭据、VIA 和其他设置不变。城市坐标以百万分之一度整数存储，范围严格检查。
实时天气只在 RAM 中，不随每次刷新写 Flash；系统重置清除位置。
GET `/weather` 返回 `[位置或null, 天气文字, 保存状态]`；
POST `/weather` 采用 `application/octet-stream`，正文是 JSON 位置对象或 `null`。
位置对象字段为 `name`（最多 48 UTF-8 字节）、`latitude`、`longitude`（整数，百万分之一度）。
接口与图片上传共用 Origin 校验、头部边界及连接超时；不返回任何 Wi-Fi 密码。

来源与使用边界：
- [Open-Meteo 当前天气字段、WMO 天气码](https://open-meteo.com/en/docs)
- [Open-Meteo 城市搜索 API](https://open-meteo.com/en/docs/geocoding-api)
- [Open-Meteo 使用条款](https://open-meteo.com/en/terms)：免费接口面向非商业使用，数据需署名。

天气功能 2026-11-01 验证：全部例程 release 构建、网页 JS 语法检查、921600 烧录通过；
浏览器实际搜索 Hangzhou、选择浙江杭州并保存，板端取得 24.6 C / Drizzle。
KV 写入耗时 4,555 us；此次查询和保存窗口内音频 empty/drop/restart 均为 0。
硬件复位后恢复位置并重新取得天气；v2 Wi-Fi/VIA 配置成功迁移，VIA v13 读取通过。
纬度 91 度请求返回 422，原配置未改变。用户确认天气文字正常、无图片遮挡和触摸问题。
验证后通过网页关闭天气，清除临时杭州位置，等待用户设置自己的城市。
尚未做长时间天气刷新和网络故障压力验证。


## 网页 UI 框架

设备设置网页使用 [Pico CSS](https://picocss.com/docs) 2.1.1 的卡片、网格、表单、按钮组、
折叠面板、进度条与 aria-busy 加载提示。浏览器业务脚本仍负责图片转换和天气接口。
`assets/web/pico.min.css` 与 `pico.LICENSE.md` 来自 npm 官方包 `@picocss/pico@2.1.1`，
保留上游 MIT 许可证。资源由固件 `/pico.css` 提供，不在浏览器运行时访问 CDN。
更新时将固定版本 npm 包的 `css/pico.min.css` 和 `LICENSE.md` 复制到上述路径，重新构建固件。

框架改版已通过 release 构建、JS 语法及 diff 检查。实机网页验证了本地 Pico 样式加载、
图片转换预览、城市搜索结果联动及经纬度折叠面板；没有改写用户图片或天气位置。


## 后端框架

后端使用 [picoserve 0.20.1](https://docs.rs/picoserve/0.20.1/picoserve/)，
提供 HTTP 解析、路由、静态文件、JSON 响应和连接关闭管理。
该版本与项目 embassy-net 0.9.1 / embassy-time 0.5.1 / embedded-io-async 0.7 匹配；
最低 Rust 声明升为 1.93，本机 stable 1.99 编译，未更换工具链。

静态资源通过 `File` 服务，状态通过 `Json` 序列化；天气 POST 是小型有界请求，
图片 POST 使用 `RequestHandlerService` 与流式 `RequestBodyReader`，每次最多 2 KiB
复制到已独占的 PSRAM bank。Flash 双槽校验提交、图片播放、KV 与双核协作保持原有职责。
上传中断或超时释放 bank，不提交半张图片。

6 个并发 HTTP 处理器，每个请求头缓冲 2 KiB，TCP 收发各 2 KiB；请求开始等待 5 秒，读取请求总时间 20 秒，
响应写入 10 秒，整条连接另有 120 秒上限；一次连接只服务一个请求。
应用继续拒绝重复 Content-Length、Host、Origin、Content-Type，以及 Transfer-Encoding，
修改请求保持同源和二进制 Content-Type 检查。GET/HEAD 与 404/405 由框架路由处理。
框架 finalize 会在返回拒绝响应前有界排空未读取正文，因此仅发请求头但不发完正文的
客户端，可能等到读取超时才收到错误响应；不绕过框架协议处理，也不无界分配内存。

2026-11-01 迁移实机验证：
- 全部例程 release 构建通过，921600 烧录启动，日志标识 picoserve。
- 四个静态资源 GET 200，返回字节与源码资源完全一致；JSON 状态兼容原网页。
- HEAD 返回 200 且无正文；未知路由 404、不支持方法 405。
- 无效图片 422、跨域修改 403、重复长度 400、超限 413、非法天气 JSON 422。
- 截断上传返回 400，随后 busy=false，序号与有效图片未改变。
- 168,112 字节完整图片上传 202，Flash 保存成功，固件保存耗时 2,681 ms；随后恢复默认图片成功。
- 天气 POST 202、KV 保存及真实天气获取成功，最后恢复未设置位置。
- 保存大图期间记录显示 DMA 恢复、音频补零，音频 DMA restart=0；保存后恢复连续运行。
  天气 KV 恰逢擦写时也观测到补零，HTTP 框架迁移不解决 Flash 擦写阻塞。
未进行长期并发/慢客户端压力或公网安全验证；保持可信局域网使用边界。


## 网页偶发缺失 CSS：并行连接修复

用户报告 Mac 网页看起来无 CSS。单独获取 `/pico.css` 返回 200、83,319 字节，
但同时请求 HTML/CSS/JS/状态的六路回归中，有一路返回连接拒绝。
原来只有一个 smoltcp 监听 socket，没有操作系统式 accept 排队；浏览器并行获取资源时
可能遭到拒绝。该复现证明服务器存在并发问题，不能仅据此断言用户 Mac 的唯一原因。

改用 `join_array` 同时轮询 6 个 picoserve 服务连接，并将网络 socket 资源从 7 增至 12。
每连接 TCP 收发从各 4 KiB 调至各 2 KiB，限制 SRAM 增量；图片 bank 独占、Flash
提交及 Core 0 单任务存储逻辑不变。编译期递归限额升至 256 以容纳泛型路由 future，
不改变设备运行时栈深度。CSS 仍使用正确 MIME、本地资源及 no-store，未增加 Mac 专用分支。
修复后全部例程 release 构建通过，921600 烧录启动。
六路并发加载连续三轮全部返回 200，CSS 每次 83,319 字节且与资源原文完全一致；
本次并行 CSS 响应耗时 2.66～3.08 秒。用户在 Mac 上复测确认样式恢复正常。
用户此前反馈 Safari 和 Edge 都出现过该问题。

温度单位使用 `°C`（U+00B0 + C）。页面 10×20 字体采用 embedded-graphics 的 Latin-1 字形集，支持度数符号，避免 ASCII 字体显示缺字；时钟仍用 Inter。
