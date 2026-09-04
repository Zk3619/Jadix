# 组件分层与代码体量（RUI × gpui-component）

> **文档地位**：组件库**架构审计**（非巩固期执行源）。分层职责以 [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §2 为准；原语缺口与作者面见 [custom_components_suggestions.md](custom_components_suggestions.md)；GPUI 生态总表见 [GPUI_ECOSYSTEM_COMPARISON.md](../comparison/GPUI_ECOSYSTEM_COMPARISON.md)。  
> **事实来源**：`Reference/gpui-component-main` · `crates/rui/src/components/` · `crates/rui/src/utils/`（2026-09-02 快照）。  
> **日期**：2026-09-02；**2026-09-02 晚** 增补 §1.6 / §8 未拆分单文件组件逐项对照

---

## 0. 一句话结论

gpui-component 单文件「极其精简」，主要来自 **框架替你承担 layout/paint/motion，且把厚逻辑摊到 `gpui-base` + `motion/`**——不是同等功能下我们真的多写了五倍无用代码。

RUI 的出路不是盲目追行数，而是：

> **同等 ShadCN 产品语义下，组件文件 = 薄壳（目标 ≤400 行）+ 共享原语（`utils/` / `primitives/`）；禁止在 `accordion` / `collapsible` 等处复制同一套 panel motion。**

---

## 1. 体量对照（可复现）

### 1.1 Collapsible（最典型）

| 项目 | 路径 | 行数 |
|------|------|------|
| gpui-base | `crates/base/src/collapsible.rs` | **95** |
| gpui-component 样式层 | `crates/ui/src/collapsible.rs` | **126** |
| **gpui 合计** | | **~221** |
| RUI | `crates/rui/src/components/collapsible.rs` | **1108**（测试 ~123，产品 ~985） |

gpui 的折叠动画**不在**上述 221 行内，而在共享模块：

| 模块 | 路径 | 行数 |
|------|------|------|
| motion 总线 | `gpui-base/src/motion.rs` | **~1364** |
| MotionReveal | `gpui-base/src/motion/reveal.rs` | **~120** |

所有需要高度展开/收起的组件**摊销**这套基础设施。

### 1.2 Accordion

| 项目 | 行数 |
|------|------|
| gpui-base `accordion.rs` | **428** |
| gpui-component `accordion.rs` | **385** |
| RUI `accordion.rs` | **1233** |

RUI 的 `accordion` 与 `collapsible` **各有一套**几乎相同的 `*PanelPresence*` + `apply_*_panel_motion`（各 ~170 行），保守 **~350–400 行纯重复**。

### 1.3 Button（简单控件也有差距）

| 项目 | 行数 | 说明 |
|------|------|------|
| gpui-base `button.rs` | **496** | 仅交互/焦点/a11y **行为** |
| RUI `button.rs` | **1545** | 行为 + 全 variant 样式 + hover peek + fragment 拆包 |

### 1.4 生态总 LOC（避免「单文件短 = 简单」错觉）

| 项目 | Rust LOC（快照） |
|------|------------------|
| gpui-component 全仓 | **~275k** |
| GPUI 本体 | **~116k** |
| RUI `crates/` | **~189k** |

gpui-component **总代码量大于 GPUI**；「精简」是**摊销到 base + motion**，不是消除了复杂度。

### 1.6 未拆分单文件组件全景（ui ± base vs RUI 单文件/目录）

> **口径**：`ui` = `crates/ui/src/<name>.rs` 或 `crates/ui/src/<name>/` 目录合计；`base` = `crates/base/src/<name>.rs`；`RUI` = `crates/rui/src/components/` 对应路径。带 `/` 的为目录合计。比值 = RUI ÷ (ui+base)，仅当分母 > 0。

| 组件 | gpui ui | gpui base | gpui 合计 | RUI | RUI/gpui | 形态 |
|------|--------:|----------:|----------:|----:|---------:|------|
| **hover_card** | 135 | 289 | **424** | 609 | **1.4×** | ui 极薄 facade |
| **kbd** | 297 | — | **297** | 328 | **1.1×** | 仅 ui，无 base |
| **label** | 334 | — | **334** | 583 | **1.7×** | 仅 ui |
| **combobox** | 1444 | 218 | **1662** | 1283 | **0.8×** | ui **很厚** + 共享 list |
| **color_picker** | 567 | 880 | **1447** | 2352† | **1.6×** | base 重；RUI 已拆目录 |
| **collapsible** | 126 | 95 | **221** | 1108 | **5.0×** | 典型薄 ui + motion 摊销 |
| **accordion** | 385 | 428 | **813** | 1233 | **1.5×** | base 与 ui 双厚 |
| **select** | 804 | 352 | **1156** | 1283 | **1.1×** | 接近 |
| **popover** | 466 | 417 | **883** | 597 | **0.7×** | RUI 更短 |
| **tooltip** | 270 | 274 | **544** | 342 | **0.6×** | RUI 更短 |
| **dialog** | — | 630 | **630** | 626 | **1.0×** | 仅 base 行为 |
| **sheet** | 243 | 241 | **484** | 661 | **1.4×** | |
| **checkbox** | 447 | 635 | **1082** | 522 | **0.5×** | RUI 显著更短 |
| **switch** | 350 | 648 | **998** | 553 | **0.6×** | |
| **slider** | 332 | 750 | **1082** | 900 | **0.8×** | |
| **breadcrumb** | 155 | — | **155** | 688 | **4.4×** | 仅 ui 薄，RUI 复合重 |
| **badge** | 147 | — | **147** | 518 | **3.5×** | |
| **pagination** | 221 | 238 | **459** | 1626 | **3.5×** | RUI 含 jumper + 宽度自适应 |
| **progress** | — | 158 | **158** | 1051 | **6.6×** | RUI 含条形+环形复合 |
| **separator** | 140 | — | **140** | 83 | **0.6×** | RUI 更短 |
| **button** | 3015‡ | — | **3015** | 1545 | **0.5×** | gpui **目录** ui/button |
| **input** | 4359‡ | — | **4359** | 1026 | **0.2×** | gpui input/ 目录极大 |
| **table** | 3688‡ | — | **3688** | 931 | **0.3×** | gpui table/ 目录 |
| **tabs** | 1574‡ | 376 | **1950** | 1411 | **0.7×** | gpui tab/ 目录 |

† RUI `color_picker/` 目录（`mod` + `model` + `view` + `drag`）合计。  
‡ gpui-component 将 button / input / table / tab 等做成 **子目录**，单看 `combobox.rs` 会误判生态体量。

**读表结论**：

1. **并非 RUI 处处更胖**：input、button、table、checkbox、tooltip、combobox（合计）等 RUI **更短或接近**——gpui 把厚度藏在 **ui 子目录 + base + 共享模块**（`searchable_list`、`list`、`input`）。
2. **RUI 明显偏胖**集中在：折叠类（collapsible）、**展示复合件**（progress、pagination、badge、breadcrumb）、以及 **手写 motion/portal**（hover_card 亦略胖）。
3. **「gpui 单文件短」有三种真相**：真薄 facade（hover_card ui 135 行）、**假薄**（combobox ui 1444 行）、**假单文件**（input 目录 4359 行）。

### 1.7 批量复现命令

### 1.7 批量复现命令

```powershell
$gpuiUi = "Reference/gpui-component-main/crates/ui/src"
$gpuiBase = "Reference/gpui-component-main/crates/base/src"
$rui = "crates/rui/src/components"
function Get-Loc($p) {
  if (-not (Test-Path $p)) { return $null }
  if ((Get-Item $p).PSIsContainer) {
    ($null = Get-ChildItem $p -Recurse -Filter *.rs | Get-Content | Measure-Object -Line).Lines
  } else { (Get-Content $p | Measure-Object -Line).Lines }
}
# 示例：combobox
Get-Loc "$gpuiUi/combobox.rs"; Get-Loc "$gpuiBase/combobox.rs"; Get-Loc "$rui/combobox.rs"
```


### 1.8 早期复现命令（折叠 / motion）

```powershell
$files = @(
  "Reference/gpui-component-main/crates/base/src/collapsible.rs",
  "Reference/gpui-component-main/crates/ui/src/collapsible.rs",
  "crates/rui/src/components/collapsible.rs",
  "Reference/gpui-component-main/crates/base/src/motion.rs"
)
foreach ($f in $files) {
  $loc = (Get-Content $f | Measure-Object -Line).Lines
  Write-Output "$loc`t$f"
}
```

---

## 2. 生态地图：代码去哪了

```text
gpui-component 组件文件（~200 行）
        │  facade：.open() .content() .refine_style()
        ▼
