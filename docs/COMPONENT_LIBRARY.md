# RUI 基础组件库（权威规范）

> **文档地位**：组件产品面的**领域规范**（选型、分层、API 契约、清单口径）。  
> **从属关系**：服从 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) 六条门槛与禁区；**执行排期 / CV 门禁**见 [`AUTHORITY.md`](AUTHORITY.md)。冻结细则 [`COMPONENT_LIBRARY_PLAN.md`](archive/COMPONENT_LIBRARY_PLAN.md)。  
> **事实来源**：`crates/rui/src/{primitives,tokens,components,style,utils}`（私有壳在 `components/internal/`）、`examples/gallery`、[`headless.md`](components/headless.md)、[`theme.md`](engine/theme.md)、[`color.md`](engine/color.md)、[`drawing.md`](drawing.md)。  
> **日期**：2026-08-12 起分域规范；**2026-09-05** Gallery 产品面走查 + 文档目录收口。中间修订见文内 WP 标注。

交叉文档：[`component_inventory.md`](components/component_inventory.md) · [`component_variants.md`](components/component_variants.md) · [`element_identity.md`](element_identity.md) · [`custom_components.md`](components/custom_components.md) · [`custom_components_suggestions.md`](components/custom_components_suggestions.md) · [`COMPONENT_LAYERING.md`](components/COMPONENT_LAYERING.md) · [`keyboard_nav.md`](ops/keyboard_nav.md) · [`desktop_smoke.md`](ops/desktop_smoke.md) · [`REACTIVITY.md`](REACTIVITY.md) · [`reactivity_and_hot_reload.md`](ops/reactivity_and_hot_reload.md) · [`theme.md`](engine/theme.md) · [`color.md`](engine/color.md) · [`a11y_smoke.md`](ops/a11y_smoke.md) · [`drawing.md`](drawing.md)

---

## 0. 一句话结论

**对外交付 ShadCN 式带默认样式的组件；对内以 Radix 式无头行为为骨架；书写上 `ui!` 标签与 Builder 纯 Rust 互为兼容的一等公民。**  
禁止二选一：纯无头拖死采用与质感验证；样式与行为粘死拖死 a11y、换肤与可测性；只保留标签或只保留 Builder 都会破坏 DX。

| 面 | 形态 | 路径 | 用户默认 |
|----|------|------|----------|
| 产品面 | shadcn：`variant` / `size` / token 色 / 焦点环 | `components/` | **是**（prelude / Gallery / 文档索引） |
| 书写面 | A: `ui!` 标签 · B: `*Builder` 链式 | 宏展开 ≡ Builder → `Element` | **双轨兼容**（见 §3） |
| 行为面 | radix：开合、roving、trap、受控 Signal、布尔态 | `primitives/` + `utils/{popup,roving,focus_trap,keyboard,toggle_state}` | 高级 / 自定义组件 |
| 语义面 | 设计令牌 + Theme light/dark | `style/theme.rs` + `tokens/` + `rui_style_tokens` | 换肤与 chrome 同源 |

ADR 对齐：纲领 **ADR-6**（ShadCN 无头分层）、**ADR-9**（双书写面兼容）— **接受，不可再分叉**。

---

## 1. 工程级定义（组件库专属）

组件库达到「工程可承诺」当且仅当下列同时成立（映射纲领六门槛）：

1. **可预测**：有状态控件的 Signal 键 / `focus_id` 规则文档化；受控与非受控无双源真理。  
2. **原生可信**：主路径控件支持 Tab / roving / Escape / 焦点环；浮层走统一 Esc 栈与 placement sync。  
3. **测宽≡绘制**：含文本的控件（Input / Textarea / Label / VirtualList 行）不依赖「猜行高」。  
4. **a11y**：Button / Input / 至少一种列表或菜单 Role 正确；焦点与值变化可进 AccessKit。  
5. **质量门**：每公开组件有 Gallery 页；核心 ≥5 场景 golden；浮层 / 表单有库测或冒烟条目。  
6. **发布契约**：稳定面 vs 实验面表格；破坏性变更进 CHANGELOG；文档站有组件索引。

未满足时，组件可演示（MVP），但不得标为稳定 API。

---

## 2. 分层架构（实现 = 规范）

```text
  应用 / Gallery / 自定义 #[component]
            │  默认 import
            ▼
  components/          ← 外观 + 变体 + 产品 API（ShadCN）
            │  组合
     ┌──────┼──────────────────┐
     ▼      ▼                  ▼
 primitives/  tokens/     utils/popup·roving·focus_trap·keyboard·text_field·toggle_state
 (布局叶子)   (语义色)     (浮层几何 / 键盘 / 文本编辑 / 布尔态)
     │
     ▼
 style/theme + color_space(OKLCH) + utility 变体 + focus_ring

  私有（非产品面）：components/internal/{text_edit,menu}（Dialog 壳在 `dialog.rs`）
```

### 2.1 职责与禁区

| 层级 | 路径 | 职责 | **禁止** |
|------|------|------|----------|
| 布局 / 交互原语 | `primitives/` | `Box`/`HStack`/`VStack`/`GStack`、`Text`/`Icon`、`interactive`、`overlay`、`path`、`Portal`；**W3：`svg` / `canvas` / `scene3d` 叶**（[`drawing.md`](drawing.md)） | 写死 primary 色、圆角、阴影等视觉常量；第二套 path 格式绕开 `primitives/path` |
| 行为工具 | `utils/popup.rs`、**`presence.rs`**、**`motion.rs`**、**`style/semantic_tone.rs`**（计划 · 语义色 parse + soft chrome · 见 [`component_variants.md`](components/component_variants.md) §6）、**`keyed_overlay.rs`**、**`layout_group.rs`**、`roving.rs`、`focus_trap.rs`、`keyboard.rs`、`text_field.rs`、**`toggle_state.rs`**、**`controlled.rs`**、**`activate_registry.rs`**、**`list_nav.rs`**、**`pointer_drag.rs`**、**`drag/`**（WP-DRAG.1 · 超越 Sortable；solid-dnd/interact；不进 prelude）、**`table_sort.rs`**、**`tree_model.rs`**、**`element_pack.rs`**（`take_packed` / `take_fragment`：展开 `rui_fragment` / 匿名多子壳，保留 `data-slot`） | 浮层标记与 placement、退场门、CSS transition 目标/sync、portal 同 key 调和、**rank→位移**、roving、Tab 陷阱、键路由、受控文本写入、布尔/泛化受控、激活注册表、列表键、拖拽会话、**拖放套件**、表排序、树扁平、**children 拆包** | 绑定某一组件的外观变体 |
| 设计令牌 | `style/theme.rs`、`theme_override.rs`、`color_space.rs`、`tokens/`、`rui_style_tokens` | OKLCH 语义色→sRGB、radius、utility 真源、dark、应用 patch | 业务状态机；完整 ICC/HDR（见 [`color_advanced.md`](engine/color_advanced.md)）；主题契约见 [`theme.md`](engine/theme.md) |
| 样式组件 | `components/` | **仅产品组件**（Button、Input、Dialog…）；厚产品策略可拆子模块（如 `toast/`、`carousel/`） | 把复杂开合/焦点状态机只藏在样式层且不可测；把私有 chrome 导出为产品 API |
| 私有壳 | `components/internal/`（`pub(crate)`） | Input/Textarea chrome（`text_edit`）；Dropdown/Context 共享菜单能力（`menu/{checkable,item_chrome,subtree}`）；随组件库分 crate | 当作公开 API 文档化或 prelude 导出；挂到框架 crate 根冒充 runtime；**单组件策略勿塞进此处**；**Dialog 壳在 `dialog.rs`，非 internal** |

