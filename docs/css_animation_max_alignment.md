# CSS 动画最大对齐计划（ANIM）

> **文档地位**：巩固期 **ANIM 域唯一执行源**（勾选、排期、DoD）。  
> **前提**：[`animation_and_motion.md`](animation_and_motion.md) 已定义 keyframes / animation / motion 分层；本文定义 **与 Web CSS Animations + Transitions 的最大对齐范围** 及 **一次性落地** 步骤。  
> **规范参照**：[MDN CSS Animations](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Animations) · [Animatable properties](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Animations/Animatable_properties) · CSS Animations Level 1 · CSS Transitions  
> **日期**：2026-08-29

---

## 0. 目标与三问结论

### 0.1 总目标

**一次性** 建立与 Web 对齐的动画基底（`AnimatableProperty` 单轨 + Specified/Used 值模型 + 布局动画 + 文本继承属性），使组件创作 **只写 CSS 语义**（class / `animation()` / `@keyframes` 文本），**禁止** 再为单个组件向 `KeyframeStop` 手工加字段。

### 0.2 三问直接回答

| # | 问题 | 结论 |
|---|------|------|
| **1** | CSS **cascaded value** 能否在 RUI 实现、且**组件不做布局测量**？ | **能，但要分层。** 非布局属性（opacity、transform、color…）可完全由引擎维护 **Specified ↔ Used** 双轨，无需组件测量。**尺寸类**（`height`/`max-height: auto` → 内容高度）的 cascaded **used value** 在浏览器里同样来自 layout；RUI 可在 **布局 pass 后自动写入 `Element.used_layout` 缓存**，Accordion 删除 `measure_panel_natural_height`，改读引擎缓存。不是「零 layout」，是 **零组件侧测量**。 |
| **2** | Layout 动画是否要做、如何优化？ | **要做。** 列入 ANIM-3；优化：**子树增量 layout**、layout 属性动画与 compositor 属性分 tick、`will-change` 提示、动画期间合并 layout 请求、非动画帧跳过 layout。 |
| **3** | Text `color` 等如何补齐？ | **扩展动画目标到 `Element` 级 `ComputedPresent` 视图**（`Style` + 继承后的 `TextStyle` + 子树可动画字段）；transition/keyframes .tick 从「只写 `Style`」改为「写 Present + 文本继承链」。 |

---

## 1. 对齐边界（写什么 / 不写什么）

### 1.1 纳入「最大对齐 v1」

| 域 | 内容 |
|----|------|
| **@keyframes** | 多 stop、分段 `timing-function`、implicit 0%/100%、Rust 注册 + **CSS 文本解析** |
| **animation-*** | L1 全部 longhands（name/duration/delay/timing/iteration/direction/fill-mode/play-state）；`steps()`/`jump-*` |
| **transition-*** | property/duration/delay/timing；与 keyframes **共用**插值表 |
| **Animatable 属性** | 见 §3 矩阵：A 合成层 + B 绘制层 + C 布局层 + D 文本/继承层 |
| **Reduced motion** | `prefers-reduced-motion` → 时长归零或 `animation: none` |
| **Utility** | Tailwind `animate-*` / `transition-*` 与属性表 1:1 |

### 1.2 明确不纳入 v1（另开 WP）

| 项 | 理由 |
|----|------|
| Scroll/View **`animation-timeline`** | CSS Animations L2；依赖 scroll 时间线基础设施 |
| **`animation-composition: accumulate`** 真实叠加 | 仅存档字段；v1 一律 replace |
| **Web Animations API** / DOM **`animation*`** 事件 | 无 DOM；用 Rust presence/on_finish 钩子替代 |
| **`background-position`** 等纯装饰动画 | 不在 `Style` 绘制 MVP |
| **`interpolate-size` / `calc-size()`** | 可 v2 替代 intrinsic 路径；v1 用 `used_layout` 缓存 |

---

## 2. Cascaded Value 模型（回答 Q1 的设计）

### 2.1 Web 语义映射

```text
Author style (Builder / class / reactive_sync 写入)
        ↓
Specified values     ← Element.specified_style (+ specified_text_style)
        ↓  cascade + inherit（已有 cascade_inheritable_text_style）
Computed/Cascaded    ← 继承解析后、layout 前
        ↓  layout（Taffy + intrinsic measure）
Used values          ← Element.used_layout + used_present（layout 后缓存）
        ↓  animation/transition tick 写入 mid
Animated used values ← 单帧 Style/TextStyle 上的「当前绘制值」
```

