# 真窗口冒烟清单（阶段七 / WP-1.x）

三平台（Windows / macOS / Linux）手工或半自动冒烟。CI 仍以 `RUI_FORCE_SOFTWARE=1` 离屏为主；本表覆盖**真窗口**路径。

**WP-1.1（IME）细则与自动化命令**：[ime.md](ime.md)。

## 前置

- 本地：`cargo run -p gallery`（或 `hello_world`）
- IME 诊断（测 #3–#4 时建议）：`$env:RUI_IME_DIAG="1"`；`$env:RUST_LOG="rui=info,warn"`
- 记录：OS 版本、GPU/驱动、IME 名称（中/日/韩）

## 清单

| # | 步骤 | 期望 | Win | macOS | Linux |
|---|------|------|-----|-------|-------|
| 1 | 启动主示例窗口 | 窗口可见、首帧非黑屏 | ☐ | ☐ | ☐ |
| 2 | 点击 Input，输入 ASCII | 文本出现、光标移动 | ☐ | ☐ | ☐ |
| 3 | 切换系统 IME，输入中文/日文 | 预编辑可见；上屏不丢字；见 [ime.md](ime.md) | ☑ 2026-08-09 | ☐ | ☐ |
| 4 | 滚动含 Input 的 ScrollArea（组字中） | IME 候选窗仍贴近光标；不漂移 | ☑ 2026-08-09 | ☐ | ☐ |
| 5 | Tab / Shift+Tab 遍历可聚焦控件 | 焦点环可见；顺序合理 | ☑ 2026-08-09 | ☐ | ☐ |
| 6 | Combobox：方向键 + Enter | 选项可选中并关闭 | ☑ 2026-08-09 | ☐ | ☐ |
| 7 | Escape 关闭最上层浮层 | 仅关闭顶层 | ☑ 2026-08-09 | ☐ | ☐ |
| 8 | 打开次窗并关闭 | 主窗仍存活；无 panic | ☐ | ☐ | ☐ |
| 9 | 读屏：聚焦 Input（见 [a11y_smoke.md](a11y_smoke.md)） | 读出名称/值；Button 可激活 | ☐ | ☐ | ☐ |

## WP-1.1 验收标准（摘要）

| 项 | 标准 |
|----|------|
| 自动化 | `cargo test -p rui --lib ime_support` 通过 |
| #3 | 拼音上屏完整；中间插入正确；选区+组字替换正确 |
| #4 | 滚动/HiDPI 下候选窗跟随 caret（`RUI_IME_DIAG` 中 `cursor` 的 `phys`/`visual` 随滚动变化） |

## 失败时

1. 附 `RUST_LOG` / `RUI_IME_DIAG` 日志片段  
2. 注明是否 `RUI_FORCE_SOFTWARE`  
3. 开 issue，标签 `smoke` / `ime` / `a11y`
