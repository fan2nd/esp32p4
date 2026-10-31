# P4 / C5 ESP-Hosted 通信验证

P4 使用独立的 `verify_hosted` Rust 测试程序，不初始化 MIPI、PSRAM、音频、
Ethernet 或 mDNS。主程序 `main` 保持原样。SDIO 使用 slot 1，20 MHz，CLK=18、CMD=19、
D0..D3=14..17，GPIO54 控制 C5 EN。

## 固定版本与兼容配置

- Embassy 固定提交 `cd5f2d0e695e69fa474bda27710094d91bd93aa4`。
  crates.io 的 `0.3.0` 尚不包含此 SDIO/MCU 接口。
- C5 使用官方 ESP-Hosted MCU 提交 `18574265e0ef4ef09cb31454e8930fc29ae301c1`，
  版本 RPC 仍为 3.0.10，但包含官方 Packet 模式修复（不是原始 v3.0.10 tag）。
  [上游修复](https://github.com/espressif/esp-hosted-mcu/commit/18574265e0ef4ef09cb31454e8930fc29ae301c1)。
- C5 配置 `CONFIG_EH_TRANSPORT_CP_SDIO_MODE_PACKET=y`、
  `CONFIG_EH_TRANSPORT_CP_SDIO_CHECKSUM=y`，保留 Wi-Fi + BLE、RPC v2。
  原始 v3.0.10 的 Packet 分支不能编译；本次使用官方修复，无自定义 C 源码修改。
- `src/driver/hosted_v3.rs` 用 Rust 回复 v3 PRIVATE INIT 能力握手；
  检查帧校验和、C5 ID、RPC v2、Packet 模式及双向 1536 字节缓冲。
  RPC 与 SDIO 数据传输由 Embassy 库处理。
- 等待接收事件时保留 10 ms 定时状态检查，避免仅依赖中断导致握手停滞。

## 构建与检查

```sh
cargo build --release --bin verify_hosted --locked
```

已有项目自有测试与主机测试包已移除。此前 release 构建及普通 Clippy 检查通过，现有音频、显示、
触摸模块有 4 项警告，因此严格 `-D warnings` 检查不通过。

板上独立验证已通过初始化、版本查询、连续扫描及心跳观察；扫描结果受容器容量限制。

## 烧录顺序

先烧 C5，再烧 P4。串口名以实际连接为准，macOS 可用 `ls /dev/cu.*` 查看。
以下命令从工程目录执行，并先将 `C5_PORT`、`P4_PORT` 设为对应板卡串口。

C5：按住 C5 BOOT，复位或重新上电后松开，让它进入下载模式。使用本次
`c5-hosted-v3-packet` 目录的 merged.bin，从地址 `0x0` 烧录；不要使用旧的 SW_AGGR 固件。
烧完释放 C5 BOOT，重新上电。

```sh
espflash write-bin --chip esp32c5 --port "$C5_PORT" --before no-reset --after no-reset --no-stub --non-interactive --skip-update-check 0x0 target/firmware/c5-hosted-v3-packet/esp-hosted-v3.0.10-packet-fix-esp32c5-sdio-wifi-ble-merged.bin
```

P4：产物位于 `target/firmware/p4-hosted-standalone/`，`verify_hosted.elf` 可直接由
`espflash flash --chip esp32p4 --port "$P4_PORT" --monitor verify_hosted.elf` 烧录。
`verify_hosted-merged.bin` 是 16 MB flash / DIO 80 MHz / 最低 rev 3.1 的合并镜像，
若使用 `write-bin` 则写到 `0x0`。若烧录后未启动，松开 BOOT 并按实体 RESET。

```sh
espflash flash --chip esp32p4 --port "$P4_PORT" --flash-size 16mb --flash-mode dio --flash-freq 80mhz --min-chip-rev 3.1 --monitor --skip-update-check target/firmware/p4-hosted-standalone/verify_hosted.elf
```

## 预期结果

独立测试程序自动初始化 C5，连续执行 3 次版本查询与 Wi-Fi 扫描，然后观察 45 秒心跳。
该程序不连接 Wi-Fi。构建与静态检查通过不能代替板上验证，
此检查也不验证 BLE、联网吞吐或音频。
