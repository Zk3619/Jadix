# 真窗口冒烟清单（阶段七）

三平台（Windows / macOS / Linux）手工或半自动冒烟。CI 仍以 `RUI_FORCE_SOFTWARE=1` 离屏为主；本表覆盖**真窗口**路径。

## 前置

- 本地：`cargo run -p rui --example <name>`（以仓库 `examples/` 实际名称为准）
- 记录：OS 版本、GPU/驱动、是否 IME（中/日/韩）

## 清单

| # | 步骤 | 期望 | Win | macOS | Linux |
|---|------|------|-----|-------|-------|
| 1 | 启动主示例窗口 | 窗口可见、首帧非黑屏 | ☐ | ☐ | ☐ |
| 2 | 点击 Input，输入 ASCII | 文本出现、光标移动 | ☐ | ☐ | ☐ |
| 3 | 切换系统 IME，输入中文/日文 | 预编辑可见；上屏不丢字 | ☐ | ☐ | ☐ |
| 4 | 滚动含 Input 的 ScrollArea | IME 候选窗仍贴近光标 | ☐ | ☐ | ☐ |
| 5 | Tab / Shift+Tab 遍历可聚焦控件 | 焦点环可见；顺序合理 | ☐ | ☐ | ☐ |
| 6 | Combobox：方向键 + Enter | 选项可选中并关闭 | ☐ | ☐ | ☐ |
| 7 | Escape 关闭最上层浮层 | 仅关闭顶层 | ☐ | ☐ | ☐ |
| 8 | 打开次窗并关闭 | 主窗仍存活；无 panic | ☐ | ☐ | ☐ |
| 9 | 读屏（可选）：聚焦 Input | 读出名称/值 | ☐ | ☐ | ☐ |

## 失败时

1. 附 `RUST_LOG=rui=debug,warn` 日志片段  
2. 注明是否 `RUI_FORCE_SOFTWARE`  
3. 开 issue，标签 `smoke` / `ime` / `a11y`