gpui-base 行为层（按域共享：popup / virtual_list / accordion …）
        │  Collapsible::render → MotionReveal + spring(progress)
        ▼
GPUI 框架
        Element trait · request_layout · element state
        spring() · AccessKit · taffy
```

```text
RUI 组件文件（~1000+ 行，现状）
        │  RSX + Builder + Context + Signal effect
        │  apply_*_panel_motion（手写 LayoutTree + keyframes + damage）
        │  chevron / exclusive / slot 产品语义
        ▼
utils/presence.rs（148 行）— 已抽但未覆盖 collapse 动画
style/keyframes — CSS collapsible-down/up
```

---

## 3. 根因拆解（按贡献排序）

### 3.1 框架抽象层级不同（最大头）

**gpui-component** 写组件 ≈ 声明式组合 + `RenderOnce`：

- `open: bool` 传入，`spring()` 在 render 里算 `progress`
- `MotionReveal::new(id, progress, content)` 裁切高度
- 框架负责 layout 节点、element state、绘制

**RUI** 在组件内实现 **半个布局/动画运行时**，例如 `apply_collapse_panel_motion`（单函数 ~170 行）需亲手处理：

- `LayoutTree::update_taffy_node_style`
- `animation_snapshot` + `@keyframes collapsible-down/up`
- `push_frame_paint_damage`
- `PresencePhase` 进出场与 `measured_height` 两阶段测量

### 3.2 动画模型不同

| 轴 | gpui-component | RUI |
|----|----------------|-----|
| 折叠 | `progress ∈ [0,1]` × `MotionReveal` 裁切高度 | CSS keyframes + snapshot 逐帧 |
| 弹簧 | 框架级 `spring()` + element state | 自管 `needs_enter` / `needs_exit_anim` |
| 退场 | `motion/presence` 统一 | 每组件 `*PanelPresenceRegistry` + thread_local |

RUI 更贴近 **ShadCN CSS 动画**（`collapsible-down/up`），代价是每个带高度的组件重复「测高 → 播动画 → 退场卸载」状态机。

### 3.3 共享原语未完成下沉

[COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §2.1 已规划 `presence.rs` · `motion.rs` · `keyed_overlay.rs` 等，但现状：

| 原语 | 状态 | 缺口 |
|------|------|------|
| `utils/presence.rs` | 有 `ExitGate` / `PresencePhase` | collapse **未**接入 |
| `utils/motion.rs` | 部分 transition 助手 | 无 `HeightReveal` / progress 裁切 |
| collapse panel | **散落在** `collapsible.rs` + `accordion.rs` | 应合并为 `utils/collapse_panel.rs` |

[custom_components_suggestions.md](custom_components_suggestions.md) §1 所述「缺中间原语 → 组件内手搓」在 Collapsible/Accordion 上已实证。

### 3.4 双书写面 + 复合件叠在同一文件

RUI 单文件常同时承载：

- `collapsible_scope` / `CollapsibleTrigger` / `CollapsibleContent`（RSX）
- `CollapsibleBuilder` + `From<Builder>` + `IntoElementList`
- Context / `use_collapsible` / exclusive group
- chevron 自动插入、text leaf 检测

gpui-component 的 Collapsible **无** Trigger/Content 复合件——调用方 `div` + `.content()`，复合语义在应用层，组件文件自然短。RUI 对标 ShadCN **保留**复合 API，行数应计入 **产品面成本**，但实现应 **单路径**（宏 ≡ Builder，见 §5.3）。

### 3.5 反应性手工接线（Signal 栈的代价）

RUI 每控件显式：`create_context` · `use_signal` · `create_effect` · `reactive_anchor` · `register_open_redraw_effect`。

GPUI 用 `bool` + `RenderOnce`；**细粒度是 RUI 优势**，但应通过 **受控/非受控 helper**（如已有 `toggle_state`）减少重复 effect，而非在每个组件重写。

### 3.6 组件间重复发明（可立刻减行）

| 重复块 | collapsible | accordion | 建议 |
|--------|-------------|-----------|------|
| `*PanelPresenceCell` | ✓ | ✓（改名） | 合并 → `CollapsePanelPresence` |
| `apply_*_panel_motion` | ✓ | ✓ | 合并 → `apply_collapse_motion` |
| `seed_*_enter/exit_anim` | ✓ | ✓ | 合并 keyframes 常量 |

dialog / sheet 等若存在类似退场逻辑，应消费同一 `Presence` + motion 原语，禁止第三套 registry。

### 3.7 功能范围不完全对等

RUI `collapsible` **多于** gpui-base 的功能包括：exclusive `group` 互斥、ShadCN `data-slot`、chevron 自动、双 API、完整退场。合理目标：**同等功能 ≤ ~400 行产品壳 + 共享原语**，而非盲目追 200 行。

---

## 4. 对照总结表

| 维度 | gpui-component | RUI 现状 | 优化方向 |
|------|----------------|----------|----------|
| 组件文件 | 薄 facade | 行为+样式+动画+API 全塞 | 拆层 + 抽原语 |
| 动画 | `motion/` 共享 | 每组件 keyframes 状态机 | `HeightReveal` / 统一 `Presence` |
| 布局 | GPUI `Element` trait | 手写 `LayoutTree` + damage | 封装 `layout_sync(el)` 助手 |
| 状态 | `bool` + spring | Signal + effect + context | 保留 Signal；扩 `controlled` / `toggle_state` 模式 |
| 重复 | base 复用 | accordion ≈ collapsible 复制 | **P0** 合并 |
| API | 单 fluent 面 | RSX + Builder | 单实现路径 |
| 复合件 | 应用层组合 | ShadCN Trigger/Content | 行为下沉；产品壳薄化 |

---

## 5. 优化路线图（按 ROI）

与 [AUTHORITY.md](../AUTHORITY.md) 波次兼容；**不新开架构编号**。新组件与重构 PR 应引用本文 §5 优先级。

### P0 — 合并 collapse panel（立刻 · 低风险）

**新增** `crates/rui/src/utils/collapse_panel.rs`（或 `primitives/collapse_reveal.rs`）：

```text
CollapsePanelPresence      // 合并 Collapse* + Accordion* presence
apply_collapse_motion      // 合并 apply_collapse_panel_motion / apply_accordion_panel_motion
seed_collapse_keyframes    // collapsible-down/up 常量一处
```

- `collapsible.rs` / `accordion.rs` 只保留：**产品语义**（exclusive vs single/multiple、roving、`value` 列表）。
- **退出信号**：两文件各减 **40–50%** 行数；库测迁到 `utils` 单测。

### P1 — progress 驱动的 HeightReveal（中长期 · 减动画行数）

学 gpui `MotionReveal` **语义**，不引入 GPUI 依赖：

```text
HeightReveal { stable_id, progress: f32, child }
  → layout: height = measured_content * progress.clamp(0, 1)
  → overflow: hidden