现状缺口（须按计划下沉，见实施计划 C-WP；**体量与 gpui-component 对照**见 [`COMPONENT_LAYERING.md`](components/COMPONENT_LAYERING.md)）：

- **折叠面板 motion**：`collapsible` / `accordion` 各有一套 `apply_*_panel_motion`（~350 行重复）——**P0** 合并为 `utils/collapse_panel.rs`（见 [COMPONENT_LAYERING.md](components/COMPONENT_LAYERING.md) §5 P0）。
- **语义色 variant**：`error`/`warn` parse 与 soft tint 在 Alert/Button/Badge/Toast 重复 —— 抽 `style/semantic_tone.rs`（见 [component_variants.md](components/component_variants.md) §6）。
- **单文件体量**：与 gpui-component 未拆分组件逐项对照见 [COMPONENT_LAYERING.md](components/COMPONENT_LAYERING.md) §1.6 / §6（combobox、hover_card、kbd、label 等）。
- **浮层**：几何与挂载已收口：`utils/popup` 的 `mark_floating` + **`mount_anchored_overlay`** / **`mount_viewport_portal_stage`**；Esc 栈 `portal_overlay`。开合状态仍可内嵌各组件——禁止第二套 placement 或嵌套 `rui_portal`。
- **Toggle 布尔态**：[`utils/toggle_state`](../crates/rui/src/utils/toggle_state.rs)（WP-3.7 ☑）；Toggle / Checkbox / Switch 只消费状态 + token。外观仍在 `components/`。  
- **Tabs / Toggle 外观**：`primitives/mod.rs` 已指向 `components/`；布尔原语不在 primitives。  
- **作者面**：`#[component]` **默认** [`component_root`](../crates/rui/src/core/reactivity.rs)（WP-3.14 ☑）；纯 Builder 复合根仍须显式调用。`untrack` 仍会跳过 `reactive_anchor`——用 `isolate_reads`（§2.3）。

### 2.2 与 Web 对标的正确映射

| Web | RUI | 说明 |
|-----|-----|------|
| Radix Primitives | `interactive` + popup/roving 契约 + 未来抽出的 headless builders | 无 DOM；行为用 Signal + focus_id |
| shadcn/ui | `components/*` + Theme tokens + `variant`/`size` | **源码在库内**，非「复制进用户仓库」；定制靠 token / class / 组合原语 |
| Tailwind / Uno | `class`/`tw` + `rui_style_tokens` + attributify 属性 | 不全量 CSSOM |
| `<svg>` / `<canvas>` | RUI `<svg>` / `<canvas>` 叶 + 2D·3D（[`drawing.md`](drawing.md)） | **无 DOM**；不搬 HTML-in-canvas；完整 SVG/Canvas2D 规范不在范围 |
| canvas-ui 式特效 | 未来 **B16**（包住子树的 shader/paint） | 不改默认 shadcn 产品面；不挡 W3 |
| react-bits 式动效 | 未来 **B17**（文字 / 背景 / 动画 UI 目录） | 不替代稳定面控件；不搬 CSS `@keyframes` 运行时；见 [`drawing.md`](drawing.md) §6.2 |

**不采用**：npm-shadcn「复制粘贴组件文件」为默认 DX。Rust crate 的正确模型是**库内分层 + 稳定公开 API + token 覆盖**；用户要完全异形皮肤时，用 `#[component]` 组合原语（见 [`custom_components.md`](components/custom_components.md)）。

### 2.3 复合控件作者面（W3 实证 · 2026-08-17）

WP-3.2（Chart）与 WP-3.3（Calendar / Date·Time·Color / Command）落地后的事实：

**自定义 / 复合组件「改一处炸一片」的心智负担，主要来自框架默认值与组合原语不完整，而不是选器领域逻辑本身。**  
日历算术、HSV、ISO 字符串是普通产品复杂度；作者真正反复踩的是：**实例 ≠ Owner、跟踪原语静默破坏锚点、浮层每控件自研、身份四轴仍漏进每个 experimental 控件。**

| 负担来源 | 性质 | W3 证据 | 关闭包 |
|----------|------|---------|--------|
| `#[component]` 默认实例 Owner；Builder 根须 [`component_root`](../crates/rui/src/core/reactivity.rs) | **3.14.1 ☑**；Builder 省略仍串 Context | TimePicker 三 Combobox 须 Builder 根 `component_root`（库内已手写）；`#[component]` 不再要求 | **WP-3.14 ☑** |
| `untrack` vs `isolate_reads` | **语义纪律**（可测） | 浮层体用 `untrack` → 空壳；须 `isolate_reads`。测：`untrack_skips_anchor_but_isolate_reads_registers` | **WP-3.14.2 ☑** |
| 开合 / portal / 滚动锚 / 叠层 | **3.4.1–3.4.3 ☑** | 锚定走 `mount_anchored_overlay`；模态走视口舞台；Esc 只关顶层 | **WP-3.4 ☑** |
| 实例信号 / 必填 `id` | **身份债未迁完** | Chart 未传 `selected_signal` 曾共用 `chart_pin`；Calendar 等仍「必填实例键」 | WP-2.8 已开收敛；余下随 3.14 cookbook + 3.8 |
| Toggle / Checkbox / Switch 布尔态 | **3.7 ☑** 共享 [`toggle_state`](../crates/rui/src/utils/toggle_state.rs) | 受控 XOR Owner 作用域；disabled 不写入 | **WP-3.7 ☑** |
| variant / size / token / 受控 API 面 / 双书写 | **3.8 ☑** 词汇表 + 审计 | 仅 verified；未知 variant → default | **WP-3.8 ☑** |
| Form helpers 校验路径 | **3.9 ☑** 错误文案 ≡ Input invalid 环 | `form_provider` + `form_field`；点击回调 clone [`FormState`](../crates/rui/src/components/form.rs)，勿在事件里 `use_form()` | **WP-3.9 ☑**（仍 `experimental`） |
| 图表 grammar、命中、tooltip 几何 | **产品复杂度**（部分实现 Bug） | path 贴图翻转、径向命中、选中描边——修在 Chart，不升格为「换渲染器」 | 已随 3.2；禁区见 [`drawing.md`](drawing.md) |
| 键盘映射、ISO 格式 | **本包产品工作** | WP-3.3 已签 | ☑ |

**因此**：`#[component]` 已是实例域；再扩 Form / planned 复合控件时，Builder 根走 `component_root`，浮层走 §4.4 helper。禁止把下一次心智爆炸解释成「作者没读懂 Solid」。

**禁止（作者面）**

| 禁止 | 原因 |
|------|------|
| 新复合控件再手抄一套 portal / Esc / 0×0 锚配方 | 第二套浮层；**WP-3.4 ☑** 后视为回归 |
| 用 `untrack` 包住整棵要注册 anchor 的子树 | 空浮层；用 `isolate_reads` |
| 省略实例 Owner 却 `provide_context` | 兄弟实例串状态；`#[component]` 已默认包；**Builder 根**仍须 `component_root` |
| 用全局 `use_signal("chart_pin")` 充当每图状态 | 多实例撞键 |
| 文档继续只示范无状态 `<Button>` | 与 W3 实证不符；手册必须含反例 |

分层仍以 §2.1 为准：外观在 `components/`，几何在 `utils/popup`，执行域在 Owner。作者面 **WP-3.14 ☑**；浮层单一路径 **WP-3.4 ☑**。

---

## 3. 双书写面（一等公民 · 必须兼容）

RUI **同时**提供两种写 UI 的方式，且必须**互相兼容**。禁止把「只用 `ui!` 标签」或「废弃 Builder」写成唯一正确路径（**ADR-9**）。

