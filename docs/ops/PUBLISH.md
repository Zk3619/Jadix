# Publish checklist（crates.io）

面向四 crate 首次（或版本）发布。**Step 0**：主名已锁定 **`jadix`**（琢玉），见 [`NAMING_DECISION.md`](../archive/NAMING_DECISION.md)；**workspace 重命名尚未执行**，发布仍阻塞于重命名落地。下文以 `jadix` 为准；热重载为 **`jadix_hot_reload`**（无 `_cli`）。

## 0. 发布模型（W4 决策）

- **Step 0（先于 publish）**：[`NAMING_DECISION.md`](../archive/NAMING_DECISION.md) 选型 **已完成** → 待 workspace 重命名 + 迁移说明。
- 采用 **Workspace + 伞形 crate** 模式：
  - 用户主入口：`jadix`（`cargo add jadix`）。
  - 核心子 crate：`jadix_macros`、`jadix_style_tokens`、`jadix_hot_reload`。
- `jadix_hot_reload` 采用 **策略 A（核心模块）**：与主 crate 及另两个子 crate 同版本发布，属于 Beta 阻塞闭环。
- 底层核心 crate 拆分口径：
  - 当前：`layout` / `renderer` 等在主 crate 内模块化维护。
  - 目标：Beta 后评估 `jadix_layout` / `jadix_renderer` / `jadix_core`（仍由伞形入口重导出）。

## 1. 版本与 SemVer

| Crate | 路径（重命名前 → 目标） | 建议 |
|-------|--------------------------|------|
| `jadix` | `crates/rui` → `crates/jadix` | 与 workspace 同步；破坏性 API → minor（0.x）或 major（≥1.0） |
| `jadix_macros` | `crates/rui_macros` → `crates/jadix_macros` | 与主 crate 同 major/minor；proc-macro 破坏性变更同步 bump |
| `jadix_style_tokens` | `crates/rui_style_tokens` → `crates/jadix_style_tokens` | utility 表删除/改名 → 视为 breaking |
| `jadix_hot_reload` | `crates/rui_hot_reload_cli` → `crates/jadix_hot_reload` | 与主 crate 同 major/minor；协议或脚手架行为破坏需同步说明 |

发布前：

```text
# 四 crate Cargo.toml version 对齐（重命名完成后）
cargo package -p jadix_style_tokens --allow-dirty
cargo package -p jadix_macros --allow-dirty
cargo package -p jadix --allow-dirty
cargo package -p jadix_hot_reload --allow-dirty
```

## 2. Features（主 crate `jadix`）

| Feature | 默认 | 说明 |
|---------|------|------|
| （default） | 开空集 | 精简：无 resvg、无大字体嵌入 |
| `runtime-logging` | 关 | 控制台（stderr）+ 可选 `RUI_LOG_FILE`（`env_logger`） |
| `tracing` | 关 | 热路径 span |
| `system-tray` | 关 | 原生托盘（`tray-icon`）；见 [`tray.md`](tray.md) |
| `backend-dx12` | 关 | Windows D3D12 |
| `bundled-*` / `fonts-*` | 关 | 字体策略 |
| `emoji-svg` | 关 | resvg Twemoji |

文档 / README 须列出上述表；默认 feature 保持精简。

## 3. 元数据

- [ ] `license = "MIT"`、`repository`、`description`、`readme`
- [ ] `keywords` / `categories` 有效
- [ ] 路径依赖在发布前改为 crates.io 版本（主 crate → `_macros` / `_style_tokens`）
- [ ] CHANGELOG 或 GitHub Release 笔记

## 4. 质量门

```text
cargo fmt --all -- --check
cargo clippy -p jadix --all-targets -- -D warnings
cargo test -p jadix --lib
cargo test -p jadix --lib offscreen
cargo check -p gallery   # 或 calculator
```

可选：`cargo bench -p jadix --bench layout_baseline`（见 `docs/ops/size_and_memory.md` release checklist）。

## 5. 发布顺序

1. `jadix_style_tokens`
2. `jadix_macros`（依赖 tokens 若有）
3. `jadix`
4. `jadix_hot_reload`

```text
cargo publish -p jadix_style_tokens
cargo publish -p jadix_macros
cargo publish -p jadix
cargo publish -p jadix_hot_reload
```

## 6. 文档站

静态大纲见 [`docs/site/README.md`](../site/README.md)；crates.io README 链到 Getting Started。

## 7. 开源发布补充项（W4）

- [x] Step 0 命名锁定（见 [`NAMING_DECISION.md`](../archive/NAMING_DECISION.md)）；☐ README / CHANGELOG 含「原 RUI → jadix」迁移说明。
- [ ] README 明确 debug 热重载脚手架：先 UI（`cargo run -p gallery`），再 watcher（`cargo run -p jadix_hot_reload -- .`）。
- [ ] README / docs 明确“单仓多子目录 + crates.io 四 crate 分包”策略。
- [ ] 明确 `components/` 随主 crate 发布，当前阶段不单独拆 crate。
- [ ] 对外使用说明包含：最小上手、feature 矩阵、平台差异、已知限制、问题反馈入口。
