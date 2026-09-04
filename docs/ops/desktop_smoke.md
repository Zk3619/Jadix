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
| 1 | 启动主示例窗口 | 窗口可见、首帧非黑屏 | ☑ 2026-08-10 | ☐ | ☐ |
| 2 | 点击 Input，输入 ASCII | 文本出现、光标移动 | ☑ 2026-08-10 | ☐ | ☐ |
| 3 | 切换系统 IME，输入中文/日文 | 预编辑可见；上屏不丢字；见 [ime.md](ime.md)（日/韩可选） | ☑ 2026-08-09（中文） | ☐ | ☐ |
| 4 | 滚动含 Input 的 ScrollArea（组字中） | IME 候选窗贴近光标；滚出视口**不失焦**、候选贴顶/底；组字滚动可接受 | ☑ 2026-08-10 | ☐ | ☐ |
| 5 | Tab / Shift+Tab 遍历可聚焦控件 | 焦点环可见；顺序合理 | ☑ 2026-08-09 | ☐ | ☐ |
| 6 | Combobox：方向键 + Enter | 选项可选中并关闭 | ☑ 2026-08-09 | ☐ | ☐ |
| 7 | Escape 关闭最上层浮层（Gallery → 浮层 → Dialog → 打开 Select） | 仅关闭顶层（先关下拉，再 Esc 才关 Dialog） | ☑ 2026-08-09 | ☐ | ☐ |
| 8 | 打开次窗并关闭（Gallery → 浮层 → Secondary window，或 **导航 → Router 外壳 → Open secondary**） | 主窗仍存活；次窗 Input/Select/Dialog 可点；Esc 不关主窗浮层；Bridge 计数两边同步 | ☑ 2026-08-10 | ☐ | ☐ |
| 9 | 读屏：聚焦 Input（见 [a11y_smoke.md](a11y_smoke.md)） | 读出名称/值；Button 可激活 | ☑ 2026-08-10 | ☐ | ☐ |

**签署说明（2026-08-10）**：开发机 **Win**（Kingdom / Win 10.0.26200）#1–#9 已签；#3 为中文 IME（日/韩未测，可选）；macOS / Linux 列仍开放。

## WP-1.1 验收标准（摘要）

| 项 | 标准 |
|----|------|
| 自动化 | `cargo test -p rui --lib ime_support` 通过 |
| #3 | 拼音上屏完整；中间插入正确；选区+组字替换正确 |
| #4 | 候选跟随 caret；滚出视口不失焦（候选贴顶/底）；组字滚动无明显卡顿（`relocate` 热路径不应每帧 EnumWindows） |

## 失败时

1. 附 `RUST_LOG` / `RUI_IME_DIAG` 日志片段  
2. 注明是否 `RUI_FORCE_SOFTWARE`  
3. 开 issue，标签 `smoke` / `ime` / `a11y`
