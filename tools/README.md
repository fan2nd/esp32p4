# 开发工具

从仓库根目录运行，先阅读脚本的参数和输入要求：

- `build_cjk_font.py`：生成 `assets/cjk16.bin`；字体授权见 `assets/CJK-FONT-LICENSE.txt`。
- `build_pet_bubble.py`：生成宠物气泡图像资源。

上述资源工具不属于固件构建的必要步骤。现有生成资源保留在 `assets/`，无需为了编译重新生成。

## Rust UI 页面主机测试

```sh
python3 tools/test_rust_ui_demo.py
```

需要 Python 3、Cargo 与 rustc。脚本识别主机 target，仅构建 tiny-ui 及其主机依赖，
从 Cargo JSON 构建产物信息取得依赖路径，再使用 `rustc --test` 运行
`tests/rust_ui_demo.rs`；测试直接引用根 bin 的页面模块，不创建额外 crate，
不构建或烧录 P4 固件。框架测试命令见 [测试导航](../tests/README.md)。
