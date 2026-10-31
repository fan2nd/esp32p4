# Wi-Fi + BLE 独立例程

`src/bin/demos/wifi_ble.rs` 仅启动 ESP32-P4 主核、C5 SDIO 和 Embassy 任务。
不初始化屏幕、触摸、PSRAM、副核、USB、音频或以太网。P4 应用全部为 Rust，
沿用 C5 上已有的 ESP-Hosted MCU 3.0.10 固件。

默认行为：

- Wi-Fi 扫描最多 16 个 AP，按 RSSI 排序，结果包含 SSID、信号强度和认证方式。
  每轮完成后等待 15 秒再扫描，不自动选择或连接网络。
- BLE 发送 HCI Reset、读取地址、配置广播；每条命令收到成功完成事件后才继续。
  广播名 `P4-WiFi-BLE`，间隔 250 ms，使用全部三个广播信道。
- 广播为不可连接广播，用于验证与 Wi-Fi 共存；当前没有 GATT 服务或配对功能。
- Wi-Fi 与 BLE 共用同一个 ESP-Hosted SDIO runner，网络层使用 embassy-net 0.9.1。

## 编译、烧录

以下 `COMx` 须替换为实际板卡串口。

```powershell
cargo build --release --bin wifi_ble
espflash flash --port COMx --baud 921600 --chip esp32p4 --partition-table config/partitions-keyboard.csv --monitor target/riscv32imafc-unknown-none-elf/release/wifi_ble
```

SDIO 引脚：CLK18、CMD19、D0～D3 为 14/15/16/17；C5 RESET54，20 MHz。
只需 Type-C 下载/串口连接。未使用显示外设，因此屏幕不作为运行状态指示。
烧录沿用键盘分区布局，保留已有键盘配置区域。

如需测试连接和 DHCP，在构建时设置 `P4_WIFI_SSID`、`P4_WIFI_PASSWORD` 环境变量，
再执行上述构建/烧录。密码会作为编译常量包含在测试固件中。
每轮扫描后，未连接时会尝试连接配置的 AP；成功后由 embassy-net 获取 IP。
删除这两个环境变量并重新构建可恢复默认扫描模式。

## 验证结论与限制

独立固件已通过 C5 初始化、Wi-Fi 扫描、BLE 广播命令及电脑端空口扫描，
静态包池主机测试通过。扫描受容量上限限制；默认扫描模式的连接/DHCP 尚未验证。

BLE 广播参数和命令布局核对来源：[Zephyr HCI 定义](https://github.com/zephyrproject-rtos/zephyr/blob/main/include/zephyr/bluetooth/hci_types.h)。