| 书写面 | 形态 | 典型入口 | 适用 |
|--------|------|----------|------|
| **A · RSX / 标签** | `ui! { <Button variant="outline">…</Button> }` | `rui_macros::ui!`、`#[component]`；经 [`prelude`](../crates/rui/src/prelude.rs) | 声明式树、热重载模板、文档站默认示例 |
| **B · Builder / 纯 Rust** | `button("ok").variant("outline").on_click(…).into()` 或 `Element::from(…)` | 同模块 `fn` + `*Builder`；经 **同一** `prelude` | 无类 HTML 标签、动态拼装、库测、复杂闭包 |

### 3.0 默认入口：`rui::prelude`

```rust
use rui::prelude::*; // 标签入口 + Builder/`fn` + 布局原语 + Element/Signal + ui!/component
```

| 进 prelude | 不进 prelude |
|------------|--------------|
| 样式组件 PascalCase **与** 对应 `snake_case` / `*Builder` | [`utils::popup`](../crates/rui/src/utils/popup.rs) 低层几何 / placement |
| `hstack` / `vstack` / `Portal` / `text` / `icon` | `popover_panel`（低层面板；复合面用 `popover` / `Popover*`） |
| `ThemeMode` / `set_theme_mode` | `overlay` / `path`、`BaseComponentBuilder`、`apply_bound_*` |
| `ui!` / `component` / `reactive!` 等宏 | `planned` 未落地组件；`internal/*` |

策略结论：**默认 DX = prelude 双书写面**；高级浮层能力显式路径导入，避免把 Radix 式无头工具误当成产品默认 API。

### 3.1 兼容契约（硬性）

1. **同一语义、同一 `Element`**：标签展开为 Builder 调用；Builder `Into`/`From` → `Element`。两种写法不得产生两套行为或两套 chrome。  
2. **可互嵌**：
   - `ui!` 子树可嵌入 `{ Element::from(button(…).variant(…)) }` 或任意已构建的 `Element`；
   - Builder / `vstack`·`hstack` 的 `.child(…)` 可接收 `ui! { … }` 的结果或 `#[component]` 产物。  
3. **API 面等价**：稳定面组件凡在 RSX 暴露的 props（`variant` / `size` / `disabled` / 受控 signal / `on_click` 等），Builder 须有对应链式方法（或文档标明「仅 RSX 糖」的极少数例外，且例外不得承载唯一行为路径）。  
4. **仅 Builder 的控件**（如部分 `toast` / `dropdown`）：允许暂无标签，但须在文档与 Gallery 标明；新增稳定面控件**默认双入口**（标签 + Builder），并进入 prelude。  
5. **热重载**：`template_id` 主要服务含 `ui!` 的模板；纯 Builder 树仍是一等运行时能力，不因无热补丁而降级为「非推荐」。  
6. **文档与示例**：Getting Started / 组件索引应**至少各给一种写法的完整片段**；标杆组件（Button、Input）两种都展示。  
7. **RSX camelCase 别名**（`rui_macros`，对位 ShadCN）：`onClick` / `defaultValue` / `captionLayout` / `valueSignal` / `showAlpha` / `sideOffset` / `openSignal` 等映射到 snake_case props；Builder **只**用 snake_case。  
8. **Builder 属性值**：**链式优先**；具预设值或固定运行时语义的属性允许 **字符串 + 可选枚举** 同一方法双收；开放域尺寸等走 `IntoCssDim` / `IntoF32`，不滥造枚举。总则见 **§3.1.1**；`variant` / `size` 冻结表见 [`component_variants.md`](components/component_variants.md) **§0**。

### 3.1.1 Builder 属性值：链式优先 + `Into` 双收

本节是 **Element / `*Builder` / `BaseComponentBuilder` 等链式 setter** 的通用口径。[`component_variants.md`](components/component_variants.md) **§0** 是其在 `variant` / `size` 外观轴上的**冻结实例**；[`box_model.md`](engine/box_model.md) §1 的 `IntoCssDim` 是**开放域**反例。

#### 链式优先（默认写法）

凡能在 Builder 上设置的属性，**默认在 `.into()` 之前**用同名链式方法完成，保持一条 fluent 链到 `Element`。

| 推荐 | 不推荐（除非链上暂无法表达） |
|------|------------------------------|
| `hstack().w("100%").pointer_events("none").into()` | `let mut el = hstack().w("100%").into(); el = el.pointer_events_none();` |
| `button("save").variant("outline").disabled(true).into()` | 先 `into()` 再对 `Element` 逐字段二次赋值 |

`Element` 上的便捷方法（如 `pointer_events_none()`、`cursor_not_allowed()`）可保留作语法糖或存量兼容；**文档、Gallery、新代码默认展示链式** `.pointer_events(…)` / `.cursor(…)`，不在示例里暗示「现代写法 = 只能先 `into()`」。

RSX / `ui!` 属性与 Builder 链式方法**同一语义**（§3.1 第 3 条）；宏展开仍应落到公开 Builder，而非绕过链式另造私有 setter。

#### `Into` 双收（字符串 + 可选枚举）

产品面 **默认仍是字符串**（对齐 CSS / ShadCN / `ui!` / 热重载模板）。对内可用枚举做真源；Builder **不得**只收枚举、废除 `&str`。

| 面 | 约定 | 禁止 |
|----|------|------|
| **RSX** | `pointer_events="none"` / `variant="outline"`（字符串或可解析值） | 强迫用户在标签里写 `XxxEnum::Variant` |
| **Builder 产品示例** | `.pointer_events("none")`、`.variant("outline")`（文档 / Gallery **默认**） | 文档暗示「现代写法 = 只能传枚举」 |
| **Builder 类型增强** | 参数 `impl Into<…>`（或等价：`From<&str>` + `From<XxxPreset>`），使 `"none"` **与** 预设枚举 **同一方法** 都能传 | 为对齐 Freya 删掉字符串重载；第二套平行 API（`.xxx_str` / `.xxx_enum`） |
| **内部** | 枚举 + `FromStr` / `parse`；未知 → 文档化 default | 组件内再发明一套未登记字符串别名 |

合法调用（同一方法、同一链）：

```rust
hstack().w("100%").pointer_events("none").child(separator()).into();
// 类型增强落地后亦可（枚举名避开 Rust `None` 等歧义）：
// hstack().w("100%").pointer_events(Pe::Ignore).child(separator()).into();
```

#### 何时加枚举、何时不加

| 属性类 | 示例 | 约定 |
|--------|------|------|
| **离散预设**（有限合法值 + 固定命中/绘制/布局语义） | `variant` / `size` / `pointer_events` / `cursor` / Scrollbar `variant` | **允许**加专用预设枚举；字符串仍为产品默认与 RSX 面 |
| **开放域**（连续、任意 CSS 长度/颜色/ calc） | `w` / `h` / `p` / `font_size` / `gap` | 用 `IntoCssDim` / `IntoF32` / `IntoCssRectPair` 等 trait，**不**为每个合法字符串发明枚举 |
| **行为回调** | `on_click` / `on_hover` | 闭包 / `Handler`；不走字符串枚举 |
| **便捷语法糖** | `pointer_events_none()`、`cursor_text()` | 可保留；等价于链式 `.pointer_events("none")` 等，**非**第二产品面 |

**枚举仅针对具有预设值或固定运行时行为的属性**——即：合法值可有限列举，且每个值在框架内有明确、可测的语义（命中策略、chrome 分支、parse 回退等）。仅为「少打引号」而为开放字符串域加枚举，视为过度设计。