浏览器把 specified 与 animated used **分槽存储**；RUI 目前只有单 `Style` + `written` receipt。ANIM-1 引入：

```rust
// crates/rui/src/style/animatable.rs（新建）
pub struct SpecifiedPresent { /* 作者意图，不受 tick 污染 */ }
pub struct UsedLayout {
    pub content_width: f32,
    pub content_height: f32,   // intrinsic / max-content 高度
    pub border_box: [f32; 4],
}
pub struct PresentStyle { /* 本帧绘制：Style 子集 + TextStyle 子集 */ }
```

**Tick 规则（对齐 CSS）：**

1. 每帧 keyframes 采样前，从 **Specified + UsedLayout** 重建 `KeyframeBase`（非从上帧 animated 值漂移）。
2. Transition 目标变化：比较 **Specified** 目标，不是 mid。
3. 动画结束 / 移除：`fill-mode: none` 恢复 Specified；`forwards` 保持最后一帧。

### 2.2 不用组件测量，Accordion 如何工作

**现状（待删除）：** `accordion.rs` → `measure_panel_natural_height` → 手动设 `max_size.height` → seed。

**目标：**

```text
1. 作者：max-height: auto（specified）；keyframes: accordion-down 仅声明 0% { max-height: 0 }
2. Layout pass（height:auto + overflow:hidden）→ 引擎写 used_layout.content_height
3. tick_element_animations 读 KeyframeBase.max_height = used_layout.content_height
4. 动画 mid 写 max_size.height（used），不反写 specified
```

组件只保留 **presence**（何时挂载/卸载），**删除** `measured_height` / `exit_from_height` / `measure_panel_natural_height`。

### 2.3 能否完全不做 layout？

| 属性类 | 无 layout 可否 cascaded used？ |
|--------|-------------------------------|
| opacity, transform, filter | ✅ |
| color, bg-color, border-color | ✅ |
| box-shadow 分量 | ✅ |
| width/height/max-/min-*, padding, margin, gap | ❌ used 值依赖 Taffy |
| max-height: auto → content height | ❌ 必须一次 layout 定 intrinsic |

**结论：** 「cascaded value 不用布局测量」对 **非尺寸属性成立**；尺寸动画 **允许引擎在 layout pass 自动算 intrinsic**，禁止组件手写测量。

---

## 3. Animatable 属性矩阵（对齐 MDN）

图例：**✅ v1** · **🔜 ANIM 波次** · **—** 不纳入 v1

### 3.1 A 层 — 合成 / 不触 layout

| CSS property | RUI 字段 | keyframes | transition | 波次 |
|--------------|----------|-----------|------------|------|
| `opacity` | `Style.opacity` | ✅ | ✅ | 已有 |
| `transform` | `translate/scale/rotate` + `TransformList` | ✅ | ✅ | 已有；ANIM-2 统一 matrix 路径 |
| `translate` / `scale` / `rotate` | 同上 | ✅ | ✅ | 已有 |
| `filter` | `Style.filter` | 🔜 | 部分 | ANIM-2 |
| `backdrop-filter` | `Style.backdrop_blur` + filter | 🔜 | 🔜 | ANIM-2 |

### 3.2 B 层 — 绘制、不触 layout

| CSS property | RUI 字段 | keyframes | transition | 波次 |
|--------------|----------|-----------|------------|------|
| `background-color` | `Style.bg_color` | 🔜 | ✅ | ANIM-2 |
| `border-color` | `border_color` / sides | 🔜 | ✅ | ANIM-2 |
| `outline-color/width/offset` | outline_* | 🔜 | ✅ | ANIM-2 |
| `box-shadow` | shadow_* + extra_shadows | 🔜 | 部分 | ANIM-2 |
| `border-radius` | `Style.radii` | 🔜 | 🔜 | ANIM-2 |

### 3.3 C 层 — 布局动画（Q2）

