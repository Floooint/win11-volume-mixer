# 开发环境与规范

本项目使用 Tauri 2 + React + TypeScript + Rust，不需要 Python 虚拟环境（`.venv`）。

动手前先阅读：[项目说明](docs/readme.md)、[架构设计](docs/architecture.md)、[开发路线](docs/roadmap.md)。

## 开发环境

| 工具 | 要求 | 本机版本（2026-09-28） |
| --- | --- | --- |
| Node.js | 24（见 `.node-version`） | v24.16.0 |
| pnpm | 12.x | 12.6.0 |
| Rust 工具链 | stable，`x86_64-pc-windows-msvc` | 1.98.1 |
| Visual Studio Build Tools 2022 | `VCTools` 工作负载（含 Windows SDK） | 17.14.41 |
| WebView2 Runtime | Windows 11 自带 | 自带 |
| Git | 任意较新版本 | 2.45.1 |

安装方式（本机实际使用）：

```text
npm install -g pnpm
winget install --id Rustlang.Rustup
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

- Node 安装在 `C:\Program Files` 时，`corepack enable` 需要管理员权限，因此改用 npm 全局安装 pnpm（装在用户目录）。
- 安装 VS Build Tools 时需要在 UAC 弹窗中确认，否则会以 `0x8013153B`（已取消）失败。
- 安装 Rust 后需重新打开终端，`~/.cargo/bin` 才会加入 PATH。
- Node 版本管理工具（fnm / Volta）可选，会读取 `.node-version` 自动切换。

项目骨架创建后：

- 在 `package.json` 中用 `packageManager` 字段固定 pnpm 版本。
- 在 `src-tauri/rust-toolchain.toml` 中固定 Rust 版本。
- 更新上表为实际固定的版本。

## 依赖管理

- 前端依赖通过 `package.json` 和 `pnpm-lock.yaml` 管理。
- Rust 依赖通过 `src-tauri/Cargo.toml` 和 `src-tauri/Cargo.lock` 管理。
- 提交锁文件；不提交 `node_modules/`、`target/` 和构建产物（见 `.gitignore`）。
- 新增依赖前先确认必要性，并在 [技术栈](docs/tech_stack.md) 中登记。

## 编码规范

### 通用

- 文件使用 UTF-8、LF 换行（见 `.editorconfig`、`.gitattributes`）。
- 界面文案使用中文；代码标识符使用英文。
- 注释说明“为什么”，不重复代码本身。

### Rust

- 使用 `cargo fmt` 格式化，`cargo clippy` 无警告。
- 所有 COM 调用只放在 `audio/` 模块的音频线程中，遵守 [架构设计](docs/architecture.md) 中的线程规则。
- `unsafe` 块尽量小，并注释其安全前提。
- 返回前端的错误统一使用 `AppError`，不使用 `unwrap()` 处理可能失败的系统调用。

### TypeScript / React

- 开启 `strict`。
- 前后端类型只使用 tauri-specta 生成的 `src/bindings.ts`，不手写重复类型。
- 组件只负责展示和交互，状态与后端通信放在 Zustand store 中。

## 开发命令

骨架创建后补充，预计包括：

```text
pnpm install          # 安装依赖
pnpm tauri dev        # 启动开发模式
pnpm tauri build      # 构建安装包
pnpm lint             # 前端检查
cargo fmt / clippy / test（在 src-tauri 下）
```

## 验证流程

提交前至少完成：

1. `cargo fmt --check`、`cargo clippy`、`cargo test` 通过。
2. 前端类型检查和 lint 通过。
3. 涉及音频逻辑时，手动对照系统音量合成器验证：调节、静音、外部修改同步、切换输出设备。

## 文档编码

- 所有 Markdown 文件使用 UTF-8 编码。
- 写入中文时避免经过未配置 UTF-8 的命令行管道。
- 写入后重新读取文件，确认中文未被替换为问号或乱码。
