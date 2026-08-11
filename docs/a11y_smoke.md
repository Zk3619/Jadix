# AccessKit / 读屏冒烟（WP-1.3）

与 [desktop_smoke.md](desktop_smoke.md) **#9**、[`a11y::bridge`](../crates/rui/src/a11y/bridge.rs) 对齐。至少在一个平台（推荐 Windows Narrator）签署。

**WP-1.3 边界**：本冒烟验收读屏主路径（角色/名称/值/Button 激活）。桥侧空串与 `template_id` path 消歧只保证**树不崩**；组件/宏仍写空身份属身份债，不在本 WP 关闭范围内——见 [`element_identity.md`](element_identity.md)。

## 主路径角色

| 控件 | AccessKit Role | 名称来源 | Actions |
|------|----------------|----------|---------|
| Input / Textarea | `TextInput` | `accesskit_name` → `aria-label` → placeholder；或 `aria-labelledby` | Focus；value 同步 |
| Button | `Button` | 显式名 → **后代可见文案**（caption 子节点） | Click、Focus |
| Combobox 触发器 | `ComboBox` | placeholder / `accesskit_name` | Focus（兼 TextInput 值） |
| Combobox 选项列表 | `ListBox` + `ListBoxOption` | 选项文案 | Click |
| Menubar | `MenuBar` / `Menu` / `MenuItem` | 文案 | Click、Focus |
| ContextMenu 面板 | `Menu` / `MenuItem` | 文案 | Click |
| **Label（不可 Tab）** | `Label` | 可见文本进 AccessKit **`value`**（非 `label` 属性） | 无 Focus；进树供浏览 / `labelledby` |

库测：`cargo test -p rui --lib a11y::`。

## 不可 Tab 的控件（Label 等）如何进无障碍

**Tab 只遍历可聚焦控件**（Button / Input…）——这是预期行为，不是 Label「没进树」。

桥结构：`Window` → **`Document`** → 控件与 Label。静态文案在 Document 下为 `Role::Label`（文本在 AccessKit **`value`**）。

### Windows Narrator：如何听到 Label

| 操作 | 听到什么 | 说明 |
|------|----------|------|
| **Tab / Shift+Tab** | 仅可聚焦控件 | 预期行为 |
| **鼠标指针朗读**（设置里「鼠标指针朗读」/ 讲述人鼠标触发） | Label / 按钮文案 | **优先用这个验收静态文案**；能读 = 树与 Name 已通 |
| **Caps Lock + →** | 项导航（依赖扫描模式） | 见下方「Caps 无效时」 |

勿为读屏把静态 Label 设成 `focusable`。关联控件：

```rust
ui! {
    // 显式 labelledby
    <Label accesskit_id="email-lbl">"邮箱"</Label>
    <Input id="email" aria_labelledby="email-lbl" placeholder="you@example.com" />

    // Solid `for`：Label 自动写 `accesskit_id=lbl:for:…`，桥侧给目标控件补 `labelled_by`
    <Checkbox id="terms" />
    <Label for="terms">"Accept terms"</Label>

    // Checkbox 内嵌文案 / Label（自动 `for` + `aria-labelledby`）
    <Checkbox>"Accept terms"</Checkbox>
    <Checkbox>
        <Label>"Accept terms"</Label>
    </Checkbox>
}
```

#### Caps Lock + 方向键无效时（常见，不一定是框架 bug）

1. **先开扫描模式**：`Caps Lock + Space`（或讲述人键 + Space）。未开扫描时 Caps+方向键常无响应。  
2. **确认讲述人键**：设置 → 辅助功能 → 讲述人 → 讲述人键可能是 **Caps Lock** 或 **Insert**；若是 Insert，应使用 `Insert + →`。  
3. 焦点须在 Gallery 窗口内；可先 Tab 到任一按钮再试项导航。  
4. 鼠标指针能读到 Label、但 Caps 仍无效 → **仍算静态文案已暴露**（AccessKit 命中测试路径正常）；Caps 项导航受讲述人模式/OS 限制，不单独阻塞 WP-1.3。

## Windows Narrator 复现（smoke #9）

1. 构建并启动：`cargo run -p gallery`
2. 打开 Narrator：`Win + Ctrl + Enter`（或设置 → 辅助功能 → 讲述人）
3. 在 Gallery **「2) Input」** 卡片中，用 **Tab** 聚焦 `gal-s2-input-a`（accessible name：`Gallery demo name`；placeholder 亦可作后备名）
4. **期望**：讲述人读出控件为编辑框/文本输入，并读出名称 **Gallery demo name**（或等价中文表述）
5. 输入若干 ASCII（如 `hello`）
6. **期望**：讲述人读出或可通过检查器看到 **值** 更新为 `hello`
7. Tab 到任意 **Button**（如 Toast）
8. **期望**：读出按钮角色与可见文案；`Enter`/`Space` 可激活（Click）
9. **静态 Label（推荐）**：开启「鼠标指针朗读」，悬停卡片标题等 Label → 应读出文案  
10. **可选**：`Caps Lock + Space` 开扫描后再 `Caps Lock + →`；失败但步骤 9 通过则记「Caps 项导航未验证」，不挡签署

可选：打开 Combobox，确认弹出列表为列表框/选项；Menubar 项为菜单项。

## 签署

| 平台 | 日期 | 机器 / OS | 读屏 | 结果 |
|------|------|-----------|------|------|
| Win | ☑ 2026-08-10 | Kingdom / Win 10.0.26200 | Narrator | Input 名/值；Button 文案；Label 鼠标指针朗读通过；Caps 项导航未验证（扫描模式） |
| macOS | ☐ | | VoiceOver | |
| Linux | ☐ | | Orca | |

失败时：附 OS 版本、是否 `RUI_FORCE_SOFTWARE`、Gallery 卡片名，开 issue 标签 `a11y` / `smoke`。
