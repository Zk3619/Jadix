# 稳定面 variant / size / token / 受控 API（WP-3.8）

> **从属**：[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §3（双书写）· §4.3（外观）· §4.2.5（状态）。  
> **范围**：仅 [`component_inventory.md`](component_inventory.md) **verified** 控件。experimental（Toggle / ToggleGroup / ColorPicker…）不进本冻结。  
> **日期**：2026-08-17；**2026-08-31** §0 `Into` 双收（字符串产品面 + 可选枚举）；**2026-08-31** 明确为 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) **§3.1.1** 在 `variant` / `size` 轴上的冻结实例

未知 `variant` / `size` **回退 `default`**（可测）。禁止在 verified 控件里发明第二套 Button 同名、不同语义的词汇而不登记本表。

---

## 0. Builder：`Into` 双收（字符串 + 枚举）

> **上位规范**：链式优先、`Into` 双收、何时加枚举见 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) **§3.1.1**。本节仅约束 **`variant` / `size` 外观轴** 的冻结词汇与 parse 回退；其它离散属性（如 `pointer_events`、`cursor`）适用同一 §3.1.1 总则，但不进本表。

产品面 **默认仍是字符串**（对齐 ShadCN / `ui!` 属性 / 热重载模板）。对内用枚举（`ButtonVariant`、`BubbleVariant`…）做真源；Builder **不得**只收枚举、废除 `&str`。

| 面 | 约定 | 禁止 |
|----|------|------|
| **RSX** | `variant="outline"` / `variant={sig.get()}`（`String` 或可解析值） | 强迫用户在标签里写 `ButtonVariant::Outline` |
| **Builder 产品示例** | `.variant("outline")`（文档 / Gallery 默认） | 文档暗示「现代写法 = 只能传枚举」 |
| **Builder 类型增强** | 参数类型为 `impl Into<XxxVariant>`（或等价：`From<&str>` + `From<XxxVariant>`），使 `"outline"` **与** `ButtonVariant::Outline` **都能传** | 为对齐 Freya 删掉字符串重载；第二套平行 API（`.variant_str` / `.variant_enum`） |
| **内部** | 枚举 + `FromStr` / `parse`；未知 → `default`（§1） | 组件内再发明一套未登记字符串别名 |

推荐签名形态（新控件 / 改造时优先）：

```rust
// 对内真源
pub enum ButtonVariant { /* … */ }
impl From<&str> for ButtonVariant { /* parse；未知 → Default */ }
impl From<ButtonVariant> for ButtonVariant {
    fn from(v: Self) -> Self { v }
}

impl ButtonBuilder {
    pub fn variant(mut self, v: impl Into<ButtonVariant>) -> Self {
        self.variant = v.into();
        self
    }
}
```

合法调用（同一方法）：

```rust
button("save").variant("outline");
button("save").variant(ButtonVariant::Outline);
```

**与 Freya 的边界**：Freya 主路径是纯枚举链式 API（无 `variant="…"` RSX 面）。RUI **学其 `Into` 双收思路作 Builder 增强**，**不**把「只能写 `Enum::Variant`」当成产品默认。对照见 [`FREYA_COMPARISON.md`](FREYA_COMPARISON.md) §6 / §10。

**现状**：多数 Builder 仍是 `fn variant(self, v: &str)` + 内部 `parse`（等价于「只开了字符串半边」）。`Into` 双收是 **允许且鼓励的增强**，不要求一次全仓改完；新增 / 触碰变体 API 时优先改成 `impl Into<…>`。experimental 控件（如 Bubble）同样适用本条与 **§3.1.1**。

**其它离散属性（示意，非本表冻结）**：`pointer_events`、`cursor` 等同样 **默认链式字符串**（如 `.pointer_events("none")`），并可按需增加 Builder 预设枚举（variant 名避开 `None`）；开放域尺寸仍走 `IntoCssDim`，见 [`box_model.md`](box_model.md)。

---

## 1. 公共词汇（Button 标杆）

| 轴 | 合法值 | 未知 |
|----|--------|------|
| **variant** | `default` · `secondary` · `outline` · `ghost` · `success` · `warning`（=`warn`）· `destructive`（=`error`）· `link` | → `default` |
| **size** | `xs` · `sm` · `default` · `lg` · `icon` · `icon-xs` · `icon-sm` · `icon-lg` | → `default` |

- `outline` = **border**（盒模型），不是 CSS `outline`；焦点环用 theme `ring`（[`color.md`](color.md)）。  
- `success` / `warning` / `destructive`：soft tint（浅 `accent/10→/20`，暗 `/20→/30`）；success=`green-600`、warning=`orange-600`（与 Toast tone 同源），destructive 用 theme token。`warn`≡`warning`，`error`≡`destructive`。  
- Gallery「基础」已展示 variant 行 + size/icon 矩阵。  
- 库测：`unknown_variant_and_size_fall_back_to_default`；`success_and_warning_soft_tint_match_toast_accents`；`apply_bound_unknown_button_variant_falls_back_to_default`。

---

## 2. 显式差异（不是 Button 词汇）

| 组件 | API | 合法值 | 说明 |
|------|-----|--------|------|
| **Scrollbar** | `variant` | `capsule`（默认）· `rect`/`rectangle` | 轨道几何；未知 → capsule |
| **Toast**（条目） | `variant` / `tone` | `default` · `destructive` · `success` · `warning` · `error` | 语气；未知 → default |
| **Toaster**（宿主） | `variant` | `stack`（默认）· `list` | 布局，不是条目语气 |
| **Label** | `size` / `text_size` | `IntoF32`（字号） | **不是** Button size 阶梯 |
| Button 以外 verified | — | 无产品 `variant`/`size` | Input 高固定 `2.25rem`（对标 default 钮） |

