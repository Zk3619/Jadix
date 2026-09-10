# Runtime 扩展点与拆 crate 注册协议

> **范围说明**：本文最初从 `window_events.rs` 的「组件点名簿」切入；拆 crate 与自定义组件接入时，**同类硬编码遍布 runtime 各层**（帧调度、绘制 sync、样式/动画、RSX 宏、资产、输入协议等）。下文以 **window event 为样板**，列出拆箱时应统一提供的 **自注册扩展点**，使 `rui`（core/runtime）不再 `use rui_components::*`。  
> **交叉**：指针拖 / DnD / resize·gesture 的分层与可选 crate 见 [`pointer_and_drag_architecture.md`](pointer_and_drag_architecture.md)（Phase 0 对接本文 `FrameHook` / `PointerCapture`）。

## 目标

用户或第三方 crate 编写自定义组件时，应能通过 **注册** 接入框架，而不是：

1. 改 `window_events.rs` / `state.rs` / `lifecycle.rs` 加一行 `tick_*` / `sync_*`；
2. 改 `rui_macros/src/lib.rs` 加 `<MyWidget>` 分支；
3. 改 `style/keyframes.rs` 或 `PropertyId` enum 才能动效；
4. fork 主 crate 才能发布组件库。

**原则**：runtime 只维护 **协议 + 注册表 + 调度循环**；具体组件在 init / `#[component]` / builder 构造时 **自注册**。

---

## 现状总览：硬编码地图

| 层 | 典型位置 | 模式 | 拆 crate 后 |
|----|----------|------|-------------|
| **帧 / 指针** | `window_events.rs` | `tick_*_drag`、`clear_*_drag`、`tick_*_presence` | `FrameHook` + `PointerCapture` |
| **空闲生命周期** | `lifecycle.rs` | `tick_toast_lifecycle`、`tick_carousel_autoplay`、`tick_hover_card` | 同上 `FrameHook::on_idle_tick` |
| **绘制 sync** | `state.rs` | 逐个 `sync_*_subtree`（button、tabs、checkbox…） | `Element::reactive_sync` 或 `PaintPassHook` |
| **hoist 后 pass** | `state.rs` | `modal_motion::flush/tick`、`accordion::tick` | `LayoutPassHook`（post-hoist） |
| **CSS 动画 tick** | `window_events.rs` + `state.rs` | `tick_animations`、`tick_active_css_transitions_subtree` | runtime 通用；组件只注册 keyframes / 属性 |
| **@keyframes** | `style/keyframes.rs` | 内置名 + `register_keyframes` | **已有 API**；缺组件侧 init 惯例 |
| **可动画属性** | `style/animatable.rs` | `PropertyId` 编译期 enum | 需 **插件式 animatable 注册**（或稳定子集 + 扩展 trait） |
| **clip-path / filter** | `style/clip_path.rs`、`filter.rs` | `register_clip_path`、`register_svg_filter` | **已有 API** |
| **RSX 标签** | `rui_macros/src/lib.rs` | 数千行按组件名 match | **`register_ui_tag` / 声明式 manifest** |
| **复合件 RSX** | `rui_macros` | `DialogTrigger`、`CollapsibleContent`… 硬编码 | 组件 crate 导出 `UiTagDescriptor` |
| **图标 / 光标** | `utils/icon_provider.rs`、`style/cursor_image.rs` | `register_icon`、`register_cursor_rgba8` | **已有 API** |
| **Presence / motion** | `modal_motion::PRESENCE_REGISTRY` 等 | 组件内注册，tick 仍在 app | 注册 + **统一 presence tick hook** |
| **Input / IME** | `window_events.rs`、`text_edit` | `__rui_input:` 前缀、硬编码 Input 路径 | `TextEditHost` trait + focus 前缀协议 |
| **Keyed 列表** | `app/keyed_list.rs` | `KEYED_LIST_REGISTRY` | 保持内部；VirtualList 作者用 builder API |

下面分节展开；**§1 window event** 是样板，其余为同等重要的注册面。

---

## §1 帧调度与指针（window event 样板）

### 现状

`window_events.rs` 帧级轮询清单（节选）：