```

- 默认路径用 progress；ShadCN `collapsible-down/up` 可作为 **可选** theme（ease vs spring）。
- 与 [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §2.1 的 `motion.rs` 规划对齐。

### P2 — 强制目录分层（新组件遵守 · 老组件渐进）

```text
components/collapsible/
  mod.rs          # 公开 API + re-export（<150 行）
  scope.rs        # Context + open state
  parts.rs        # Trigger / Content RSX 壳
  builder.rs      # Builder only
```

**硬规则**：单文件 **>300 行** 禁止继续堆功能——只能下沉 `utils/` 或拆子模块（`toast/`、`carousel/` 先例）。

### P3 — 双书写面单实现

- RSX 宏 **只** 调 `*_scope` + 子件注册。
- Builder **只** 调同一 `*_scope`；禁止复制 `apply_collapse_motion` 或第二套树构建。
- 审计清单：每个 `components/*.rs` 的 `From<*Builder>` 是否与 scope 重复建树。

### P4 — 三层与 gpui-component 对齐（保留 ShadCN 复合 API）

| 层 | 路径 | 行数预算 |
|----|------|----------|
| 行为 | `utils/` · `primitives/` | Collapse / Popover / FocusTrap |
| 无头复合 | `components/internal/` 或 `*_parts.rs` | Trigger/Content 只拼树 + context |
| 产品 | `components/` | variant、token、chevron 默认样式 |

对应 gpui：`gpui-base` ≈ 行为 + 无头；`gpui-component` ≈ 产品。见 [GPUI_ECOSYSTEM_COMPARISON.md](../comparison/GPUI_ECOSYSTEM_COMPARISON.md) §5.2。

### P5 — KPI：功能对等，非行数崇拜

| 指标 | 目标 |
|------|------|
| 新中等浮层组件 | **<500 行**，且不复制 toast TLS/hoist 仪式（见 [custom_components_suggestions.md](custom_components_suggestions.md) §3） |
| Collapsible / Accordion 重构后 | 各 **<450 行** 产品壳 + 共享 `collapse_panel` |
| 重复 panel motion | **0** 份（仅 `utils` 一处） |

---

## 6. 未拆分单文件组件逐项深读

下列对照 **gpui-component 仍保持单文件（或 ui 单文件 + base 单文件）** 的组件，与 RUI **尚未拆子目录** 的同名/对位组件。读法：**先看 gpui 合计行数，再看依赖的共享模块，最后看 RUI 多出来的产品面**。

### 6.1 gpui-component 的五种「单文件形态」

| 形态 | 示例 | ui 行数 | base | 特征 |
|------|------|--------:|-----:|------|
| **A · 极薄 facade** | `hover_card.rs` | 135 | 289 | `RenderOnce` 里委托 `gpui_base::*` + `Popover::render_popover_content` |
| **B · ui 单体仍厚** | `combobox.rs` | **1444** | 218 | 同一文件内 `Entity` + `SearchableListDelegate`；列表逻辑在 `searchable_list/` |
| **C · 仅 ui、无 base** | `kbd.rs` · `label.rs` · `badge.rs` | 147–334 | — | 纯 `RenderOnce` + theme；无复合 Trigger/Content |
| **D · base 为主** | `dialog.rs`（仅 base）· `progress.rs` | — / 少 | 158–630 | ui 层极薄或不存在 |
| **E · 假单文件** | `input/` · `button/` · `table/` | 3000+ | 可选 | **目录**合计才是真实体量 |

**误区**：只打开 `hover_card.rs`（135 行）会以为「gpui 都很短」；打开 `combobox.rs`（1444 行）或 `input/`（4359 行）则相反。

### 6.2 Combobox

| | gpui-component | RUI |
|--|----------------|-----|
| **行数** | ui **1444** + base **218** = **1662**；另依赖 `searchable_list/`、`list/`、`input/` | **1283** 单文件 |
| **架构** | `Entity<ComboboxState<D>>` + delegate 模式；键盘绑定在 base `init`；弹出层用 `List` / `SearchableList` | `combobox_scope` + `ComboboxInput` / `ComboboxItem` RSX 复合 + `ComboboxBuilder` |
| **API** | 单 fluent `Combobox::new` + `render_trigger` 闭包 | ShadCN 全复合：`ComboboxInput` · `List` · `Group` · `Empty` · `Separator` |
| **结论** | gpui **单文件并不短**；优势是 **列表虚拟化/搜索已抽到 searchable_list**，combobox 只做胶水 | RUI 总量略少，但 **过滤/可见项/active_index 全在组件内**；应把列表导航抽到 `utils/list_nav` + 与 Select 共用 |

### 6.3 HoverCard

| | gpui-component | RUI |
|--|----------------|-----|
| **行数** | ui **135** + base **289** = **424** | **609** |
| **架构** | base：`HoverCard` + `HoverCardState` `Entity` + `Popup` 延迟开闭；ui：`.trigger()` `.content()` → `Popover::render_popover_content` | 自研 **thread_local 定时器表** + `hover_card_next_wake_at` / `tick_hover_card`；`mount_anchored_overlay` + RSX `HoverCardTrigger` / `Content` |
| **多出行数来源** | — | ① ShadCN 复合件与双 API；② **未复用** tooltip/popover 的延迟逻辑；③ `reactive_anchor` + portal 手写 |
| **优化** | 学 base：**开闭延迟 + Entity 状态机** 进 `utils/hover_intent.rs`（与 Tooltip 共用）；ui 文件目标 **<200 行** |

gpui ui 层全文仅 ~150 行，核心是：

```rust
BaseHoverCard::new(self.id)
    .open_delay(...)
    .trigger(...)
    .content(|state, window, cx| {
        Popover::render_popover_content(anchor, appearance, window, cx)
            .child(content(state, window, cx))
    })
```

### 6.4 ColorPicker

| | gpui-component | RUI |
|--|----------------|-----|
| **行数** | ui **567** + base **880** = **1447** | `color_picker/` 目录 **2352**（`model` · `view` · `drag`） |
| **架构** | base：`ColorPickerState` Entity + HSV 几何；ui：调色板网格 + `Slider` + `Input` + `Tab` + `Popover` 组合 | RUI 已拆文件但 **域逻辑仍全在组件 crate**；浮层/拖拽与 chart 无共享 |
| **结论** | gpui **base 承担颜色模型**（880 行），ui 只做皮肤 | RUI 应抽 `utils/color_model.rs`（HSV/hex 互转）+ 复用 `popover`；目录拆分正确，**共享层仍缺** |

### 6.5 Kbd

| | gpui-component | RUI |
|--|----------------|-----|
| **行数** | ui **297**（无 base） | **328** |
| **架构** | `Keystroke` → 平台修饰键格式化（macOS ⌃⌥⇧⌘ 大段 `cfg`）+ `RenderOnce` 画胶囊 | `kbd` / `kbd_group` + `take_packed` + token 色；**无** `binding_for_action` |
| **结论** | **接近parity**；gpui 行数多在 **跨平台键名映射**（~120 行 `format`） | RUI 若加 action→快捷键显示，应放 `utils/keystroke_format.rs`，勿撑爆 `kbd.rs` |

### 6.6 Label

| | gpui-component | RUI |
|--|----------------|-----|
| **行数** | ui **334**（无 base） | **583** |
| **架构** | `StyledText` + `highlights` / `masked` / `secondary`；无选区 | `TextBuilder` + **`for=` 关联** + **文本选区** + 剪贴板 + `accesskit labelled_by` |
| **多出行数来源** | — | RUI **功能超集**（可选中 Label 是产品决策）；选区/剪贴板应下沉 `utils/text_selection.rs`（与 Input 共用） |
| **结论** | 对等「静态标签」RUI 可压到 **~350 行**；选区作为 feature 或 `LabelSelectable` 子类型 |

### 6.7 其他「gpui ui 薄、RUI 胖」单文件

| 组件 | gpui 合计 | RUI | 主要原因 |
|------|----------:|----:|----------|
| **breadcrumb** | 155 | 688 | RUI 完整 ShadCN 复合 + `Ellipsis` + 当前项样式；gpui 仅一个 `Breadcrumb` fluent |
| **badge** | 147 | 518 | RUI variant/size 矩阵 + 双 API + `asChild` 式 slot |
| **pagination** | 459 | 1626 | RUI：`pagination_range` 算法 + **Jumper** + `fit_pagination_for_width` 自适应；gpui 用 `PaginationState` + Button 组合 |
| **progress** | 158 (base) | 1051 | RUI：**条形 + 环形** + `ProgressTrack/Label/Value/Indicator` 全套复合；gpui base 仅条 |

### 6.8 其他「RUI 反而更瘦」的对照（避免片面自卑）

| 组件 | gpui 合计 | RUI | 说明 |
|------|----------:|----:|------|
| **input** | 4359 (ui/) | 1026 | gpui 编辑器/LSP/多行/IME 全在 input 目录 |
| **button** | 3015 (ui/) | 1545 | gpui button/ 含 variant 系统与更多平台态 |
| **table** | 3688 (ui/) | 931 | gpui 虚拟表+列调整在 table/ |
| **checkbox** | 1082 | 522 | RUI 已用 `toggle_state` |
| **combobox** | 1662 | 1283 | 见 §8.2 |

### 6.9 按组件类型的优化抓手

| 类型 | 代表 | RUI 优先动作 |
|------|------|----------------|
| 浮层 + 延迟 | hover_card, tooltip | 抽 `utils/hover_intent.rs`；与 popover 共用 Esc/anchor |
| 列表选择 | combobox, select | 抽 `searchable_list` 等价物；与 `list_nav` 合并 |
| 展示复合 | progress, pagination, badge | 拆 `parts.rs`；算法进 `utils/pagination.rs` 等 |
| 折叠动画 | collapsible, accordion | **P0** `collapse_panel`（§5） |
| 纯展示 | kbd, label, separator | 保持单文件；超 400 行只下沉 format/selection |
| 重域表单 | color_picker | 保持目录；抽 `color_model` + 复用 popover |

---

## 7. 与 gpui-component 的「该学 / 不该搬」

| 该学 | 不该搬 |
|------|--------|
| base / motion **分层**与摊销 | 整体迁 GPUI + gpui-component 栈 |
| `MotionReveal` + `progress` 语义 | 弃 Signal 换 `Render` trait |
| 组件文件 = facade | 砍掉 ShadCN Trigger/Content 复合 API |
| Story / 截图测驱动 | Node/Bun 绑定层 |

底座禁区不变：winit + wgpu + taffy + glyphon + Solid Owner（纲领 ADR）。

---

## 8. 交叉文档

| 文档 | 关系 |
|------|------|
| [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §2 | 分层职责权威 |
| [custom_components_suggestions.md](custom_components_suggestions.md) | 原语缺口 · Toast 类复杂度 |
| [custom_components.md](custom_components.md) | 作者写复合控件手册 |
| [GPUI_ECOSYSTEM_COMPARISON.md](../comparison/GPUI_ECOSYSTEM_COMPARISON.md) | GPUI 生态总对照 |
| [component_inventory.md](component_inventory.md) | 分桶与验证状态 |

---

## 附录 A. RUI 大文件监视列表（>1000 行 · 2026-09-02）

优先按 P2 拆层或 P0/P1 抽原语：

| 文件 | ~行数 | 备注 |
|------|-------|------|
| `chart/define.rs` | ~2058 | grammar 产品复杂度；保持子模块 |
| `carousel/*` | ~3500 合计 | 已分子模块；继续抽 motion |
| `toast/*` | ~2800 合计 | Presence/Overlay 原语收益最大 |
| `collapsible.rs` | ~1108 | **P0** 与 accordion 合并 motion |
| `accordion.rs` | ~1233 | **P0** 同上 |
| `color_picker/` | ~2352 合计 | 已拆目录；抽 `color_model`（§6.4） |