**命名**：Builder 预设枚举的 variant 应避开 Rust 关键字与标准库歧义（CSS `none` → 预设枚举用 `Ignore` 等，而非 `None`）。运行时解析真源（如 `PointerEvents`）与面向作者的 Builder 预设（如未来的 `Pe`）可分层；对内 `parse` / `as_str` 保持单一真理。

**现状**：多数 Builder 仍是 `fn xxx(self, v: &str)` + 内部 `parse`（等价于「只开了字符串半边」）。`Into` 双收与链式优先是 **允许且鼓励** 的增强，不要求一次全仓改完；新增 / 触碰相关 API 时优先遵守本节。

### 3.2 反例（禁止）

| 禁止 | 原因 |
|------|------|
| 新能力只进 `ui!` 属性、Builder 长期缺失 | 破坏无标签书写能力 |
| 文档暗示「现代写法 = 只能写标签」 | 与项目「掌握代码 / 纯 Rust 拼装」目标冲突 |
| RSX 与 Builder 走不同状态机或不同 token | 双源真理，无法互嵌保证观感一致 |
| prelude 只导出标签、废除 Builder re-export | 违反 ADR-9 |
| Builder 只收枚举、废除 `.variant("outline")` | 打断 ShadCN / `ui!` / 热重载字符串契约；见 **§3.1.1** · [`component_variants.md`](components/component_variants.md) §0 |
| 先 `.into()` 再改属性，而链上可一次写完 | 破坏 fluent 一致性；见 **§3.1.1** 链式优先 |
| 为 `w("50%")` 等开放域强造枚举 | 应用 `IntoCssDim`；见 **§3.1.1** · [`box_model.md`](engine/box_model.md) |
| 复合组件为每个槽发明 `*PartsClass` / 十余个 `*_class` 字段 | 维护 N 份平行结构体；与 §3.3「组合面调样式」冲突；数据糖例外见 §3.3.4 |
| 闭包装配为每个内部槽发明 `*_with` / `*SlotBuilder` / `*Parts` | 根 Builder 穿透；内部格子用 `.slot("data-slot", …)` 或零件自己的 `class`/`base` |
| `From<*Builder>` 代建第二棵 `class: None` 树、且无 `.child()` / `*_scope` | Tier 3 成为唯一路径；RSX 与 Builder 样式能力不对等 |

代码锚点（同一按钮 · 双面）：

```rust
use rui::prelude::*;

// A · 标签（无持久状态时可省略 id）
ui! { <Button variant="outline" on_click={move || { /* … */ }}>"保存"</Button> }
ui! { <Badge variant="secondary">"New"</Badge> }

// 多实例 / 测试 / 稳定焦点时再显式 id（见 §4.2）
ui! { <Button id="save" variant="outline" on_click={move || { /* … */ }}>"保存"</Button> }

// B · Builder（无类 HTML 标签）；空 id 可走自动生成，持久状态勿依赖自动 id
let _: Element = button("save")
    .variant("outline")
    .on_click(move |_| { /* … */ })
    .into();
let _: Element = badge().variant("destructive").text("Error").into();
```

互嵌（验收用例，见 `crates/rui/tests/prelude_dual_surface.rs`）：

```rust
use rui::prelude::*;

// 标签树 ← 嵌入 Builder
let _ = ui! {
    <VStack gap="0.5rem">
        { Element::from(button("x").variant("ghost").text("内嵌 Builder")) }
        <Button variant="outline">"标签子节点"</Button>
        { Element::from(badge().variant("outline").text("内嵌 Badge")) }
        <Badge variant="link">"标签 Badge"</Badge>
    </VStack>
};

// Builder 树 ← 嵌入 ui!
let _: Element = vstack()
    .gap("0.5rem")
    .child(ui! { <Label id="cap">"内嵌标签"</Label> })
    .into();
```

实现细节以各组件 `//!` 与 `From<*Builder> for Element` 为准；宏展开不得绕过公开 Builder 另造私有平行 API（宏生成调用公开函数除外）。

### 3.3 复合组件三层 API（组合 · Opts · 数据糖）

本节约束 **多槽复合控件**（Dialog、Accordion、Questionnaire、Sidebar…）的公开形状。  
**ADR-9** 要求双书写面**行为等价、可互嵌**；**并不要求**「一个根 Builder 链式改完所有内部槽的间距/字号」。  
**样式自定义的主路径**是 **Tier 1 组合面**：各零件自带 `class` / `base`（对位 ShadCN 各 primitive 的 `className`）。

#### 3.3.1 三层模型

```text
┌──────────────────────────────────────────────────────────────┐
│ Tier 1 · 组合面（文档默认推荐 · 自定义样式主路径）              │
│   {comp}_scope(…, body)  或  {comp}().child(|| parts…)       │
│   每槽 = 独立 #[component] / 已构建 Element，class / base 可调 │
├──────────────────────────────────────────────────────────────┤
│ Tier 2 · 零件 Opts（仅有「行为参数」的面板 / 内容壳）           │
│   {Comp}ContentOpts { class, side, motion, … }               │
│   RSX 属性与 Builder 链式方法共用同一 Opts 结构（见 overlay §8.5）│
├──────────────────────────────────────────────────────────────┤
│ Tier 3 · 数据便利糖（Gallery / 快速原型 · 非唯一路径）          │
│   .question() / .item() / .slide() / .link() …                 │
│   内部必须复用 Tier 1 的 render_* / scope，禁止第二套状态机     │
└──────────────────────────────────────────────────────────────┘
```

| 层 | 职责 | 样式怎么调 | 标杆实现 |
|----|------|------------|----------|
| **Tier 1** | Context + 根壳；用户拼零件树 | 每个零件 `class` / `base`；根 `.class()` 或 scope 的 `class` 参数 | `dialog_scope` + `dialog().child()`；`empty().header()`；`progress().child()`；[`overlay_shells.md` §8.5](components/overlay_shells.md) |
| **Tier 2** | 带 placement / motion / trap 等**行为**的内容壳 | Opts 上的 `class` + 零件 `base` | `PopoverContentOpts`、`DialogContentOpts`、`SelectContentOpts` |
| **Tier 3** | 用数据/闭包描述内容，少写树 | 见 **§3.3.3**；不得承载**唯一**行为路径 | `accordion().item()`、`carousel().slide()`、`questionnaire().question()` |

**叶子控件**（Button、Input、Badge）只有一层：Builder 即产品面，不适用本节三层划分。

#### 3.3.2 Tier 1 形状（新增与重构的默认模板）

每个稳定面复合组件应同时具备：

1. **`{name}_scope`** — 提供 Context、根 `data-slot`、受控 Signal 接线；接受根 `class: Option<String>` 与 `body: impl FnOnce() -> Element`。  
2. **零件 `#[component]`** — `{Name}Title` / `{Name}Content` / `{Name}Actions`…；每零件暴露 `class` + `base`（宏已生成）。  
3. **根 Builder** — `{name}(id)` 配置根行为（受控 signal、`on_*`、根 `.class()`），并提供 **`.child(|| …)`** 进入 scope 拼树（对位 `dialog().child()`）。  
4. **RSX** — 复合标签经宏展开为 `{name}_scope` / `{name}_item_scope`，不得绕过公开 scope。

推荐写法（Builder 组合面 · 样式全开）：

```rust
use rui::prelude::*;

questionnaire("plan")
    .shortcuts("letters")
    .class("gap-8") // 根壳
    .child(|| ui! {
        <QuestionnaireProgress class="text-sm" />
        <QuestionnaireItem name="direction" class="gap-4">
            <QuestionnaireTitle class="text-lg">"What next?"</QuestionnaireTitle>
            <QuestionnaireChoices class="gap-2">
                <QuestionnaireChoice value="a" class="py-3">"Option A"</QuestionnaireChoice>
            </QuestionnaireChoices>
        </QuestionnaireItem>
        <QuestionnaireActions class="gap-3">
            <QuestionnairePrevious />
            <QuestionnaireNext />
        </QuestionnaireActions>
    })
```

