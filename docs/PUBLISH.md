# Publish checklist（crates.io）

面向 `rui` / `rui_macros` / `rui_style_tokens` 首次（或版本）发布。

## 1. 版本与 SemVer

| Crate | 路径 | 建议 |
|-------|------|------|
| `rui` | `crates/rui` | 与 workspace 同步；破坏性 API → minor（0.x）或 major（≥1.0） |
| `rui_macros` | `crates/rui_macros` | 与 `rui` 同 major/minor；proc-macro 破坏性变更同步 bump |
| `rui_style_tokens` | `crates/rui_style_tokens` | utility 表删除/改名 → 视为 breaking |

发布前：

```text
# 三 crate Cargo.toml version 对齐或文档说明依赖范围
cargo package -p rui_style_tokens --allow-dirty
cargo package -p rui_macros --allow-dirty
cargo package -p rui --allow-dirty
```

## 2. Features（`rui`）

| Feature | 默认 | 说明 |
|---------|------|------|
| （default） | 开空集 | 精简：无 resvg、无大字体嵌入 |
| `runtime-logging` | 关 | `env_logger` |
| `tracing` | 关 | 热路径 span |
| `system-tray` | 关 | 托盘 stub 门闸（见 `docs/tray.md`） |
| `backend-dx12` | 关 | Windows D3D12 |
| `bundled-*` / `fonts-*` | 关 | 字体策略 |
| `emoji-svg` | 关 | resvg Twemoji |

文档 / README 须列出上述表；默认 feature 保持精简。

## 3. 元数据

- [ ] `license = "MIT"`、`repository`、`description`、`readme`
- [ ] `keywords` / `categories` 有效
- [ ] 路径依赖在发布前改为 crates.io 版本（`rui` → `rui_macros` / `rui_style_tokens`）
- [ ] CHANGELOG 或 GitHub Release 笔记

## 4. 质量门

```text
cargo fmt --all -- --check
cargo clippy -p rui --all-targets -- -D warnings
cargo test -p rui --lib
cargo test -p rui --lib offscreen
cargo check -p gallery   # 或 hello_world / calculator
```

可选：`cargo bench -p rui --bench layout_baseline`（见 `docs/size_and_memory.md` release checklist）。

## 5. 发布顺序

1. `rui_style_tokens`
2. `rui_macros`（依赖 tokens 若有）
3. `rui`

```text
cargo publish -p rui_style_tokens
cargo publish -p rui_macros
cargo publish -p rui
```

## 6. 文档站

静态大纲见 [`docs/site/README.md`](site/README.md)；crates.io README 链到 Getting Started。