```rust
let slider_dragging = crate::components::slider::tick_slider_drag(x, y);
let toast_dragging = crate::components::toast::tick_toast_drag(x, y);
// … color_picker, carousel, resizable, drawer, anchored_overlay …
if slider_dragging || toast_dragging || …
    || modal_motion::tick_modal_presence()
    || accordion::tick_accordion_panel_presence()
{
    window.request_redraw();
}
```

同类模式：

| 位置 | 模式 |
|------|------|
| `window_events.rs` mouseup | 逐个 `clear_*_drag_if_any()` |
| `lifecycle.rs` | `tick_toast_lifecycle`、`tick_carousel_autoplay`、`tick_number_step_hold` |
| `state.rs` hoist 后 | `modal_motion::flush/tick`、`accordion::tick` |

本质：**runtime 知道「哪些组件需要在每帧/每次指针事件时被推进」**，但没有统一协议。

### 为什么会变成这样

1. **早期缺通用扩展点** — 没有 `FrameService` / hook registry，新组件只能直接在 `window_events` 加一行。
2. **拖拽各自为政** — 各组件全局 drag 状态 + `tick_*`，runtime 只问「要不要 redraw」。
3. **Input/Scrollbar 更特殊** — 与 hit-test、IME、focus 强耦合，逻辑嵌在事件路径里。
4. **Presence** — `PRESENCE_REGISTRY` 已有雏形，但 **tick 入口仍暴露在 app 层**。

### 目标协议

#### 1.1 `FrameLoop` + 注册表

```rust
// rui crate — 不知道具体组件
trait FrameHook: 'static {
    fn on_pointer_move(&mut self, ctx: &mut PointerFrameCtx) -> FrameAction;
    fn on_pointer_up(&mut self, ctx: &mut PointerFrameCtx) -> FrameAction;
    fn on_idle_tick(&mut self, now: Instant) -> FrameAction;
}

struct FrameAction {
    needs_redraw: bool,
    next_wake: Option<Instant>,
}
```

组件 crate 在 init 时 `register_frame_hook(Box::new(SliderDragHook))`。  
`window_events` / `lifecycle` 只做：

```rust
for hook in frame_hooks.iter_mut() {
    if hook.on_pointer_move(ctx).needs_redraw { request_redraw(); }
}
```

`modal_motion::tick_modal_presence`、`accordion::tick`、toast lifecycle、carousel autoplay 均变为 hook。

#### 1.2 统一 `PointerCapture`

合并 `tick_*_drag` + `clear_*_drag_if_any` + `is_*_dragging`：

- 按下时 `capture(CaptureKind::Drag { id, handler })`
- move 时分发给当前 capture
- 抬起时 `release_capture()`

runtime 只维护 **一个** capture 栈。

#### 1.3 Input / Scrollbar

协议可 generic，实现可外置：

- `FocusId` 前缀协议（`__rui_input:` 等）
- `TextEditHost` trait — `input_value_signal_key` 留在 `rui-input` crate

---

## §2 绘制 sync 与 layout pass

### 现状

`state.rs` 帧同步仍逐个 import 组件：

```rust
crate::components::button::sync_button_hover_paint_subtree(...);
crate::components::tabs::sync_tabs_trigger_subtree(...);
crate::components::checkbox::sync_checkbox_checked_subtree(...);
// … command, context_menu, switch, radio_group, sidebar, bubble, avatar …
```

Modal / floating hoist 后另有 `flush_modal_shell_motion_subtree`、`tick_hoisted_modal_css` 等硬编码。

### 已有正确方向

[`ReactiveSync`](../../crates/rui/src/core/tree.rs) — 组件在根节点挂 callback，runtime 遍历 `sync_reactive_elements_subtree_ex` 触发，**不重建子树**。Button、Switch、Checkbox 等应全部迁移到此路径。

### 目标协议

| 机制 | 用途 | 注册方式 |
|------|------|----------|
| **`ReactiveSync`** | 信号 → 样式/布局/文本，单节点更新 | `el.with_reactive_sync(...)` 构造时自带，无需 app 点名 |
| **`PaintPassHook`** | 仍需 viewport 剪枝的全树 pass（focus ring、class variant hover） | `register_paint_pass_hook`；runtime 在 sync 阶段调用 |
| **`LayoutPassHook`** | portal hoist **之后** 的 motion flush / keyed overlay 调和 | `register_layout_pass_hook(Phase::PostHoist, …)` |