RSX 与上例 **同一棵零件树、同一套 `class`**，仅书写面不同。

#### 3.3.3 样式规则（硬性）

| 需求 | 做法 | 禁止 |
|------|------|------|
| 改某一槽间距/字号/颜色 | 该零件 `class="…"` 或 RSX 布局属性（进 `base`） | 指望根 Builder 穿透所有槽 |
| 闭包装配的内部格子（日历日、滑块拇指、图轴） | 根 `.slot("calendar-day", \|c\| c.class(..).style(..))`；节点打 `data-slot` | 每组件 `*_with` / `*SlotBuilder` / `*Parts` |
| 改根纵向节奏 | scope / 根 Builder 的 `.class()`（如 `gap-8`） | 在 Tier 3 里硬编码且不可覆盖 |
| 改带行为的内容壳 | Tier 2 `{Comp}ContentOpts { class, … }` | 为纯 `class` 单独再发明 Opts 枚举 |
| 数据糖场景偶尔改壳 | 题级/项级 `class` 字段（见 §3.3.4） | 全库复制 `*PartsClass` 十数字段 |

**合并顺序**（与 ShadCN `cn(defaults, className)` 一致）：先组件默认 utility，再用户 `class`；后写覆盖冲突项。共享 helper 可放在 `style/utility`（如 `merge_slot_class`），**不要**每组件一份 `*PartsClass` 除非有充分数据 DSL 理由（§3.3.4）。闭包装配没有独立零件 Builder 时，用框架 `SlotMap`（根 `.slot("data-slot-name", …)`）；应用级「所有实例统一」走 `[data-slot=…]` 样式表（见 [global-stylesheet-loader.md](plans/global-stylesheet-loader.md)），不要每个实例点名槽。

#### 3.3.4 Tier 3 与 `*PartsClass` 的边界

**默认**：Tier 3 的 `From<*Builder>` 是**薄适配器**——调用与 Tier 1 相同的 `render_*` / 零件函数，而不是在 `From` 里再写一棵 `class: None` 的平行树。

允许在 Tier 3 上增加**少量**根级/项级 `class` 或 **`.parts(|p| …)`** 当且仅当：

- 内容是 **数据 DSL**（如问卷 `question(name, title, |q| …)`），用户合理期望「只描述数据、少写树」；且  
- 文档与 Gallery **同时展示 Tier 1 组合示例**，标明 Tier 3 为便利路径。

`Questionnaire` 的 `QuestionnairePartsClass` / `.parts()` 属于上述**例外**，**不得**作为 Accordion / Carousel / Pagination 等组件的默认模板。

#### 3.3.5 Tier 2 与 overlay 对齐

浮层、菜单、选择器等内容面板：**`*Opts` 为共享真源**（[`overlay_shells.md` §8.5](components/overlay_shells.md)）：

- RSX：`<PopoverContent side="bottom" class="…" />` → `PopoverContentOpts`  
- Builder：`popover_content(PopoverContentOpts { class: Some(…), … }, body)`  
- 禁止 RSX 与 Builder 各维护一套不同的 content 参数表。

#### 3.3.6 新增复合组件 Checklist

落地前自检（库测 + Gallery 各至少一例）：

- [ ] `{name}_scope` 公开，且 `body` 内可放任意零件组合  
- [ ] 每槽 `{Name}*` 零件有 `data-slot`、`class`、`base`  
- [ ] `{name}()` Builder 有根配置 + **`.child(|| …)`**  
- [ ] `ui!` 复合标签展开到 scope（非仅 `#[component]` 直调）  
- [ ] 若有行为内容壳，有 `{Name}ContentOpts` 且 RSX/Builder 共用  
- [ ] 若有 Tier 3，其 `From` 复用 `render_*`，且文档/Gallery 有 Tier 1 对照  
- [ ] 库测：组合面与 Tier 3（若有）同一 `template_id` / 关键 `data-slot` / 行为（开合、提交等）

作者向步骤见 [`custom_components.md`](components/custom_components.md) §复合控件配方。

#### 3.3.7 存量组件治理（不要求一次改完）

按**接缝成本**递增推进，新增组件**直接遵守 §3.3.2**，不等待存量扫尾：

| 优先级 | 动作 | 典型对象 |
|--------|------|----------|
| P0 | 补 **`.child()`** 或公开 `*_scope`，使 Tier 1 可组合 | Accordion、Pagination、Menubar、NavigationMenu、Breadcrumb |
| P1 | `From<*Builder>` 抽 **`render_*`**，去掉重复 `class: None` | Accordion `.item()`、Carousel `From`、Sidebar `From` |
| P2 | 已有 **`*Opts`** 的补 `class` 文档与 Builder 链式透传 | `MenubarContentOpts`、`DropdownMenuContentOpts` |
| P3 | Tier 3 文档降级为「快速原型」；Gallery 补 Tier 1 示例 | 与 §6.3 联动 |
| P4 | 闭包装配内部槽走根 `.slot("data-slot", …)`，不要每组件 `*_with` / `*Parts` | 新复合件直接遵守；Pagination / Carousel 存量另扫 |
| 例外 | 数据 DSL 保留 `.parts()` / 项级 `class`，不推广 `*PartsClass` | Questionnaire |

**Stepper** 等仍无 scope/零件拆分的，应按 §3.3.2 拆为 `stepper_scope` + 零件，而不是在单文件过程式渲染里堆 `class` API。

---

## 4. API 契约（所有公开组件）

受控 / 非受控与细粒度更新口径交叉 [`REACTIVITY.md`](REACTIVITY.md) §7；身份四轴见 [`element_identity.md`](element_identity.md)。

### 4.1 命名与暴露

- **RSX**：PascalCase 标签（`<Button>`）；实现侧 `#[component]` 与/或 Builder `From` → `Element`。  
- **Builder**：`snake_case` 入口函数（如 `button`、`input`）+ `*Builder` 链式方法；与标签**同一模块导出**。  
- 函数名可与标签同名（模块已 `allow(non_snake_case)`）时，须在文档区分「标签用 PascalCase / 调用用模块路径」。  
- 少数暂仅 Builder / 函数：`toast`、`dropdown` 等——文档与 Gallery 须标明；补标签属增强，不阻塞 Builder 一等地位。Dialog 为复合标签（`Dialog` / `DialogTrigger` / `DialogContent` / …）+ `dialog` Builder。

### 4.2 身份与状态（`id` 勿过度使用）

> **跨层权威契约**：不变量、AccessKit 派生、组件/宏/框架职责与事故原型见 **[`element_identity.md`](element_identity.md)**（本文仅组件 API 摘要）。  
> **现状判断**：多数 Builder 把 `id: impl Into<String>` 当作**首位必参**，Gallery 也充满 `gal-*` 字符串。这把「实例键 / 焦点 / 信号命名空间 / 测试选择器」揉进同一个 prop，造成**过度使用 `id`**；空串写入 `template_id`/`focus_id` 曾导致读屏树重复 `NodeId` 崩溃。实现分期收敛见 [`AUTHORITY.md`](AUTHORITY.md) **WP-CV.4 / WP-2.8 / WP-2.4**。

#### 4.2.1 四条身份轴（禁止混为一谈）

