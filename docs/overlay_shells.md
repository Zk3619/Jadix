# 浮层无头壳：`anchored_overlay` 与 `viewport_modal`

> **状态**：设计收敛（**未落地**）。  
> **从属**：[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §4.4（浮层单一路径 · WP-3.4 ☑）、[headless.md](headless.md)、[custom_components.md](custom_components.md)。  
> **背景**：[custom_components_suggestions.md](custom_components_suggestions.md)（薄壳 + 下沉原语）、[plans/组件交互下沉_145e4c9e.plan.md](plans/组件交互下沉_145e4c9e.plan.md)（WP-PRIM.1–4 已 ☑）。  
> **日期**：2026-09-02  

本文把 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §4.4 的两条浮层路径，收敛为 **`utils` 层两个一等无头编排壳**。目标是：新做一个中等浮层（含自定义随意放置面板）可在数百行内完成，且不复制 Popover / Dialog 里的 portal / presence / motion 仪式。

---

## 1. 为什么要收

### 1.1 现状

浮层能力已分散在三处，作者必须「读源码拼配方」：

| 层 | 路径 | 职责 |
|----|------|------|
| 几何原语 | [`utils/popup.rs`](../crates/rui/src/utils/popup.rs) | `mark_floating`、`sync_floating_placement`、`mount_anchored_overlay`、`mount_viewport_portal_stage` |
| Esc / z 栈 | [`app/portal_overlay.rs`](../crates/rui/src/app/portal_overlay.rs) | `portal_sync` / `portal_sync_presence`、`OverlayKind`、`OverlayMode` |
| 内部编排 | [`components/internal/modal_shell.rs`](../crates/rui/src/components/internal/modal_shell.rs)、[`floating_motion.rs`](../crates/rui/src/components/internal/floating_motion.rs)、[`modal_motion.rs`](../crates/rui/src/components/internal/modal_motion.rs) | presence、入退场、underlay 默认、hoist 后 motion flush |

每个产品组件（Select、Popover、Dialog…）仍重复：`reactive_anchor` → `portal_sync` → `isolate_reads` → `apply_*_motion` → `mount_*`。

### 1.2 目标

| 指标 | 验收 |
|------|------|
| 新浮层组件行数 | < 500 行（见 [custom_components_suggestions.md](custom_components_suggestions.md) §3） |
| 第二套 portal | **禁止**（WP-3.4 回归） |
| prelude | **不进**；显式 `use rui::utils::anchored_overlay` / `viewport_modal` |
| 产品外观 | 仍在 `components/*`；壳不写 `bg-popover`、圆角等令牌 |
| 双书写面 | **必须** RSX 标签 + Builder / `*Opts` fn（§8.5）；不进 prelude |

### 1.3 不是什么

- **不是** ShadCN 式 `<Dialog>` / `<Popover>` 产品组件。  
- **不是** 替代 [`utils/popup.rs`](../crates/rui/src/utils/popup.rs)——几何与 placement 算法仍留在 popup。  
- **不是** 替代 [`primitives/portal.rs`](../crates/rui/src/primitives/portal.rs)——`<Portal>` 仍用于应用自管的简单树外挂载；完整交互浮层应走本文两个壳。  
- **不是** 替代 [`utils/keyed_overlay.rs`](../crates/rui/src/utils/keyed_overlay.rs)——多实例同 key 调和（Toast 等）仍用 KeyedOverlay；壳可与 keyed 组合。

---

## 2. 分层模型

```
作者 / 产品组件 (Dialog, Popover, 自定义浮层)
        │
        ▼
┌───────────────────────────────────────────────────┐
│  utils/anchored_overlay.rs   utils/viewport_modal.rs │  ← 本文：编排壳（计划）
└───────────────────────────────────────────────────┘
        │
        ├── utils/presence · utils/motion · utils/keyed_overlay
        ├── app/portal_overlay (Esc LIFO, z-index)
        └── utils/popup (mark_floating, sync, mount_*)
        │
        ▼
primitives/portal · primitives/overlay · layout hoist
```

**原则**：壳 = **编排**；popup = **几何**；components = **外观 + 语义**。

---

## 3. `utils/anchored_overlay.rs`

### 3.1 职责

非模态浮层的一站式壳：

- `portal_sync` / `portal_sync_presence`（`OverlayMode::Stacked`）
- `reactive_anchor` + 0×0 占位（或等价 view id）
- 浮层体 `isolate_reads`（**禁止** `untrack` 包整棵面板）
- `use_floating_presence` + `apply_floating_panel_motion`（委托 internal，逐步升为 `pub` 或迁入壳模块）
- 按定位模式调用 `mark_floating` / `mark_floating_at_point` / 无 `mark_floating`
- `apply_stored_floating_place` + `mount_anchored_overlay`（或 unanchored 时 `panel_block_clicks`）

对应 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §4.4 第一行：**锚定浮层**。

### 3.2 三种定位模式（同一壳）

「锚定浮层」指 **portal 浮层 + 自动/半自动定位**，并不要求绑在 trigger 上。

| 模式 | 枚举（草案） | 定位来源 | 典型场景 |
|------|--------------|----------|----------|
| **Trigger** | `Placement::Trigger { focus_id, side, align, gap }` | trigger 的 layout 矩形 + `compute_anchored_placement` | Select、Dropdown、有锚 Popover、Tooltip |
| **Point** | `Placement::Point { x, y, gap }` | 视口坐标 + `compute_cursor_menu_placement` | ContextMenu、**随意放置面板**、拖拽后更新 `(x,y)` |
| **Unanchored** | `Placement::Unanchored` | 不 `mark_floating`；面板自管 `fixed` / `inset` / flex | 无锚 Popover、布局驱动（居中条、贴角 HUD） |

**随意放置**走 **Point** 或 **Unanchored**，不走视口模态。

- **Point**：坐标是唯一真相；拖拽 = 更新 `Signal<[f32; 2]>` 后重建或刷新 `ATTR_FLOATING_POINT`。  
- **Unanchored**：不用 `sync_floating_placement` 写 xy；适合百分比 / flex 居中。  
- ContextMenu 先例：点定位后可 `remove(ATTR_FLOATING_TRIGGER)`，避免误判为「点在 trigger 上」。

可选第四态（后续）：`Placement::Manual` — `sync` 在 `strategy=manual` 时跳过 xy 重算，只读 `{id}__floating_place` 信号（拖拽按住期间减少与 placement 引擎打架）。**首版可不做**，Point 模式更新坐标即可。

### 3.3 草案 API（Builder · 真源）

Builder 为**行为真源**（[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §3.1）；RSX 标签展开为同一 `fn` / `*Opts`（见 [§8.5 双书写面](#85-双书写面rsx--builder)）。**不进 prelude**；显式 `use rui::utils::anchored_overlay`。

```rust
use rui::app::portal_overlay::{OverlayKind, OverlayMode};
use rui::utils::anchored_overlay::{anchored_overlay, Placement, FloatingMotionKind};
use rui::utils::popup::PopupPlacement;

// Trigger 模式
anchored_overlay("color-picker")
    .open(open)
    .placement(Placement::Trigger {
        focus_id: "input:color".into(),
        placement: PopupPlacement::BottomStart,
        gap: 4.0,
        strategy: FloatingStrategy::Auto, // 或 Fixed（钉在 trigger 侧）
    })
    .kind(OverlayKind::Popover)
    .motion(FloatingMotionKind::Fade)
    .child(|| isolate_reads(|| build_panel()))
    .into()

// Point 模式 — 随意放置 / 可拖拽改坐标
let pos = use_signal("palette-pos", || [120.0_f32, 80.0]);
anchored_overlay("palette")
    .open(open)
    .placement(Placement::Point {
        x: pos.get()[0],
        y: pos.get()[1],
        gap: 0.0,
    })
    .kind(OverlayKind::Popover)
    .child(|| build_palette())
    .into()

// Unanchored 模式
anchored_overlay("hud")
    .open(open)
    .placement(Placement::Unanchored)
    .kind(OverlayKind::Popover)
    .child(|| panel.fixed().inset("1rem", "auto", "auto", "1rem").child(...))
    .into()
```

壳内部固定顺序（作者不得打乱）：

1. `use_floating_presence(id, motion)`  
2. `reactive_anchor(view_id, \|\| { ... })`  
3. `floating_portal_phase(id, open, presence, kind, mode)` → `None` 则返回 0×0  
4. `isolate_reads(\|\| child())` 构建 panel  
5. 按 `Placement` 打 `mark_floating*` / 或跳过  
6. `apply_stored_floating_place`（Trigger / Point）  
7. `apply_floating_panel_motion(...)`  
8. `mount_anchored_overlay(panel, oz.panel)` 或 unanchored 时 `panel_block_clicks(panel, oz.panel)`  
9. 外包 `reactive_placeholder_sized(view_id, 0, 0).pointer_events_box_none()`

### 3.4 与现有组件映射

| 组件 | 应用 `Placement` |
|------|------------------|
| Select / Combobox / DatePicker / ColorPicker / Tooltip | Trigger |
| Dropdown / Menubar / ContextMenu | Trigger 或 Point |
| Popover（有锚） | Trigger + 可选 `strategy=fixed` |
| Popover（无锚） | Unanchored |
| 自定义调色板 / 浮动工具窗 | Point 或 Unanchored |
| Toast | **不用本壳**（`OverlayMode::Pinned` + keyed host） |

### 3.5 从 `internal` 提升的内容

实施时从下列模块 **复制编排逻辑进壳** 或 **改为 `pub(crate)` → `pub`**（择一，避免双实现）：

| 符号 | 现位置 |
|------|--------|
| `floating_portal_phase` | `internal/floating_motion.rs` |
| `use_floating_presence` | 同上 |
| `apply_floating_panel_motion` | 同上 |
| `FloatingMotionKind` | 同上 |

`sync_floating_placement` 仍由 `app/state.rs` 每帧调用，**不**搬进壳。

---

## 4. `utils/viewport_modal.rs`

### 4.1 职责

视口级**模态**浮层壳：

- `portal_sync(Modal)` / `portal_sync_presence`
- `build_modal_overlay`（underlay：色、opacity、blur、点击关闭）
- 用户提供 **stage 内容**（居中 Dialog、贴边 Sheet 等）；壳负责 `mount_viewport_portal_stage` + `mount_modal_layer`
- `apply_modal_shell_motion` + hoist 后 `flush_modal_shell_motion_subtree`（runtime 已接）

对应 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §4.4 第二行：**视口模态**。

### 4.2 不是什么

- 全屏 underlay + **自由 (x,y) 浮窗** → 用 `anchored_overlay` Point，**不用** viewport_modal。  
- CommandOverlay 的 top-1/3 chrome → 仍是 viewport 舞台内的**产品布局**，壳只提供舞台 + underlay。

### 4.3 草案 API（Builder · 真源）

Builder 为真源；RSX 对位见 [§8.5](#85-双书写面rsx--builder)。

```rust
use rui::utils::viewport_modal::{viewport_modal, ModalSurfaceMotion, ShellOverlayOpts};

viewport_modal("settings")
    .open(open)
    .overlay(ShellOverlayOpts {
        opacity: Some(0.25),
        backdrop_blur: Some(2.0),
        close_on_click: true,
        ..Default::default()
    })
    .motion(ModalSurfaceMotion::DialogFade) // 或 SheetSlide { edge, ... }
    .stage(|panel_z| {
        // 产品层：居中卡片 / 贴边 sheet；舞台内只用 swallow_clicks，禁止 panel_block_clicks
        vstack()
            .items_center()
            .justify_center()
            .inset(0.0)
            .w("100%")
            .h("100%")
            .child(swallow_clicks(dialog_card))
            .into()
    })
    .into()
```

壳输出：`mount_modal_layer(underlay, mount_viewport_portal_stage(stage, panel_z))`。

### 4.4 从 `internal` 提升的内容

| 符号 | 现位置 |
|------|--------|
| `modal_sync` | `internal/modal_shell.rs` |
| `build_modal_overlay` | 同上 |
| `ShellOverlayOpts`、`DEFAULT_MODAL_OVERLAY_*` | 同上 |
| `apply_modal_shell_motion` | `internal/modal_motion.rs` |
| `mounted_signal` / `ModalPresenceHandle` | 同上 |

Dialog / Sheet / CommandDialog 迁移后应只剩：**语义 Context、触发器、面板 chrome（圆角/令牌）、stage 内布局**。

---

## 5. 横切契约（两壳共用）

### 5.1 Esc 与叠层

- 一律 `OverlayMode::Stacked`（Toast 等 Pinned 除外）。  
- `OverlayKind` 必须显式传入（Menu / Popover / Tooltip / Modal），互斥规则见 [`portal_overlay.rs`](../crates/rui/src/app/portal_overlay.rs)。  
- Esc 只关顶层：[keyboard_nav.md](keyboard_nav.md)。

### 5.2 跟踪纪律（WP-3.14）

| 要做 | 不要做 |
|------|--------|
| 浮层体 `isolate_reads` | `untrack` 包整棵面板（锚点不注册 → 空壳） |
| `#[component]` 或 `component_root` 包复合根 | 同父连续 `provide_context` 不包根 |

### 5.3 禁区（WP-3.4）

- 第二套 `left/top` 坐标 sync。  
- 视口舞台内再 `panel_block_clicks` 包 `portal_panel`（嵌套 hoist）。  
- 关着仍把菜单留在文档流里占布局。  
- 用裸 `<Portal>` 替代 Esc 栈（简单展示除外）。

### 5.4 与 WP-PRIM 原语的关系

| 原语 | 用法 |
|------|------|
| [`presence`](crates/rui/src/utils/presence.rs) | 壳内建；退场时 `open=false` 仍 `mounted` 直到动画结束 |
| [`motion`](crates/rui/src/utils/motion.rs) | 壳写目标；mid 由 `transition_snapshot` 推进 |
| [`keyed_overlay`](crates/rui/src/utils/keyed_overlay.rs) | 多实例列表浮层（Toast）与 anchored 壳正交；可组合 `item_template_id` |
| [`pointer_drag`](crates/rui/src/utils/pointer_drag.rs) | Point 模式拖拽时更新坐标；可扩展 `DragKind::FloatingPanel`（产品层或壳内 hook） |

---

## 6. 选型速查

| 需求 | 选用 |
|------|------|
| 跟按钮走的菜单 / 选择器 | `anchored_overlay` · **Trigger** |
| 右键菜单、屏幕任意 (x,y)、拖拽改位置 | `anchored_overlay` · **Point** |
| 无坐标、flex 居中 / 贴角 HUD | `anchored_overlay` · **Unanchored** |
| 模态对话框 / 侧滑 Sheet / 全屏遮罩 | `viewport_modal` |
| 仅树外挂载、无 Esc 栈 | [`primitives/Portal`](../crates/rui/src/primitives/portal.rs) |
| 通知队列、pinned 宿主 | Toast host（`OverlayMode::Pinned`） |
| **仅全屏变暗 / 点击关闭，无面板** | [`primitives/overlay`](#7-遮罩scrim--underlay) 或 `viewport_modal` 的 underlay 部分 |
| **卡片 / 容器内局部遮罩** | [`Portal` mount](#73-独立容器内遮罩portal-mount--绝对铺满) + 绝对定位 `Box` |

---

## 7. 遮罩（Scrim / Underlay）

### 7.1 结论：**不新建产品级「遮罩组件」**

遮罩在栈里已是 **`primitives/overlay`** 无头原语，**不需要**再做一个 `<Scrim>` / `<Mask>` 进 `components/` 或 prelude。

| 层级 | API | 用途 |
|------|-----|------|
| **原语（已有）** | [`primitives/overlay`](../crates/rui/src/primitives/overlay.rs) · `overlay(z)` | 全屏 underlay：`color` / `opacity` / `backdrop_blur` / `close_on_click`；内部 `portal_panel` 提升到根叠层 |
| **便捷封装（已有）** | [`utils/popup::underlay_close_on_click`](../crates/rui/src/utils/popup.rs) | 模态点击关闭；委托 `overlay` |
| **模态默认（internal）** | `build_modal_overlay` · [`modal_shell`](../crates/rui/src/components/internal/modal_shell.rs) | Dialog/Sheet 默认 alpha / blur；**WP-OVL.1** 随 `viewport_modal` 公开为 `ShellOverlayOpts` |
| **产品样式** | Dialog / Sheet / AlertDialog | 在壳之上填语义色、variant scrim（[`semantic_tone`](../crates/rui/src/style/semantic_tone.rs)） |

**原则**：遮罩 = **无头原语 + 可选 utils 封装**；带圆角、variant、文案的仍是产品组件，不是第二层 Mask 组件。

### 7.2 全屏遮罩：直接用 `overlay`

需要**整窗变暗 + 可选点击关闭**（不一定有居中面板）时：

```rust
use rui::primitives::overlay::overlay;
use rui::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};

let open = use_signal("dimmer-open", || true);
// 若要与 Dialog 等同栈互斥，登记 Modal；仅展示层可省略 portal_sync，但 z 需自管
let oz = portal_sync("dimmer", open, OverlayKind::Modal, OverlayMode::Stacked);

let underlay: Element = overlay(oz.map(|z| z.underlay).unwrap_or(1000))
    .color(tokens::background()) // 或 parse_color("background")
    .opacity(0.4)
    .backdrop_blur(4.0)
    .close_on_click(open)        // 不需要点击关闭则省略，用 swallow_clicks 包 Box
    .into();
```

要点：

- `overlay` **默认无设计令牌色**（`color` alpha 为 0），调用方必须 `.color()` / `.opacity()`。  
- 点击关闭走 **underlay arm/disarm**（`disarm_underlay_close` / `arm_underlay_close`），与打开手势防连点；见 [`popup.rs`](../crates/rui/src/utils/popup.rs)。  
- 嵌套 Menu/Popover 在栈顶时，underlay 先 `try_dismiss_outside_press()`，再关自己的 `open`（库测 `overlay_prefers_nested_menu_dismiss_before_closing_modal`）。  
- **完整模态**（underlay + 舞台 + 面板）仍用 [`viewport_modal`](#4-utilsviewport_modalrs)，不要手拼第二套 underlay 逻辑。

### 7.3 独立容器内遮罩：`<Portal mount="…">` + 绝对铺满

**`overlay()` 面向根 portal 全屏 underlay**，不会自动裁进某个卡片。要在**子区域**上叠暗色：

```rust
// 宿主：relative + 稳定 template_id
ui! {
    <Box id="card-host" relative=true w="20rem" h="12rem">
        {card_content()}
    </Box>
    <Portal id="card-scrim" mount="card-host">
        <Show when={busy.get()}>
            <Box absolute=true inset="0" w="100%" h="100%"
                 bg="black" opacity="0.35" items="center" justify="center">
                <Spinner />
            </Box>
        </Show>
    </Portal>
}
```

| 场景 | 做法 |
|------|------|
| 整窗模态 dimmer | `overlay(z)` + `portal_sync(Modal)` |
| 卡片 / 侧栏 / 地图控件内 loading | `Portal mount=宿主id` + `absolute inset=0` |
| 模态对话框 | `viewport_modal`（underlay 已含） |
| 非模态 Popover / Menu | **无 underlay**；仅 `panel_block_clicks` |

局部遮罩**不进** Esc 栈亦可；若要与全局 Modal 叠层，需单独评估 `portal_sync` 与 z。

### 7.4 与两个浮层壳的分工

| 壳 | 默认遮罩 |
|----|----------|
| `viewport_modal` | **有** — `build_modal_overlay` / `ShellOverlayOpts` |
| `anchored_overlay` | **无** — 非模态；外点关闭靠 `try_dismiss_outside_press` |

若要在锚定浮层下加 dimmer（少见）：自行 `mount_modal_layer(overlay(...), stage)` 会混入模态语义；更干净的做法是 **先 `viewport_modal` 再嵌 anchored**，或产品层明确「抽屉 + 全屏 scrim」用 `viewport_modal` 的 stage 布局。

### 7.5 WP-OVL 对遮罩的交付

- **不新增** `components/scrim.rs`。  
- `viewport_modal` 公开 `ShellOverlayOpts`（从 `modal_shell` 提升）。  
- 在本文 + [`headless.md`](headless.md) 写清：**全屏 → `overlay`；局部 → `Portal mount`**。

---

## 8. 自定义锚定浮层与视口模态

### 8.1 定制边界（壳负责 vs 作者负责）

| 作者定制 | 壳 / 框架负责 |
|----------|----------------|
| 面板 UI（`child` / `stage` 闭包）：布局、令牌、表单 | `portal_sync`、z-index、Esc LIFO |
| `Placement`（Trigger / Point / Unanchored） | `reactive_anchor`、0×0 占位、`isolate_reads` |
| `OverlayKind`（Menu / Popover / Tooltip） | `sync_floating_placement`（Trigger / Point） |
| `FloatingMotionKind` / `ModalSurfaceMotion` | presence 退场、`apply_*_motion`、hoist 后 flush |
| 模态 `ShellOverlayOpts`（色、opacity、blur、点遮罩关闭） | `mount_viewport_portal_stage`、`mount_modal_layer` |
| stage 内居中 / 贴边 / Command top-1/3 布局 | underlay 嵌套 dismiss 顺序 |

壳**不写** `bg-popover`、`rounded-md` 等产品 chrome。

### 8.2 自定义锚定浮层

#### 落地前（过渡期手写配方）

与 [custom_components.md](custom_components.md) §复合控件配方一致：

```rust
use rui::core::reactivity::isolate_reads;
use rui::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use rui::app::portal_overlay::{portal_sync_presence, OverlayKind, OverlayMode};
use rui::utils::popup::{
    apply_stored_floating_place, mark_floating_at_point, mount_anchored_overlay,
    PopupPlacement, mark_floating,
};
// floating_motion 仍在 internal，复制 Popover/Dropdown 片段

let open = use_signal("my-float-open", || false);
let pos = use_signal("my-float-pos", || [200.0_f32, 120.0]);
let id = "my-float";
let view_id = format!("{id}__view");
let presence = use_floating_presence(id, FloatingMotionKind::Fade);

reactive_placeholder_sized(view_id.clone(), 0.0, 0.0).pointer_events_box_none().child(
    reactive_anchor(view_id, move || {
        let mounted = /* presence mounted signal */;
        let Some(oz) = portal_sync_presence(id, open, mounted, OverlayKind::Popover, OverlayMode::Stacked) else {
            return Element::new().w(0.0).h(0.0);
        };
        let mut panel = isolate_reads(|| build_my_panel());
        mark_floating_at_point(&mut panel, id, "", pos.get()[0], pos.get()[1], 0.0);
        panel.attrs.remove(ATTR_FLOATING_TRIGGER);
        apply_stored_floating_place(&mut panel, id);
        apply_floating_panel_motion(/* ... */);
        mount_anchored_overlay(panel, oz.panel)
    }),
)
```

#### 落地后（`anchored_overlay` Builder）

```rust
anchored_overlay("my-float")
    .open(open)
    .placement(Placement::Point { x: pos.get()[0], y: pos.get()[1], gap: 0.0 })
    .kind(OverlayKind::Popover)
    .motion(FloatingMotionKind::Fade)
    .on_panel(|_| {
        vstack()
            .p("1rem")
            .bg(tokens::popover())
            .rounded("0.5rem")
            .child(/* 任意内容 */)
            .into()
    })
    .into()
```

**扩展点（草案）**：

| 方法 | 作用 |
|------|------|
| `.placement(...)` | Trigger / Point / Unanchored |
| `.kind(OverlayKind::…)` | Esc 互斥 |
| `.motion(...)` | Fade / Expand |
| `.match_trigger_width(bool)` | 等同 `ATTR_FLOATING_MATCH_TRIGGER` |
| `.max_h("24rem")` | 视口钳位后的滚动上限 |
| `.strategy(FloatingStrategy::Fixed)` | 钉在 trigger 侧、不翻转 |
| `.child(fn)` | 面板；壳内包 `isolate_reads` |

### 8.3 自定义视口模态

#### 落地前

复用 Dialog 路径：`modal_sync` → `build_modal_overlay` → 自研 `stage` → `mount_viewport_portal_stage` → `mount_modal_layer`；stage 内子节点 **`swallow_clicks` only**。

#### 落地后

```rust
viewport_modal("confirm")
    .open(open)
    .overlay(ShellOverlayOpts {
        color: Some(parse_color("background")),
        opacity: Some(0.35),
        backdrop_blur: Some(8.0),
        close_on_click: true,
    })
    .motion(ModalSurfaceMotion::DialogFade)
    .stage(|_panel_z| {
        // 完全自定义：侧滑、全屏表单、无标题确认框……
        hstack()
            .w("100%")
            .h("100%")
            .justify_end() // 例如 Sheet 贴右
            .child(swallow_clicks(sheet_panel))
            .into()
    })
    .into()
```

**扩展点（草案）**：

| 方法 | 作用 |
|------|------|
| `.overlay(ShellOverlayOpts)` | 遮罩外观与点击关闭 |
| `.motion(ModalSurfaceMotion::…)` | DialogFade / SheetSlide { edge, … } |
| `.stage(fn)` | 视口内布局；接收 `panel_z` 供子 portal 嵌套（罕见） |
| `.presence(...)` | 受控退场（默认内建） |

AlertDialog、CommandDialog 等 = **`viewport_modal` + 产品 Context + 固定 stage 布局**。

### 8.4 自定义时仍须遵守

- 浮层体 **`isolate_reads`**，禁止 `untrack` 包整棵面板。  
- 模态 stage **禁止** `panel_block_clicks` 再包 `portal_panel`。  
- 单一 `open` 写入点；Esc / 外点关闭走 `portal_overlay`。  
- 多实例用稳定 `id` + Owner / 受控 Signal，避免全局 `use_signal("open")` 撞键。  
- **双书写面**：RSX 与 Builder 语义等价（[§8.5](#85-双书写面rsx--builder)）。

### 8.5 双书写面（RSX + Builder）

自定义浮层 / 模态壳与 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) **§3 双书写面（ADR-9）** 同等约束：**Builder 与 `ui!` 标签必须语义等价、可互嵌**，不得只文档化 Builder。

#### 8.5.1 硬性契约

| # | 契约 |
|---|------|
| 1 | **同一语义、同一 `Element`**：RSX 展开为 `anchored_overlay_content(opts, \|\| …)` / `viewport_modal_stage(opts, \|\| …)`，与 Builder `.child()` / `.stage()` 调用同一实现。 |
| 2 | **`*Opts` 为共享真源**：`AnchoredOverlayContentOpts`、`ViewportModalOpts`（名称草案）同时服务 RSX 属性解析与 Builder 链式方法。 |
| 3 | **camelCase 别名**：`openSignal`、`sideOffset`、`closeOnClick`、`backdropBlur` 等映射 snake_case（与 Popover / Dialog 一致）。 |
| 4 | **可互嵌**：`ui! { <AnchoredOverlayContent>…</AnchoredOverlayContent> }` 与 `{ Element::from(anchored_overlay("x").child(\|\| …)) }` 可出现在同一棵树。 |
| 5 | **不进 prelude**：双写面 ≠ 默认导入；仍 `use rui::utils::anchored_overlay` / `viewport_modal`，但 **RSX 标签必须在 `ui!` 中可用**（`rui_macros` 注册）。 |
| 6 | **库测**：每种壳至少一对「Builder 构建」与「Opts + content fn」断言同一 `template_id` / portal 标记。 |

#### 8.5.2 对位现有产品组件（Popover / Dialog）

| 产品模式 | 无头壳对位 |
|----------|------------|
| `popover_scope` + `<Popover>` / `popover().child()` | `anchored_overlay_scope` + `<AnchoredOverlay>` / `anchored_overlay().child()` |
| `popover_content(PopoverContentOpts, body)` + `<PopoverContent>` | `anchored_overlay_content(AnchoredOverlayContentOpts, body)` + `<AnchoredOverlayContent>` |
| Dialog 复合根 + `DialogContent` | `viewport_modal_scope` + `<ViewportModal>` + `<ViewportModalContent>`（或 `<ViewportModalStage>`） |

**复合根**（可选 `open` 受控、稳定 `id`）用 `*_scope`；**实际 portal 面板**用 `*_content` / `*_stage`，与 Popover 的 Trigger/Content 分离一致。

#### 8.5.3 RSX 示例（草案）

**锚定浮层 — Trigger**

```rust
ui! {
    <AnchoredOverlay id="picker" openSignal={open}>
        <Button focus_id="input:color">"选色"</Button>
        <AnchoredOverlayContent
            side="bottom"
            align="start"
            sideOffset={4}
            kind="popover"
        >
            {build_swatch_grid()}
        </AnchoredOverlayContent>
    </AnchoredOverlay>
}
```

**锚定浮层 — Point（随意放置）**

```rust
ui! {
    <AnchoredOverlay id="palette" openSignal={open}>
        <AnchoredOverlayContent
            placement="point"
            x={pos.get()[0]}
            y={pos.get()[1]}
            kind="popover"
        >
            {build_palette()}
        </AnchoredOverlayContent>
    </AnchoredOverlay>
}
```

**视口模态**

```rust
ui! {
    <ViewportModal id="settings" openSignal={open} overlayOpacity={0.25} closeOnClick={true}>
        <ViewportModalContent motion="dialog-fade">
            <VStack p="1.5rem" bg="card" rounded="0.5rem" max_w="32rem">
                {settings_form()}
            </VStack>
        </ViewportModalContent>
    </ViewportModal>
}
```

RSX 属性 `side` / `align` / `placement="point"` 在宏内译为 `Placement::Trigger { … }` 或 `Placement::Point { … }`（复用 [`popover_side_align_to_placement`](../crates/rui/src/components/popover.rs) 或迁入 `utils/popup`）。

#### 8.5.4 Builder 对位示例

与 §8.5.3 **同一语义**的 Builder 写法：

```rust
anchored_overlay("picker")
    .open(open)
    .child(|| ui! {
        <Button focus_id="input:color">"选色"</Button>
        <AnchoredOverlayContent side="bottom" align="start" sideOffset={4}>
            {build_swatch_grid()}
        </AnchoredOverlayContent>
    })

// 或纯 Builder（无标签）
anchored_overlay("palette")
    .open(open)
    .child(|| {
        Element::from(
            anchored_overlay_content(
                AnchoredOverlayContentOpts::default()
                    .placement(Placement::Point { x: pos.get()[0], y: pos.get()[1], gap: 0.0 })
                    .kind(OverlayKind::Popover),
                || build_palette(),
            ),
        )
    })
```

```rust
viewport_modal("settings")
    .open(open)
    .overlay_opacity(0.25)
    .close_on_click(true)
    .child(|| ui! {
        <ViewportModalContent motion="dialog-fade">
            {settings_card()}
        </ViewportModalContent>
    })
```

#### 8.5.5 `rui_macros` 交付（WP-OVL 必含）

| 任务 | 说明 |
|------|------|
| 注册标签 | `AnchoredOverlay`、`AnchoredOverlayContent`、`ViewportModal`、`ViewportModalContent`（名称可评审，但须成对） |
| 展开目标 | `anchored_overlay_scope` / `anchored_overlay_content` / `viewport_modal_scope` / `viewport_modal_content` |
| 懒构建 body | Content 子树仅在 `open` 时构建（对齐 `popover_content`） |
| Gallery | 每个壳 **RSX 与 Builder 各一例**（COMPONENT_LIBRARY §3 第 6 条） |

#### 8.5.6 作者自定义复合组件

用户在自家 crate 包一层时，仍须双写：

```rust
#[component]
pub fn MyPalette(open: Signal<bool>, children: Option<Element>) -> Element {
    ui! {
        <AnchoredOverlay id="my-palette" openSignal={open}>
            <AnchoredOverlayContent placement="point" x={...} y={...}>
                {children}
            </AnchoredOverlayContent>
        </AnchoredOverlay>
    }
}

// 等价 Builder 包装
pub fn my_palette(open: Signal<bool>, body: impl FnOnce() -> Element) -> Element {
    anchored_overlay("my-palette").open(open).child(body)
}
```

**禁止**：仅暴露 `#[component]` 而无 Builder/`scope` fn；或 RSX 走私有分支产生不同 portal 行为。

---

## 9. 点定位与拖拽（solid-dnd 关联）

### 9.1 点定位（`Placement::Point`）

**语义**：面板锚在视口坐标 `(x, y)`，**不**需要 trigger 元素。打开时写入 `mark_floating_at_point`；每帧 `sync_floating_placement` 用 [`compute_cursor_menu_placement`](../crates/rui/src/utils/popup.rs) 做翻转与视口钳位。

| 字段 | 说明 |
|------|------|
| `x`, `y` | 视口/client 空间原点（通常面板左上角展开方向为右下，与 ContextMenu 一致） |
| `gap` | 相对点的偏移；`0` 常见 |
| `trigger_focus_id` | 可传 `""` 并 `remove(ATTR_FLOATING_TRIGGER)`，避免外点判定当成「点在 trigger 上」 |

**程序设位**（非拖拽）：

```rust
pos.set([x, y]); // open 为 true 时 reactive 重建 → mark_floating_at_point 读新坐标
```

**与 Unanchored 选型**：要参与 `sync` 的视口钳位、max_h → **Point**；纯 flex 居中 / 百分比 → **Unanchored**。

### 9.2 拖拽改位（浮动面板）

今日**没有** `DragKind::FloatingPanel`；用 [`pointer_drag`](../crates/rui/src/utils/pointer_drag.rs) 自管会话，**更新 `pos` Signal** 为唯一坐标真相。

```rust
// 标题栏按下
.on_pointer_down(move |e| {
    pointer_drag::begin(DragKind::/* 待扩展 FloatingPanel */, id, e.x, e.y);
})
// app 指针路由 / window_events 中
if let Some(sess) = pointer_drag::tick(x, y) {
    if sess.kind == /* FloatingPanel */ && sess.id == id {
        let [px, py] = pos.peek();
        let (dx, dy) = pointer_drag::dx_dy(&sess);
        pos.set([px + dx, py + dy]);
        pointer_drag::begin(/* 重置 start 为当前指针，避免累加漂移 */ ...);
    }
}
.on_pointer_up(|_| { pointer_drag::clear(); })
```

**与 placement 引擎**：

- 每帧 `sync` 根据 `ATTR_FLOATING_POINT` **重算** xy；拖拽时须每帧让 `mark_floating_at_point` 读到新 `pos`（重建或改 attr）。  
- **WP-OVL 可选**：`Placement::Manual` — 拖拽期间 `sync` 跳过 xy，只读 `{id}__floating_place`；减少与 `compute_cursor_menu_placement` 争抢。首版 Point + 更新坐标即可。

**性能**：优先改 **placement 坐标**，少动画 `left/top`（见 [bugs.md](bugs.md) §9 与 `transform` 建议）；若壳支持，拖拽中可用 `translate` 做 preview，松手再提交 `pos`（产品层策略）。

### 9.3 与 [bugs.md](bugs.md)「solid-dnd」愿望的关系

[`bugs.md`](bugs.md) Capabilities 列出：

> solid-dnd/sortablejs 拖拽、拖拽排序及切换父级、**拖拽时的浮层**、碰撞检测等

这是**独立能力 backlog**（AUTHORITY §7），**不是** `anchored_overlay` 首版范围。拆分如下：

| 能力 | 现状 | 与本文关系 |
|------|------|------------|
| 列表重排 | [`utils/drag_reorder`](../crates/rui/src/utils/drag_reorder.rs) + app 指针路由 | 与浮层壳正交 |
| 指针会话 | [`utils/pointer_drag`](../crates/rui/src/utils/pointer_drag.rs)（Slider/Toast/Carousel…） | Point 浮层拖拽可扩展 `DragKind::FloatingPanel` |
| **拖拽替身浮层**（sortable 的 drag ghost） | **未做** | solid-dnd 典型需求：拖起时 `anchored_overlay` **Point** 跟指针，或 `OverlayMode::Pinned` + `translate`；需碰撞检测与 list 索引 |
| 跨容器拖放 / 改父级 | **未做** | 超出 overlay_shells；归 solid-dnd WP |
| 浮层入退场 | `floating_motion` / `modal_motion` 已有；产品化加深见 bugs | 壳统一 `motion` 参数 |

**推荐演进**（文档化，非承诺排期）：

1. **WP-OVL.3**：Gallery Point 面板 + 标题栏拖拽改 `pos`（验证壳）。  
2. **WP-DND.1**（建议）：`pointer_drag::DragKind::FloatingPanel` + `anchored_overlay::drag_handle(...)` 辅助。  
3. **WP-DND.2**：sortable ghost — `drag_reorder` 提交前用 Point 浮层显示替身，落点碰撞检测。  
4. 参考生态：Solid [solid-dnd](https://github.com/thisbeyond/solid-dnd) / SortableJS 的 **drag overlay layer** ≈ 本框架 **Point `anchored_overlay` + Pinned z**。

### 9.4 点定位 + 拖拽检查清单

- [ ] 坐标存 `Signal`，不手写第二套 `left/top` sync  
- [ ] `remove(ATTR_FLOATING_TRIGGER)`（无 trigger 时）  
- [ ] 拖拽更新 `pos` 后 `mark_floating_at_point` 同步  
- [ ] `portal_sync` + `mount_anchored_overlay`；非模态不加全屏 `overlay`  
- [ ] `pointer_drag` 释放时 `clear()`  
- [ ] solid-dnd 级 sortable ghost / 改父级：**另开 WP**，不阻塞 WP-OVL

---

## 10. 实施计划（建议顺序）

| 阶段 | 工作 | 验收 |
|------|------|------|
| **WP-OVL.1** | 新增 `utils/viewport_modal.rs` + RSX 标签（`ViewportModal*`）+ `rui_macros` 展开 | Dialog 库测不回归；Gallery Builder + RSX 各一例 |
| **WP-OVL.2** | 新增 `utils/anchored_overlay.rs` + RSX 标签（`AnchoredOverlay*`） | Popover 抽 1 调用点；`mount_anchored_overlay_is_zero_anchor` 仍绿 |
| **WP-OVL.3** | Gallery：Point 浮层 + 标题栏拖拽；**RSX 与 Builder 双示例** | < 500 行；见 [§9](#9-点定位与拖拽solid-dnd-关联) |
| **WP-OVL.4** | 存量 Dialog/Popover 薄包装迁移；文档链入 [§8.5](#85-双书写面rsx--builder) | 新 PR 禁止手抄 portal 配方 |
| **WP-DND.***（建议，非 OVL 阻塞） | `DragKind::FloatingPanel`、sortable drag ghost、碰撞检测 | 见 [bugs.md](bugs.md) solid-dnd；[§9.3](#93-与-bugsmdsolid-dnd愿望的关系) |

**不纳入 WP-OVL 首版**：prelude 导出、`Placement::Manual`、焦点陷阱产品化（仍归 C-WP-1.3）、完整 solid-dnd 栈。  
**纳入 WP-OVL 首版（原「不纳入」已撤销）**：`AnchoredOverlay` / `ViewportModal` RSX 标签与 `*Opts` 双写（ADR-9）。

---

## 11. 迁移检查清单（PR 用）

- [ ] 浮层走 `anchored_overlay` 或 `viewport_modal`，未手拼 `portal_sync` + `mount_*`  
- [ ] **RSX 与 Builder 同一 `*Opts` / content fn**（[§8.5](#85-双书写面rsx--builder)）；Gallery 或库测各覆盖一种写法  
- [ ] 子树 `isolate_reads`，无 `untrack` 包面板  
- [ ] Trigger / Point 未混用第二套坐标  
- [ ] 模态 stage 内仅 `swallow_clicks`  
- [ ] `OverlayKind` 与产品语义一致  
- [ ] 库测：`portal_overlay` Esc 栈、popup 锚点测仍绿  
- [ ] 未向 prelude 添加导出  

---

## 12. 相关链接

- 遮罩原语：[`primitives/overlay.rs`](../crates/rui/src/primitives/overlay.rs)  
- 浮层单一路径（已签）：[AUTHORITY.md](AUTHORITY.md) WP-3.4  
- 作者四条红线：[custom_components.md](custom_components.md) §复合控件配方  
- 双书写面权威：[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §3（ADR-9）  
- 键盘 / Esc：[keyboard_nav.md](keyboard_nav.md)  
- 动画分工：[animation_and_motion.md](animation_and_motion.md)  
- solid-dnd / 拖拽 backlog：[bugs.md](bugs.md) Capabilities  
- 战略背景：[custom_components_suggestions.md](custom_components_suggestions.md)
