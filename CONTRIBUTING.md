# 贡献指南

感谢你愿意给 BitCat 出力。这是一个 Windows 优先的 Rust workspace（`core` / `app` / `xtask`）+ 无框架前端的项目，动手前花三分钟读完本页。

## 环境准备

- Windows + Visual Studio（含 C++ 桌面开发 workload）+ Rust stable
- SDL2 通过 `sdl2 = { features = ["bundled", "static-link"] }` 静态链接，**无需单独安装**
- Linux/macOS 也可以开发：`core` 与前端可编译可测，`app` 的 Windows FFI 部分只在 Windows 参与

日常命令统一走 Makefile（PowerShell / cmd / Git Bash 通用），Windows 下设置过一次即可：

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"
```

```bash
make build        # 开发构建（含配置复制）
make run          # 运行
make test-fast    # core 快速测试（~20s）
make test         # 完整 workspace
make release      # 优化构建
```

前端测试：`cd app/frontend && npx vitest run`

## 开发流程

1. Fork + 分支，或直接在 master 上开短分支
2. 首次运行 `cargo test` 会自动安装 cargo-husky hooks：pre-commit 跑 `cargo fmt --check`，pre-push 跑 fmt + clippy + `make test-fast`
3. 提交信息用约定式提交：`feat(scope): ...` / `fix(scope): ...` / `docs(scope): ...`，summary 用简短中文或英文祈使句
4. 开 PR，按模板填自检；**用户可见的改动必须先读 [docs/product/design-spec.md](docs/product/design-spec.md)**（验收标准），并在 PR 里附「产品自检」

## 项目约定（摘录，全文见 CLAUDE.md）

- 日志只用 `tracing`，大文本必须 `log_preview()` 截断，禁止裸写用户/AI 文本
- Rust 字符串处理中文必须按字符边界，禁止字节索引切片
- 每个 `.rs` 文件顶部有 `//!` 模块文档（做什么 / 为什么 / 与谁交互）
- 记忆检索坚持 grep-first，不引入 Embeddings / Vector RAG
- 不做关键词意图分类——让模型自己选工具，Rust 只管 schema、校验、权限和执行
- 临时产物放 `.playwright-cli/`，调研文档放 `docs/research/`

## License

提交 PR 即表示你同意以 [MIT License](LICENSE) 授权你的贡献。