| CSS property | RUI / Taffy | keyframes | transition | 波次 |
|--------------|-------------|-----------|------------|------|
| `width` / `height` | `size` | 🔜 | 🔜 | ANIM-3 |
| `min-width/height` | `min_size` | 🔜 | 🔜 | ANIM-3 |
| `max-width/height` | `max_size` | ✅ max-h | ✅ max-h | ANIM-3 泛化 |
| `padding-*` | `padding` | 🔜 | 🔜 | ANIM-3 |
| `margin-*` | `margin` | 🔜 | 🔜 | ANIM-3 |
| `gap` | `gap` | 🔜 | 🔜 | ANIM-3 |
| `border-width` | `border_width*` | 🔜 | 🔜 | ANIM-3 |
| `flex-grow/shrink/basis` | flex_* | 🔜 | 🔜 | ANIM-3 |
| `font-size` | `TextStyle.size` | 🔜 | 🔜 | ANIM-4 |
| `line-height` | TextStyle leading | 🔜 | 🔜 | ANIM-4 |
| `letter-spacing` | TextStyle | 🔜 | 🔜 | ANIM-4 |

### 3.4 D 层 — 文本 / 继承（Q3）

| CSS property | RUI 字段 | 说明 | 波次 |
|--------------|----------|------|------|
| `color` | `TextStyle.color` | 继承；tick 写叶或容器 | ANIM-4 |
| `text-decoration-color` | `TextStyle.decoration_color` | 继承 | ANIM-4 |
| `-webkit-text-fill-color` | 映射 `TextStyle.color` | shadcn 兼容 | ANIM-4 |
| `caret-color` | Input chrome | 子模块 | ANIM-4 |

**实现要点：** `AnimatableProperty` 带 `TargetSlot::Style(…)` | `TargetSlot::TextStyle(…)` | `TargetSlot::InheritedText(…)`；tick 遍历子树时 **只改 animated layer**，不破坏 specified。

---

## 4. Layout 动画优化（回答 Q2）

### 4.1 分级策略

```text
Compositor tick（每帧，无 layout）
  opacity, transform, filter, opacity-only damage

Paint tick（每帧，无 layout）
  colors, shadow, radii → frame_paint_damage

Layout tick（仅 C 层属性变化时）
  size/padding/margin/gap → mark_element_layout_dirty(subtree)
  → 合并：同一帧多属性只 layout 一次
  → 子树边界：animated 节点为 root 的 layout_subtree（见 layout_subtree.md）
```

### 4.2 具体优化项

| 优化 | 做法 |
|------|------|
| **Layout coalescing** | `AppState` 收集 `LayoutDirtySet`；帧末单次 `compute_layout` |
| **Subtree layout** | C 层动画节点标记 `layout_subtree_root`；不整树重算 |
| **UsedLayout 缓存** | layout 完成后写 `content_height`；keyframes 读缓存，不重复 measure API |
| **`will-change` 等价** | `Style.will_change: EnumSet<AnimatableProperty>`；动画前提升 compositor 路径 |
| **Damage 分级** | compositor 动画 → `push_frame_paint_damage` only；layout 动画 → layout + paint |
| **Reduced motion 降级** | C 层动画 → 直接 snap 终值 + 一次 layout，不逐帧插值 |
| **Intrinsic 动画** | `max-height: 0 ↔ auto`：仅首帧 + 末帧 full layout；mid 帧插 px（与浏览器一致） |

### 4.3 性能预算（DoD）

- 单元素 compositor 动画：**0** layout/frame  
- 单 Accordion 面板展开：**≤ 2** layout（首帧 intrinsic + 可选末帧 settle）  
- 10 并行 layout 属性动画：**1** coalesced layout/frame 树内

---

## 5. 目标架构（单轨，消灭 Accordion 式补丁）

```text
style/animatable.rs       PropertyId, TargetSlot, InterpolateFn, LayoutImpact
style/animatable_table.rs 静态注册表（§3 矩阵代码化）
style/specified.rs        SpecifiedPresent 读写、与 Style 同步
style/used_layout.rs      UsedLayout 缓存、layout pass 钩子
style/keyframes.rs        KeyframeStop → BTreeMap<PropertyId, Value>
style/animation.rs        统一 tick（transition + keyframes → PresentStyle）
style/easing.rs           cubic-bezier + steps/jump-*
utils/motion.rs           不变：Element transition 配方
```

