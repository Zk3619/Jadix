# AccessKit / 读屏冒烟（WP-1.3）

与 [desktop_smoke.md](desktop_smoke.md) **#9**、[`accesskit_bridge`](../crates/rui/src/accesskit_bridge.rs) 对齐。至少在一个平台（推荐 Windows Narrator）签署。

## 主路径角色

| 控件 | AccessKit Role | 名称来源 | Actions |
|------|----------------|----------|---------|
| Input / Textarea | `TextInput` | `accesskit_name` → `aria-label` → placeholder | Focus；value 同步 |
| Button | `Button` | 按钮文案 | Click、Focus |
| Combobox 触发器 | `ComboBox` | placeholder / `accesskit_name` | Focus（兼 TextInput 值） |
| Combobox 选项列表 | `ListBox` + `ListBoxOption` | 选项文案 | Click |
| Menubar | `MenuBar` / `Menu` / `MenuItem` | 文案 | Click、Focus |
| ContextMenu 面板 | `Menu` / `MenuItem` | 文案 | Click |

库测：`cargo test -p rui --lib accesskit_bridge`。

## Windows Narrator 复现（smoke #9）

1. 构建并启动：`cargo run -p gallery`
2. 打开 Narrator：`Win + Ctrl + Enter`（或设置 → 辅助功能 → 讲述人）
3. 在 Gallery **「2) Input」** 卡片中，用 **Tab** 聚焦 `gal-s2-input-a`（accessible name：`Gallery demo name`；placeholder 亦可作后备名）
4. **期望**：讲述人读出控件为编辑框/文本输入，并读出名称 **Gallery demo name**（或等价中文表述）
5. 输入若干 ASCII（如 `hello`）
6. **期望**：讲述人读出或可通过检查器看到 **值** 更新为 `hello`
7. Tab 到任意 **Button**（如 Toast）
8. **期望**：读出按钮角色与可见文案；`Enter`/`Space` 可激活（Click）

可选：打开 Combobox，确认弹出列表为列表框/选项；Menubar 项为菜单项。

## 签署

| 平台 | 日期 | 机器 / OS | 读屏 | 结果 |
|------|------|-----------|------|------|
| Win | ☐ | | Narrator | |
| macOS | ☐ | | VoiceOver | |
| Linux | ☐ | | Orca | |

失败时：附 OS 版本、是否 `RUI_FORCE_SOFTWARE`、Gallery 卡片名，开 issue 标签 `a11y` / `smoke`。