**禁区**：Scrollbar/Toast 的 `variant` 不得解析成 Button 的 `outline`/`ghost`。

experimental 备注（不挡本包）：Toggle `default`/`outline` + `sm`/`default`/`lg`；ColorPicker 另有 `inline`/`sketch`/… 族。

---

## 3. Token 真源

| 规则 | 要求 |
|------|------|
| chrome 色 | `parse_color("primary")` / [`tokens::*`](../crates/rui/src/tokens/colors.rs) / Theme OKLCH；**禁止**组件内魔法 hex / 裸 `[r,g,b]` 当品牌色 |
| 焦点环 | [`focus_ring_color`](../crates/rui/src/style/focus_ring.rs) = theme `ring` |
| Toast success / warning | `chart-2` / `chart-4` + 语义前景（随 dark 换肤） |
| 海拔阴影 | 可用 token 名（`foreground` / `destructive` / `chart-*`）；勿写死 `#16a34a` |

**换肤冒烟（建议，不替代库测）**：Gallery → 切 dark → 抽查 Button / Input / Dialog：主色、边框、焦点环仍来自 Theme，无残浅色魔法绿/琥珀。

---

## 4. 受控 API 命名

单一写入；调用方 `Signal` 在则**不得**再为同一语义建内部 store。绑定更新走 `apply_bound_*` 或 paint sync（Input：`bind_controlled_value` + `pull_controlled_into_store`）。

| 语义 | Builder | RSX | verified |
|------|---------|-----|----------|
| 文本/选项值 | `value_signal` | `value` / `valueSignal` | Input · Textarea · Select · Combobox · RadioGroup · Slider · Tabs · Progress |
| 浮层开合 | `open` / `open_signal` | `open` / `openSignal` | Dialog · Popover · Tooltip · HoverCard · DropdownMenu · Sheet |
| 浮层间距 | `side_offset` | `sideOffset` | Popover · HoverCard ·（Tooltip 用 `side`） |
| 布尔开合（控件） | `checked_signal` | `checked` | Checkbox · Switch |
| 浮层开合 | `open_signal` | `open` | Dialog · Tooltip |
| 按钮变体绑定 | `apply_bound_button_variant` | `variant={sig.get()}` | Button |

**别名（不是第二套状态）**：RadioGroup `selected_signal` ≡ `value_signal`；Tabs 另有 `selected_signal(usize)` 与字符串 `value_signal` 并存（文档化，勿再加第三套）。

库测：`apply_bound_button_variant_updates_chrome_in_place`；`input_value_signal_seeds_store`；`external_set_pulls_into_store_when_unfocused`。

---

## 5. 双书写面审计（verified）

稳定面凡 RSX 暴露的行为 props，Builder 须有链式方法（规范 §3.1）。RSX camelCase 别名（`onClick` / `valueSignal` / `defaultValue`）**仅糖**；Builder 只用 snake_case。

| 组件 | 双入口 | 受控 | 缺口 / 例外 |
|------|--------|------|-------------|
| Button | `Button` / `button` | variant bind | `hover_easing` 双面；未知 variant → default |
| Label | `Label` / `label` | — | `size` = 字号 |
| Separator | `Separator` / `separator` | — | |
| Card 复合 | `Card*` 标签 | — | 分区槽；无独立 variant 词汇 |
| Input | `Input` / `input` | `value` ↔ `value_signal` | `type` → `input_type`（宏糖）；`truncate` 为 no-op（文档已写） |
| Textarea | `Textarea` / `textarea` | `value` ↔ `value_signal` | |
| Checkbox / Switch | 标签 + fn | `checked` ↔ `checked_signal` | 布尔原语 [`toggle_state`](../crates/rui/src/utils/toggle_state.rs) |
| RadioGroup | 标签 + fn | `value_signal` | `selected_signal` 别名 |
| Slider | 标签 + fn | `value_signal` | |
| Select / Combobox | 标签 + fn | `value_signal` | 复合 Trigger/Content 与扁平 `options` 两写法 |
| Dialog | 复合标签 | `open` | Builder：`dialog` / `open_signal`；壳走 WP-3.4 |
| Tooltip | 标签 + fn | `open_signal` | |
| HoverCard | 复合标签 + fn | `open` ↔ `open_signal` | 悬停；`side`+`align`+`sideOffset` |
| Toast / Toaster | 标签 + `toast()` / `toaster()` | 队列全局 | **命令式** `toast("…").show()` 是产品面，不是「仅 RSX」；条目 `variant` ≠ 宿主 `variant` |
| Tabs | 复合标签 + fn | `value_signal` | |
| ScrollArea / Scrollbar | 标签 + fn | — | Scrollbar `variant` 见 §2 |

**互嵌**（规范 §3 示例）：`crates/rui/tests/prelude_dual_surface.rs`（Button + Input 受控双写法）。

标杆片段：

```rust
use rui::prelude::*;

ui! { <Button variant="outline">"保存"</Button> }
let _: Element = button("save").variant("outline").text("保存").into();
// Builder 增强后亦可（§0）：.variant(ButtonVariant::Outline)

let v = use_signal("name", || String::new());
ui! { <Input id="name" value={v} /> }
let _: Element = input("name").value_signal(v).into();
```