`sync_*_subtree` 点名簿与 §1 的 `tick_*` 点名簿 **同构**，应同批消除。

---

## §3 样式与动画注册

### 现状

| 能力 | 现状 | 自定义组件缺口 |
|------|------|----------------|
| **`@keyframes`** | `register_keyframes` / `parse_and_register_keyframes` 已公开 | 组件应在 `mod` init 或 theme 加载时注册；文档缺「作者惯例」 |
| **可动画属性** | `PropertyId` 为 **编译期 enum**（`animatable.rs`） | 第三方 **无法** 注册新 interpolable 字段而不改 core |
| **`clip-path: url(#id)`** | `register_clip_path` | 已可用 |
| **SVG filter** | `register_svg_filter` / `register_svg_filter_graph` | 已可用 |
| **编程式 Animation** | `style/waapi.rs` 句柄注册表 | 组件 `Element::animate(...)` 即可；runtime tick 已通用 |
| **Presence + transition** | `utils/presence`、`utils/motion` | 组件内自管；presence tick 仍漏在 app（见 §1） |
| **主题 keyframes** | `keyframes.rs` 内置 `accordion-up`、`collapsible-down` 等 | 应迁至组件 crate init，或 `rui-theme` 集中注册 |

### 目标协议

1. **Keyframes / clip / filter / cursor** — 保持现有 `register_*` 函数；拆 crate 后 re-export 自 `rui::style`，组件 `#[link_section]` 或 `inventory` 式 **静态注册**（可选）减少漏调。
2. **Animatable 属性** — 长期：`PropertyId` + 静态表 → **运行时 animatable 插件表**（或 `rui-style` 子 crate 持有 enum，组件 feature 扩展）。短期：文档冻结「仅 core 列出的属性可 keyframes」。
3. **CSS tick** — `tick_animations`、`tick_active_css_transitions_subtree` **留在 runtime**，组件不注册；组件只注册 **定义**（keyframes 名、clip id）。

详见 [`animation_and_motion.md`](animation_and_motion.md)（第一部分分层 · 第二部分冻结子集）。

---

## §4 RSX 宏与 `ui!` 标签

### 现状

[`rui_macros/src/lib.rs`](../../crates/rui_macros/src/lib.rs) 按 **组件名字符串** 分支：

- 通用标签 → `Element` builder
- `Button` / `Input` / `SidebarMenuButton` → 特殊 prop 绑定、`bind_attr`、匿名 id 序号
- 复合件 → `DialogTrigger`、`CollapsibleContent`、`MessageScrollerProvider` 等 scope 顺序
- SVG 标签 → `svg_tag_canonical` 硬编码表

自定义 `#[component]` 函数 **可在 Builder 路径使用**，但 **`<MyWidget />` 在 `ui!` 中** 要么走 generic（丢失 prop 别名/复合件语义），要么 **改宏源码**。

### 目标协议

拆 crate 前需要 **声明式标签注册**，而非无限膨胀 `match name_str`:

```rust
// 组件 crate（或 rui-components 聚合 init）
ui_tag! {
    name: "MyWidget",
    builder: my_widget,
    props: {
        variant => bind_variant,
        disabled => bind_bool,
        onClick => alias("on_click"),
    },
    compound: ["MyWidgetTrigger", "MyWidgetContent"], // 可选
}
```

或 compile-time manifest JSON / `inventory` 收集，由 `rui_macros` **生成** dispatch 表。

**最低可行**：文档约定第三方组件 **仅 Builder + `#[component]`**，RSX 用 `{ Element::from(my_widget(...)) }`；内置组件继续硬编码直至 manifest 落地（见 [custom_components.md](../components/custom_components.md) §双写面）。

---

## §5 资产与主题