| 轴 | 含义 | 典型载体 | 用户是否常写 |
|----|------|----------|--------------|
| **实例键 / 信号命名空间** | 非受控 `use_signal`、hover spring、`{id}__open` 等全局表键 | 今日多由组件 `id` 派生 | 仅多实例或跨子树持久时需要稳定键 |
| **`focus_id`** | Tab / 点击焦点、AccessKit、浮层 `anchor` | `Element.focus_id`；常带前缀（`input:`、`tabstrip:`） | 需要可编程焦点或 a11y 名时 |
| **`template_id`** | 热重载 / `reactive!` **槽位** reconcile（≠ 全局 a11y 主键） | 宏或 `reactive!(id, …)` | 热补丁或显式 anchor 时 |
| **测试 / 调试选择器** | 自动化点选、日志 | 可与上重合，**非**每个控件必填 | 测试需要时 |

**禁止**：为「看起来像 HTML」而给每个静态 Label / 布局壳强行编造 `id`。  
**禁止**：文档暗示「不用 id 就不合法」——`Separator` / `Card` 等无状态控件已证明可以没有用户 `id`。  
**禁止**：`id == ""` 原样写入 `Some("")` 的 `focus_id` / `template_id`（见身份契约 I-1）。

#### 4.2.2 何时**需要**稳定用户可见 `id`（或等价实例键）

| 场景 | 要求 |
|------|------|
| 非受控状态且**多实例**并存（多 Input、多 Dialog） | 稳定唯一键；否则全局 `use_signal("{id}__…")` **撞键串状态**（Input 空 id 注释已明示） |
| 受控 Signal 由外部持有 | **不强制**组件 `id`；状态在调用方 Signal |
| 焦点环 / 读屏 / 浮层锚点需稳定引用 | 显式 `focus_id`（或文档约定由 `id` 派生且稳定） |
| 测试选择器、IME/冒烟脚本 | 显式稳定 id / focus_id |
| 仅单次点击、无内部持久信号的 Button | **可不传**；允许会话内自动 id（见下） |

#### 4.2.3 自动生成 id 的边界

今日实现（事实）：

- `Button`：空 id → `btn-N`（进程内递增）  
- `Input`：空 id → `__rui_input_rt_N`（兜底，避免多框共用 value 信号）  
- `Navbar` / `bar_chart` / `line_chart` / `area_chart` / `pie_chart` / `define_chart`：**无必填 id**（WP-2.8）；空 → 不写 `template_id`  
- `Progress` / `Toggle` / `Checkbox` / `Switch`：空 → `resolve_instance_id`；布尔非受控 → [`resolve_bool_signal`](../crates/rui/src/utils/toggle_state.rs)（Owner 作用域，WP-3.7）  
- RSX 宏：部分省略 id 的标签会生成唯一名或 Default 空串（以宏为准）

| 可以 | 不可以指望 |
|------|------------|
| 单页内区分 hover/paint 键、避免立刻撞键 | **跨 rebuild / 热重载后仍指向同一逻辑实例** |
| 无状态展示、一次性按钮 | 表单字段持久、多实例非受控状态的长期身份 |

自动 id **不是**「用户永远不用想身份」的完整方案；长期方向是 **Owner 作用域信号**（`use_scoped_signal`），减少对全局字符串 `id` 的依赖（见 [`reactivity_and_hot_reload.md`](ops/reactivity_and_hot_reload.md)、[`custom_components.md`](components/custom_components.md)）。

#### 4.2.4 API 与文档约定（目标态）

| 控件类别 | `id` / 实例键 | 说明 |
|----------|---------------|------|
| **展示型**（Separator、Card、纯布局 Navbar 壳等） | **默认不要求** | 若 Builder 仍留 `id` 参数，标为可选或仅用于测试 |
| **有状态 · 非受控**（Input、Checkbox、Select、Dialog open…） | 多实例要稳定键；单实例可框架派生 | 优先受控 Signal，少强迫用户起名 |
| **有状态 · 受控** | 可选 | 行为不依赖组件 id |
| **浮层** | 组件实例键可框架派生；**锚点用 `focus_id` / `anchor_focus_id`** | 勿让用户为 trigger 与 panel 各编两个无关魔法字符串（除非要稳定引用） |

Gallery / 示例：**只在演示焦点、多实例、测试选择器处写 id**；其余按钮勿堆 `gal-btn-1…N`。

#### 4.2.5 状态规则（与身份正交）

| 规则 | 要求 |
|------|------|
| 非受控 | 内部信号键必须落入**唯一命名空间**（显式 id、宏生成 id、或 Owner 作用域）；布尔控件走 [`resolve_bool_signal`](../crates/rui/src/utils/toggle_state.rs) |
| 受控 | 调用方持有 `Signal<T>`；组件**不得**再为同一语义状态建第二份 signal |
| 禁用 | `disabled` 统一关闭指针与键盘激活，并参与 `disabled:` utility 变体 |
| `focus_id` 前缀 | `input:`、`tabstrip:`、`scroll:` 等；AccessKit 依赖前缀；新增前缀须改 bridge + 本文或 `keyboard_nav.md` |

### 4.3 外观（ShadCN）

| 约定 | 要求 |
|------|------|
| `variant` | **产品默认字符串**（对齐 shadcn：`default` / `secondary` / `outline` / `ghost` / `success` / `warning` / `destructive` / `link`…）；对内枚举真源；Builder 鼓励 `impl Into<XxxVariant>` **双收**（`"outline"` 与 `XxxVariant::Outline`）。通用口径 **§3.1.1**；**冻结表 + §0** [`component_variants.md`](components/component_variants.md) |
| `size` | 同上：字符串产品面 + 可选 `Into<XxxSize>`；合法阶梯 `xs` / `sm` / `default` / `lg` / `icon*`；与 Button 对齐者优先复用，差异须登记冻结表 |
| 颜色 | 经 `parse_color("primary")` 等语义名；令牌表与明暗见 [`theme.md`](engine/theme.md)；OKLCH→display sRGB 数学见 [`color.md`](engine/color.md)；禁止组件内硬编码与 Theme 无关的色值 |
| `outline` 变体 | 语义是 **border**（盒模型边框），**不是** CSS `outline` 属性；焦点环用 shadow/`ring`，见 [`color.md`](engine/color.md) |
| 焦点环 | 默认走 `style/focus_ring` + theme `ring`；`focus-visible` 行为与冒烟表一致 |
| 动效 | hover/focus 可用 spring/cubic（Button 已用 spring + 线性 sRGB lerp）；paint sync 优先，避免整树 rebuild |

**禁区（外观轴）**：为对齐 Freya 等纯枚举 API，废除 Builder / RSX 字符串入口，或拆成 `.variant_str` / `.variant_enum` 两套平行方法。

### 4.4 浮层

> **编排收敛**：[overlay_shells.md](components/overlay_shells.md) — **`utils/anchored_overlay`（WP-OVL.2/3 ☑）与 `utils/viewport_modal`（WP-OVL.1 ☑）已落地**。新浮层走壳；存量产品组件 WP-OVL.4 再薄包装。

必用 API（新组件不得绕开）：

| 形态 | 必用 | 禁止 |
|------|------|------|
| 锚定（Select / Combobox / Date / Color / Tooltip / 有锚 Popover / Dropdown / Menu） | `portal_sync` + `mark_floating` + [`mount_anchored_overlay`](../crates/rui/src/utils/popup.rs) | `portal_panel` 当 flex 兄弟；手写第二套 `left/top`；关着仍把菜单留在布局里 |
| 视口模态（Dialog / Sheet / CommandOverlay） | `portal_sync(Modal)` + [`mount_viewport_portal_stage`](../crates/rui/src/utils/popup.rs)（+ underlay）；舞台内只 [`swallow_clicks`](../crates/rui/src/utils/popup.rs) | `panel_block_clicks` 再包 `portal_panel`（嵌套 hoist） |
| Esc | `portal_overlay` LIFO；只关顶层 | 每控件自写全局 Esc |