**冻结规则（ANIM-0 起）：** 禁止新增 `KeyframeStop` 字段；禁止新增 `TransitionProperty` 枚举成员而不进注册表。

---

## 6. 一次性落地 To-Do（按依赖顺序）

> 勾选仅在本文维护。完成一项勾一项；**不按组件拆 PR**，按 ANIM 波次合并。

### ANIM-0 — 治理与冻结（1–2 天）

- [ ] **A0.1** 在 [`AUTHORITY.md`](AUTHORITY.md) §1.2 争议路由增加 ANIM 域 → 本文  
- [ ] **A0.2** [`animation_and_motion.md`](animation_and_motion.md) 顶部链接本文为「对齐执行源」  
- [ ] **A0.3** 代码注释 / PR 模板：**禁止** 扩展 `KeyframeStop` 手工字段（CI 可选 grep 检查）  
- [ ] **A0.4** Gallery 增「ANIM 对齐」占位页（后续逐波填 demo）

### ANIM-1 — Specified / Used 基底 + 单轨插值（核心，~1–2 周）

- [ ] **A1.1** 新建 `style/animatable.rs`：`PropertyId`、`AnimatableValue`、`LayoutImpact::{None,Paint,Layout}`  
- [ ] **A1.2** 新建 `style/animatable_table.rs`：注册 A 层现有属性（opacity/transform/max-height）  
- [ ] **A1.3** 新建 `style/specified.rs`：`SpecifiedPresent`；`Element` 增加 `specified_present` / `present` 分槽（或等价 RefCell 侧车）  
- [ ] **A1.4** 新建 `style/used_layout.rs`：`UsedLayout`；`layout_tree` layout 完成后写入 `content_height` / `content_width`  
- [ ] **A1.5** 重构 `tick_element_animations`：采样前 `KeyframeBase` 来自 `specified + used_layout`  
- [ ] **A1.6** 重构 `tick_element_transitions`：`specified_prop_value` 改为读 Specified 槽  
- [ ] **A1.7** `AnimationLock` 改为 `EnumSet<PropertyId>` 或位图，删除手写 match  
- [ ] **A1.8** 迁移 `KeyframeStop` → `HashMap<PropertyId, AnimatableValue>`（保留 builder API 兼容层）  
- [ ] **A1.9** 新建 `style/easing.rs`：实现 `steps(n, start|end)`、`jump-*`  
- [ ] **A1.10** 测试：cascaded base 不漂移；transition 目标不 mid 误判；animation 移除恢复 specified  
- [ ] **A1.11** 测试：used_layout.content_height 在 height:auto 子树正确

### ANIM-2 — A+B 层属性全覆盖（~1 周）

- [ ] **A2.1** 注册表加入：filter、backdrop-filter、bg/border/outline color、box-shadow 分量、border-radius  
- [ ] **A2.2** keyframes 内置规则补全：filter fade、shadow tween（与 Toast 对齐）  
- [ ] **A2.3** `apply_keyframe_sample` 泛化为表驱动  
- [ ] **A2.4** Tailwind `animate-*` 回归；golden 如有则更新  
- [ ] **A2.5** Gallery：合成层 + 阴影/圆角 keyframes demo

### ANIM-3 — C 层布局动画 + 优化（~2 周）

- [ ] **A3.1** 注册表加入 C 层（§3.3 全表）  
- [ ] **A3.2** `AppState`：`LayoutDirtySet` + 帧末 coalesced layout  
- [ ] **A3.3** `mark_element_layout_dirty_subtree` 与 animation tick 集成  
- [ ] **A3.4** `will_change` 字段 + compositor 快路径  
- [ ] **A3.5** intrinsic `max-height` / `height` 动画：读 `used_layout`，**删除** Accordion 组件测量  
- [ ] **A3.6** 备选：`grid-template-rows: 0fr→1fr` 作为 `accordion-down` 第二实现（feature flag 对比）  
- [ ] **A3.7** 性能测试：Accordion 展开 layout 次数 ≤ 2；10 节点 padding 动画 1 layout/frame  
- [ ] **A3.8** Gallery：layout 动画段（width/padding/gap）

### ANIM-4 — D 层文本 / 继承 + 作者面（~1–2 周）