| API | 位置 | 用途 |
|-----|------|------|
| `register_icon` | `utils/icon_provider.rs` | 自定义图标源 |
| `set_icon_provider` | 同上 | 替换 Lucide 解析 |
| `register_cursor_rgba8` | `style/cursor_image.rs` | 自定义光标 |
| `register_path_scene` | `primitives/path.rs` | Chart / SVG path 共享场景 |

均已公开；拆 crate 后归入 `rui-assets` 或保持 `rui::` re-export。**无需 app 层点名**。

---

## §6 已部分通用（保留并推广）

| 机制 | 状态 |
|------|------|
| `ReactiveSync` | 组件自管 sync — **正确方向**，应替代 `sync_*_subtree` |
| `modal_motion::PRESENCE_REGISTRY` | 内部注册 — **半成品**（tick 仍在 app） |
| `portal_sync` / overlay stack | app 层通用 — **较好** |
| `register_keyframes` / clip / filter | 公开 API — **可用** |
| `request_redraw()` | 通用 — **OK** |
| 逐组件 `tick_*` / `sync_*` / RSX match | 硬编码 — **待重构** |

---

## §7 自定义组件作者检查清单（目标态）

接入自定义组件 crate 时，作者应只需：

1. **`#[component]` + Builder** — 构造 `Element`（已有）
2. **`register_frame_hook`** — 拖拽 / autoplay / presence tick（若需要）
3. **`el.with_reactive_sync`** — 信号驱动样式（替代请求 app 加 `sync_*`）
4. **`register_keyframes` / `register_clip_path`** — 主题动效（若需要）
5. **`register_icon`** — 自定义图标（若需要）
6. **`ui_tag!` 或 Builder-only** — RSX 标签（manifest 落地前用 Builder）
7. **（可选）`register_layout_pass_hook(PostHoist)`** — portal 壳 motion

**不应**需要：改 `window_events.rs`、`state.rs`、`rui_macros` 主分支。

---

## §8 建议演进顺序

与 [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md)「Beta 前 components 不单独拆 crate」兼容；以下为 **拆 crate 前** 在 monolith 内完成的抽象，行为不变。

| 阶段 | 内容 | 产出 |
|------|------|------|
| **R1** | `app::frame_hooks` | 迁入 `window_events` + `lifecycle` 全部 `tick_*` / `clear_*`；删 app 对组件 drag 的 import |
| **R2** | `PointerCapture` | 合并 drag 系列 |
| **R3** | `ReactiveSync` 全覆盖 | 删除 `state.rs` 中 `sync_*_subtree` 点名；保留 focus ring / class variant 等通用 pass |
| **R4** | `LayoutPassHook` | hoist 后 modal/floating/accordion flush |
| **R5** | keyframes 外迁 | 组件/theme init 注册；`keyframes.rs` 只保留解析与表 |
| **R6** | `ui_tag` manifest | 宏从 manifest 生成；内置组件迁入 manifest |
| **R7** | Input/IME 协议化 | `TextEditHost`；最后动（与 `tree_hit` 耦合最深） |
| **R8** | animatable 插件表 | 可选；依赖 R5 与 damage 分级稳定 |

---

## §9 与 Dialog bug 的关系

Modal 问题根源在 **motion 构建/hoist 时序**，不在 `window_events` 是否 call `tick_modal_presence`。  
但 `tick_modal_presence` 放在 `window_events` 说明：**presence 生命周期没有纳入统一帧模型**，这也是架构债的一部分。R4 一并治理。

---

## 结论

`window_events.rs` 的「组件点名簿」只是 **runtime 硬编码的一角**。拆 crate 与第三方自定义组件要求在各层提供 **对称的注册协议**：

- **帧 / 指针** → `FrameHook` + `PointerCapture`
- **绘制** → `ReactiveSync` + `PaintPassHook` + `LayoutPassHook`
- **动画定义** → 现有 `register_*` + 作者 init 惯例
- **RSX** → 标签 manifest（或过渡期 Builder-only）
- **资产** → 现有 `register_icon` 等

Monolith 阶段可以暂时堆显式调用；**任何新组件 PR 应优先走已有注册 API**（`ReactiveSync`、`register_keyframes`），避免 lengthen 点名簿。准备拆 crate 时按 §8 **R1→R7** 顺序抽扩展点，使 `rui` core 不再认识具体组件名。