- **命中默认**：hoist 后的浮层 / 弹窗 / 菜单轮廓为 `pointer-events: auto`，挡住点击穿透；仅特殊壳层显式透传——根 portal 叠层、视口居中舞台空白区（点到 underlay）、0×0 锚、Toast 宿主 / 动画中的 `none` / `box-none`。禁止再依赖「无 handler → 默认 box-none」。  
- Escape **只关顶层**（库测 `push_two_esc_closes_top_only` / `three_layers_esc_closes_top_only_each_time`）；点击外部关闭与受控 `open` 信号单一写入点。  
- Tab：有顶层 Stacked 时在该层面板循环（[`keyboard_nav.md`](ops/keyboard_nav.md)）；显式 `sync_focus_trap` 仍属 **C-WP-1.3**。  
- 打开时焦点陷阱与关闭后焦点归还须可测或可冒烟。

**存量审计（WP-3.4 ☑）**

| 组件 | 路径 | 备注 |
|------|------|------|
| Select / Combobox / DatePicker / ColorPicker / Tooltip | 锚定 helper | 浮层体 `isolate_reads`，勿 `untrack` |
| Popover | 有锚 → helper；无锚 → `panel_block_clicks` | 无锚不是第二套 placement |
| Dropdown / ContextMenu / Menubar | 锚定 helper | Dropdown 关闭不挂菜单 |
| NavigationMenu | `mark_floating` + `panel_block_clicks`；非 viewport 外包 0×0 锚 | `viewport` 槽为产品 in-flow，标例外 |
| Dialog / Sheet / CommandOverlay | 视口舞台 | CommandOverlay chrome 为 top-1/3 |
| CommandDialog | 复用 Dialog 壳 | — |
| Toast | pinned `portal_panel` | 通知宿主，非锚定 placement |

### 4.5 无障碍最低集

| 控件类 | Role / 行为（最低） |
|--------|---------------------|
| Button / Toggle | Button；名称取自后代可见文案或 `aria-label`；pressed 时 toggled |
| Input / Textarea | TextField；值与焦点同步；可用 `aria-labelledby` 关联 Label |
| Label（静态） | **不进 Tab**；进 a11y 树为 Label（文本在 `value`）；关联控件用 `accesskit_id` + `aria-labelledby` |
| Tabs 条带 | TabList（`tabstrip:`） |
| Checkbox / Switch | 可 toggled |
| 菜单 / Command / Combobox | 打开关闭 + 键盘移动（深化见巩固期 WP） |

细节实现以 `a11y/bridge.rs` 为准；读屏步骤见 [`a11y_smoke.md`](ops/a11y_smoke.md)；新增前缀必须同时改 bridge 与本文或 [`keyboard_nav.md`](ops/keyboard_nav.md)。

---

## 5. 组件清单（代码事实 · 2026-09-05）

来源：`components/mod.rs` `pub use` + Gallery 分区。  
**分桶权威表**（`experimental` / `verified` / `planned` / `broken` + **id 策略**）：[`component_inventory.md`](components/component_inventory.md)。  
粗口径：**Gallery 验证** = 对应分区可演示且无未分类 P0（2026-09-05 产品面已走查）；**工程** = 满足 §1；**SemVer 0.1** = §5.2.1（窄于 Gallery 验证面）；样式承诺面见 §5.2.2；尚未 `pub use` 的对位为 `planned`（inventory §1.9）。

参考分工：ShadCN → 产品命名与分区；Solid → 受控 Signal / 细粒度 sync（非组件表）；EUI → **密度标尺**（inventory §3）+ **视觉契约**（[`EUI_NEO_VISUAL.md`](comparison/EUI_NEO_VISUAL.md)），不搬架构。

### 5.1 按 Gallery 分区

| 分区 | 组件（`components/` 模块） | 分桶摘要（详见 inventory） |
|------|----------------------------|----------------------------|
| **基础** | Button、Badge、AspectRatio、Label、Separator、Card、Toggle、ToggleGroup、Progress、Image、Kbd、Skeleton、Empty、Marker、Message、MessageScroller、Alert、AlertDialog、Item、Attachment、List、Bubble、Avatar、Breadcrumb、ButtonGroup | 产品面 `verified`（Gallery 2026-09-05）；CV.1 核心仍为 Button/Label/Separator/Card |
| **表单** | Input、InputAdornment、InputOTP、Textarea、Checkbox、Switch、RadioGroup、Slider、Select、Combobox、Calendar、DatePicker、TimePicker、ColorPicker、Questionnaire；Form helpers；文件选择为 `utils/file_dialog` | 输入/选器 `verified`；**Form** = `experimental`（Context helpers，已降出 §5.2.1）；Questionnaire / FileDialog = `experimental` |
| **浮层 / 反馈** | Dialog、Sheet、Drawer、Popover、Tooltip、HoverCard、DropdownMenu、ContextMenu、Toast、Command；**AnchoredOverlay / ViewportModal** 无头壳；**Drag**（`utils/drag` 实验） | Dialog/Toast/Tooltip 含 CV 测；其余产品面 `verified`（Gallery）；Command 搜索算法仍可加深；Drag = `experimental` |
| **数据** | Table、VirtualList、VirtualMasonry、TreeView、Pagination、Chart、Carousel、Stepper、Timeline | Table/VirtualList/TreeView/Pagination/Carousel/Stepper/Timeline = Gallery `verified`；**Chart** / **VirtualMasonry** = `experimental`（不进 SemVer 0.1） |
| **绘制** | Svg（`primitives/svg`）；Canvas（`primitives/canvas`）；Scene3d（feature）；B16 / B17 仍规划 | Svg/Canvas = `experimental`（WP-3.11/3.12 ☑）；Scene3d = `experimental`（WP-3.13 ☑，默认 feature 关）；B16/B17 未落地 |
| **导航** | Tabs、Menubar、Navbar、NavigationMenu、Sidebar、Pagination；Gallery 另演示 Router / Navigator / 次窗 | Tabs 含 CV 测；其余产品面 `verified` |
| **布局** | ScrollArea、ScrollViewport、Scrollbar、Resizable、Accordion、Collapsible | ScrollArea/Scrollbar 含 CV 测；Accordion/Collapsible/Resizable/Viewport = Gallery `verified` |
| **文本** | Label 选区、markdown inline / 块级 md（`text/`） | 选区随 Label；inline=`experimental`；块级 md **缺失** |

**ShadCN `planned`（尚未 `pub use`，详见 inventory §1.9）**：

| 优先级 | 规划组件 | 建议分区 |
|--------|----------|----------|
| P1 | Spinner | 基础 |
| P2 | — | 多数 P1/P2 已落地（Avatar / Skeleton / Alert / Accordion / Collapsible / Breadcrumb / Empty / ButtonGroup / AlertDialog / Drawer） |
| 等价不单列 | Sonner→Toast、NativeSelect→Select、Field→Form | Drawer **已独立导出**（vaul 式 snap）；勿再等同 Sheet |

**不在 `components/`、勿当缺失组件**：`Portal`/`overlay`/`path`/`svg`/`canvas`/`scene3d`→`primitives/`；`popup`/`roving`/`focus_trap`/`text_field`→`utils/`。`planned` 未落地前**禁止**写入 prelude 或假装已导出。

### 5.2 发布承诺面（SemVer 口径草案）

#### 5.2.1 组件稳定面（CV 范围 · inventory §2）