- [ ] **A4.1** `TargetSlot::TextStyle` / `InheritedText`；tick 写 `Element.text_style`  
- [ ] **A4.2** 实现 `color` / `text-decoration-color` transition + keyframes  
- [ ] **A4.3** `cascade_inheritable_text_style` 与 specified 文本槽协同（animated 不污染 inherit 源）  
- [ ] **A4.4** `@keyframes` **CSS 文本解析** → `register_keyframes`（`from/to/百分比` + 属性名）  
- [ ] **A4.5** `prefers-reduced-motion`：读 theme/media；动画 duration→0 或 disable  
- [ ] **A4.6** 删除组件内重复 `dialog_css_anim` 式工厂，统一 `keyframes_anim_entry` + preset  
- [ ] **A4.7** Gallery：文本 color 渐变、inherit 动画 demo

### ANIM-5 — 迁移、清理、DoD（~1 周）

- [ ] **A5.1** **Accordion**：删 `measure_panel_natural_height`、`measured_height`、`exit_from_height`；仅 presence + class  
- [ ] **A5.2** **Dialog/Modal**：seed 只写 specified opacity/scale；used 由引擎  
- [ ] **A5.3** **Toast**：确认仍走 transition+motion；shadow/filter 走注册表  
- [ ] **A5.4** 全库 `cargo test` + Gallery ANIM 页目视验收  
- [ ] **A5.5** 更新 [`component_inventory.md`](component_inventory.md) ANIM 列  
- [ ] **A5.6** 废弃旧 API：`KeyframeStop` 字段 builder 标记 `deprecated` → 下一大版本删除  
- [ ] **A5.7** 文档：§3 矩阵「✅」列与实现 1:1 对齐审计

### ANIM-6 — 延后（不挡 v1 DoD）

- [ ] **A6.1** Scroll-driven `animation-timeline`  
- [ ] **A6.2** `animation-composition: accumulate`  
- [ ] **A6.3** `interpolate-size` / intrinsic keyword 动画  
- [ ] **A6.4** `background-position` / `clip-path` keyframes  

---

## 7. 验收标准（整项 ANIM v1 DoD）

1. **零组件测量**：Accordion / Collapsible 无 `measure_*_height`；intrinsic 全走 `used_layout`。  
2. **属性表**：§3 矩阵 A+B+C+D 的 v1 行全部为 ✅。  
3. **双引擎共用**：任意属性同时支持 transition + keyframes，同一插值函数。  
4. **Specified 不漂移**：动画 mid 不写入 specified；结束行为符合 `fill-mode`。  
5. **Text color**：`<Label>` / 文本叶 `transition: color` + `@keyframes fade` 可用。  
6. **Layout 预算**：§4.3 性能预算测试 CI 通过。  
7. **Reduced motion**：系统偏好开启时 Gallery 动画可验证跳过。  
8. **Regression**：Dialog / Toast / Carousel 现有测绿。

---

## 8. 风险与对策

| 风险 | 对策 |
|------|------|
| `Element` 增槽破坏 binary/layout | 侧车 `Rc<RefCell<AnimationPresent>>` 先落地，稳定后再 inline |
| Layout 动画掉帧 | coalescing + subtree；超预算降级 snap |
| Text 继承 + 动画冲突 | specified 文本 inherit 源与 present 分离；layout 前 cascade |
| 与 reactive_sync 写 Style 冲突 | reactive 只写 specified；tick 只写 present |
| 范围膨胀 | §1.2 硬边界；ANIM-6 不纳入 v1 DoD |

---

## 9. 相关文档与代码

| 资源 | 路径 |
|------|------|
| 分层决策 | [`animation_and_motion.md`](animation_and_motion.md) |
| Layout 子树 | [`layout_subtree.md`](layout_subtree.md) |
| Damage | [`damage.md`](damage.md) |
| 引擎现状 | `crates/rui/src/style/{keyframes,animation,element_style}.rs` |
| Accordion 待删测量 | `crates/rui/src/components/accordion.rs` |
| 文本继承 | `layout_tree.rs` → `cascade_inheritable_text_style` |

---

## 10. 变更记录

| 日期 | 说明 |
|------|------|
| 2026-08-29 | 初版：三问结论 + 属性矩阵 + ANIM-0~6 一次性 To-Do |
