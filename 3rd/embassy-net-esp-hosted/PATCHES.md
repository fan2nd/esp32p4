# Local changes

Source: https://github.com/embassy-rs/embassy/tree/cd5f2d0e695e69fa474bda27710094d91bd93aa4/embassy-net-esp-hosted

Expose Control::set_wifi_enabled and ESP-Hosted MCU WifiStop RPC, keeping Bluetooth and SDIO running. Other backends reject unsupported stop operations. Uncomment WifiStop IDs and payloads in esp_hosted_rpc.proto and enable existing generated WifiStop request/response types in the filtered Rpc oneof (field IDs 281/537), including encode/decode/size branches. Sibling dependencies retain the original git revision.

Track association and disconnect event generations. Disconnect/stop waits for the old association event before a new connect may start, including events preceding the RPC response; legacy synchronous FG retains reply-based behavior. This prevents a late reason-8 disconnect from failing the next attempt.