| 稳定面控件（0.1 起承诺 semver） | 实验面（可破坏，须标 `experimental` 或文档警告） |
|--------------------------------|--------------------------------------------------|
| Button、Label、Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Separator、Card | Chart / Path 绘制细节、VirtualMasonry、Scene3d |
| ScrollArea / Scrollbar、Tabs、Dialog、Toast、Tooltip | Date/Time/Color 本地化与时区、FilePicker 平台差异 |
| Select、Combobox（基础开合） | TreeView 大数据、Table 分组/pinning；**Form**（Context helpers） |
| | Command 搜索算法、Questionnaire DSL；其余 Gallery `verified` 但未写入 0.1 的产品控件（见 inventory §2） |

**分桶快照（2026-09-05）**：Gallery 产品面已走查并升 inventory `verified`；§5.2.1 **SemVer 0.1 名单不自动扩大**（W4 / WP-4.1 决定）。Form = `experimental`；**broken = 无**；**planned** 仅余 Spinner 等（inventory §1.9）。升格/降级只改 [`component_inventory.md`](components/component_inventory.md) §2 并回写本小节。

#### 5.2.2 样式承诺面（非控件 · 不进 CV · inventory §1.8）

| 承诺面（0.1 起承诺 semver） | 延后 / 实验 |
|----------------------------|-------------|
| Theme / `set_theme_mode`、utility `class` 主集（`rui_style_tokens`，含 `box-border`/`box-content`） | 完整 CSSOM、分边 border、真视口单位、局部 `em` 见 [`box_model.md`](engine/box_model.md) / AUTHORITY B15 |

验收归属工程域：WP-2.9、[`theme.md`](engine/theme.md)、换肤冒烟；**禁止**用 Gallery 控件走查或 verified/stable-candidate 分桶标记。

#### 5.2.3 `components/` 发布模式（W4 决策）

- 当前采用 **伞形 crate**：`crates/rui/src/components` 作为 `rui` crate 的产品 API 面随 `rui` 一并发布（用户入口 `cargo add rui`）。
- `components/` 当前**不单独拆 crate**：Beta 前避免额外版本耦合、发布顺序复杂度与迁移成本。
- 稳定/实验边界继续以 §5.2.1 与 [`component_inventory.md`](components/component_inventory.md) 为准；破坏性调整通过 `rui` 的 SemVer + CHANGELOG 对外声明。
- 若后续评估拆出独立组件包（如 `rui_components`），前置条件：API 边界稳定、CI/冒烟独立、迁移指南与兼容层齐备（Beta 后再议）。

### 5.3 明确不做（组件库范围）

- 换 VDOM / 弃 wgpu / 引入浏览器控件。  
- 为「纯无头公共 API」另起一套与 `components/` 平行的完整产品面（可渐进抽出 headless builder，但默认文档与 prelude 仍指向样式组件）。  
- **废除或边缘化 Builder / 纯 Rust 书写面**（或反之只保留 Builder、废弃 `ui!`）——双书写面均为一等公民，见 §3。  
- **把用户 `id` 当作每个控件的必填 HTML 式属性**——身份轴拆分见 §4.2；过度必填是债务，不是设计目标。  
- **复活独立 `AlertDialog` 薄封装 Dialog**——产品面已有独立 [`AlertDialog`](../crates/rui/src/components/alert_dialog.rs)（阻塞确认 + 语义 variant）；页内非模态横幅用 [`Alert`](../crates/rui/src/components/alert.rs)。禁止再把 AlertDialog 删成 Dialog 别名。  
- **把 Drawer 当成 Sheet 的别名**——Drawer 为 vaul 式 snap；Sheet 为侧滑面板。  
- 移动 / 鸿蒙一等公民组件变体。  
- 完整 CSSOM、DOM 选择器、Web 专有 a11y 树。

---

## 6. 质量门与验收模板

### 6.1 每个新组件 / 行为破坏性改动的 PR 检查表

- [ ] 分层：新状态机是否应在 `utils`/`primitives` 而非仅组件内复制  
- [ ] **作者面**：`#[component]` 已默认 Owner；Builder 复合根是否 `component_root`；浮层子树是否 `isolate_reads` 而非 `untrack`；浮层是否走 `mount_anchored_overlay` / 视口舞台（§2.3 / §4.4）  
- [ ] 受控 / 非受控单一写入点  
- [ ] **身份**：未把 `id` 当作万金油；区分实例键 / `focus_id` / `template_id`（[`element_identity.md`](element_identity.md) + §4.2）；空串不写入 focus/template；展示型不强制用户起 id  
- [ ] Theme token，无硬编码色  
- [ ] **双书写面**：稳定面新 props 同时进 RSX 与 Builder（或文档标明例外）；互嵌仍编译  
- [ ] Gallery 对应分区可见；**示例不堆砌无必要 id**  
- [ ] 库测或 `desktop_smoke` 条目（浮层 / 焦点至少其一）  
- [ ] 若属稳定面：更新组件索引与 CHANGELOG 草稿  

### 6.2 自动化基线

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test -p rui --lib
cargo test -p rui --lib offscreen
cargo check -p gallery
```

组件 golden 目标（巩固期 / WP-2.6）：Button、Input、Dialog、Tabs、ScrollArea ≥5 场景进 CI（稳定面；VirtualList/Markdown 升格后再加深）。

### 6.3 Gallery 契约

- 侧栏分区与 §5.1 一致：基础 · 表单 · 浮层/反馈 · 数据 · **绘制** · 导航 · 布局 · 文本 · 关于；Svg / Canvas / Scene3d 已在「绘制」（WP-3.11 / 3.12 / 3.13）；Chart 仍在「数据」。Scene3d 演示需 `--features scene3d`。浮层分区含 **Drag**（`DragShowcase`）。  
- **每公开稳定面组件至少一处可运行示例**。  
- 变体矩阵优先展示在「基础 / 表单」（Button variants/sizes 已是标杆）。  
- **复合组件**：Gallery 宜同时展示 **Tier 1 组合面**（`.child()` / 手写零件 `class`）与 **Tier 3 数据糖**（若有），见 **§3.3**。  
- 「关于」或基础区宜保留 **Builder 写法** 对照示例，避免用户以为只能写标签。

---

## 7. 文档拓扑（组件相关）

```text
AUTHORITY.md                       ← ★执行源（CV / 组件 WP 并入统一计划）
ENGINEERING_GRADE_FRAMEWORK.md     ← 总纲领（ADR-6 / ADR-9）
REACTIVITY.md                      ← 细粒度领域规范（受控 Signal / 三档更新）
COMPONENT_LIBRARY.md               ← 本文件（组件领域规范）
components/component_inventory.md  ← §5 分桶附录
element_identity.md                ← 跨层身份契约
crates/rui/src/prelude.rs          ← `use rui::prelude::*`
archive/COMPONENT_LIBRARY_PLAN.md  ← 冻结展开（C-WP 细则）
components/headless.md · custom_components.md
ops/keyboard_nav.md · a11y_smoke.md · reactivity_and_hot_reload.md
engine/theme.md · engine/color.md · engine/color_advanced.md
comparison/gap_analysis.md §6      ← 历史 EUI 密度摘要（真源转 inventory §3）
```

争议处理：先 ADR，再改本规范；禁止静默改变「默认交付面 = shadcn 样式组件」或「双书写面互相兼容」。受控 Signal / 细粒度更新争议以 [`REACTIVITY.md`](REACTIVITY.md) 为准。**自定义组件难写**以本文件 **§2.3** 为准。排期以 [`AUTHORITY.md`](AUTHORITY.md) 为准。

---

*规范随清单与 SemVer 面更新而修订；新增公开组件必须落入 §5 分区并更新 Gallery。*
