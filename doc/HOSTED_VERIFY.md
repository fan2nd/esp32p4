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
  [本次成功的 CI 构建](https://github.com/fan2nd/esp32p4/actions/runs/41090067693)。
- C5 配置 `CONFIG_EH_TRANSPORT_CP_SDIO_MODE_PACKET=y`、
  `CONFIG_EH_TRANSPORT_CP_SDIO_CHECKSUM=y`，保留 Wi-Fi + BLE、RPC v2。
  原始 v3.0.10 的 Packet 分支不能编译；本次使用官方修复，无自定义 C 源码修改。
- `src/driver/hosted_v3.rs` 用 Rust 回复 v3 PRIVATE INIT 能力握手；
  检查帧校验和、C5 ID、RPC v2、Packet 模式及双向 1536 字节缓冲。
  RPC 与 SDIO 数据传输由 Embassy 库处理。
- 等待接收事件时同时保留 10 ms 定时状态检查。实测仅等待中断时枚举成功但收不到 INIT；
  加状态检查后握手与 RPC 成功。`hosted_bus.rs` 输出枚举命令日志辅助诊断。

## 构建与检查

```sh
cargo build --release --bin verify_hosted --locked
```

握手适配层的主机测试需从工程目录外运行，以避开 P4 target/build-std 配置：

```sh
cd /tmp
cargo test --manifest-path /Users/fan/esp32p4/tests/hosted/Cargo.toml --target aarch64-apple-darwin --locked
```

已通过 release 构建及 4 项握手/透传测试。Clippy 普通检查通过，现有音频、显示、
触摸模块有 4 项警告，因此严格 `-D warnings` 检查不通过。

2026-10-31 已在 P4 rev 3.1 + C5 上实际烧录独立固件并通过：
初始化、3 次版本查询（3.0.10）、3 次扫描（每次返回16条，达到容器容量），
以及随后45秒心跳观察。日志：`target/hosted-verify/serial-standalone.log`。
首次枚举超时是 C5_BOOT 尚未释放；释放后枚举成功。随后仅等待中断时INIT未到，
增加10ms状态检查后完整验证通过。

## 烧录顺序

先烧 C5，再烧 P4。串口名以实际连接为准；C5 STLink 曾为 `/dev/cu.usbmodem102`，
P4 CH343 曾为 `/dev/cu.usbmodem5B910652311`，可用 `ls /dev/cu.*` 查看。

C5：按住 C5 BOOT，复位或重新上电后松开，让它进入下载模式。使用本次
`c5-hosted-v3-packet` 目录的 merged.bin，从地址 `0x0` 烧录；不要使用旧的 SW_AGGR 固件。
烧完释放 C5 BOOT，重新上电。

```sh
espflash write-bin --chip esp32c5 --port /dev/cu.usbmodem102 --before no-reset --after no-reset --no-stub --non-interactive --skip-update-check 0x0 /Users/fan/esp32p4/target/firmware/c5-hosted-v3-packet/esp-hosted-v3.0.10-packet-fix-esp32c5-sdio-wifi-ble-merged.bin
```

P4：产物位于 `target/firmware/p4-hosted-standalone/`，`verify_hosted.elf` 可直接由
`espflash flash --chip esp32p4 --port <P4串口> --monitor verify_hosted.elf` 烧录。
`verify_hosted-merged.bin` 是 16 MB flash / DIO 80 MHz / 最低 rev 3.1 的合并镜像，
若使用 `write-bin` 则写到 `0x0`。若烧录后没有启动日志，松开 BOOT 并按实体 RESET。

```sh
espflash flash --chip esp32p4 --port /dev/cu.usbmodem5B910652311 --flash-size 16mb --flash-mode dio --flash-freq 80mhz --min-chip-rev 3.1 --monitor --skip-update-check /Users/fan/esp32p4/target/firmware/p4-hosted-standalone/verify_hosted.elf
```

## 预期结果

独立测试程序自动初始化 C5，连续执行 3 次版本查询与 Wi-Fi 扫描，然后观察 45 秒心跳。
扫描只记录 AP 数量，不连接 Wi-Fi，不打印 SSID。全部成功后才输出：

```text
hosted: PASS init + 3 version RPCs + 3 scans + 45s heartbeat
```

失败会打印 `hosted: FAIL` 或拒绝 INIT 的具体原因。构建通过和握手单元测试通过
不能代替这条板上 PASS 日志；此检查也不验证 BLE、联网吞吐或音频。
