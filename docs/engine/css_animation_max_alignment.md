# CSS 动画（animation / transition / @keyframes）

> **文档地位**：CSS Animations Level 1 + CSS Transitions 的**已承诺子集**事实手册（近乎完全对齐 Web 的冻结面）。分层选型仍见 [`animation_and_motion.md`](animation_and_motion.md)。  
> **冻结状态**：**已冻结**（2026-09-05 子集）。纠错（行号、测试命令、矩阵笔误）可改；策略变更（扩面、改插值、放开 §1.2）须改本文，禁止静默扩面。  
> **执行源**：ANIM-0～6 已退出排期；勾选清单仅为落地史（§6），不再当 To-Do。  
> **实现锚点**：`style/animatable.rs`（`PropertyId` 单轨）· `style/timing.rs`（timing / spring 核）· `style/animation.rs`（transition / keyframes tick）· `style/waapi.rs`（句柄注册表）· `style/keyframes.rs` + `keyframes_parse.rs` · `style/easing.rs` · `style/specified.rs` · `style/used_layout.rs` · `style/clip_path.rs` · `style/scroll_timeline.rs` · `crate_tests/style/anim_freeze.rs`。  
> **规范参照**：[MDN CSS Animations](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Animations) · [Animatable properties](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Animations/Animatable_properties) · CSS Animations L1 · CSS Transitions  
> **日期**：2026-09-05（双引擎契约测绿；文档追上实现）。

交叉：[`animation_and_motion.md`](animation_and_motion.md) · [`box_model.md`](box_model.md) · [`layout_subtree.md`](layout_subtree.md) · [`damage.md`](damage.md)

---

## 0. 一句话

RUI 用 **一条 `PropertyId` 插值轨** 驱动 CSS `transition` 与 `@keyframes`：作者只写 class / `animation()` / `@keyframes` 文本；**禁止** 再给 `KeyframeStop` 手工加字段。组件不做布局测高。本冻结面承诺 §1.1 + §3 + **三层动画模型（§1.3）**；§1.2 仅保留尚未纳入的次要项。扩面仍须改本表再写代码。

---

## 0. 目标与三问结论

### 0.1 冻结目标（已达成）

与 Web 对齐的动画基底（`PropertyId` 单轨 + Specified/Used 值模型 + 布局动画 + 文本继承属性）已落地：组件创作 **只写 CSS 语义**（class / `animation()` / `@keyframes` 文本），**禁止** 再为单个组件向 `KeyframeStop` 手工加字段。

CI 锚点：`cargo test -p rui --lib anim_freeze`。

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

### 1.2 可做、尚未纳入（次要）

`PropertyId` 现用到 **62 = `OffsetRotate`**，`repr(u8)` 位图上限 **< 64**，还剩 **1** 个槽（63）。

| 项 | 为什么能做 | 接法 / 代价 |
|----|------------|-------------|
| **`background-size` 动画** | 普通 Vec2/`cover` discrete | 可挤进剩余 `PropertyId`；与 `background-position` 成对 |
| **`transition-behavior: allow-discrete`** | 离散端点在 t=0.5 切换（CSS） | 现成 discrete 路径加一个开关；不必新槽 |
| **`view-timeline-inset` 全集** | 已有 range 映射，inset 是采样窗口微调 | 扩 `view_timeline` 解析，不占 `PropertyId` |

**真正的硬边界（仍不做）：**

| 项 | 理由 |
|----|------|
| 浏览器 **DOM** `animation*` 事件名 / `EventTarget` / `Promise` / `DocumentTimeline` | 无 DOM；完成通知是 callback / `Signal`。每窗口一条单调时钟 |
| **编程面 `composite: add/accumulate`** | Web 上也极少用；CSS `animation-composition` 声明式表面仍保留 |
| **vertex instance loc 2–15 再加字段** | 已满；新几何走 **storage buffer**（clip poly 已是这条路），禁止再扩 instance 顶点槽 |
| **vertex interpolator loc >14** | 设备 `max_inter_stage_shader_variables` 为 0-based 14；`instance_index` 打进 loc 14 的 `extra_and_id.y`，禁止 `@location(15)` 输出 |

负 `animation-delay`：`after_delay = elapsed - delay_ms`，负 delay 立刻进入周期中段（CSS）。`anim_freeze` 锁定。

### 1.3 三层动画模型（引擎 / 声明式 / 句柄）

要的是 **模型**，不是 WAAPI 完整规范。CSS `transition` 已隐含 timing 的一半；编程面做 Rust 味子集。

| 层 | 承诺 | 落地 |
|----|------|------|
| **1 引擎** | timing + keyframes + easing + 中断/重定向 | `Timing::resolve`（delay / iterations / direction / fill / endDelay）· `PropertyId` 插值 · transition 从**当前计算值** retarget（spring 保留 `vel`）· `LayoutImpact` 分流 layout vs paint · `needs_frame` 驱动连续/按需重绘 |
| **2 声明式** | `transition:` / `animation:` / `@keyframes` | 已冻结 §1.1；另 `transition: … spring()` / `spring(k, d[, mass])` |
| **3 编程式** | Rust 句柄子集 | `Animation` = **id + 注册表**。`run(frames, Timing)` / `play` / `pause` / `reverse` / `set_rate` / `seek` / `finish` / `cancel` / `on_finish`。**不**照搬 DocumentTimeline、Promise、`getComputedTiming` 规范名（用 `KeyframeEffect::model()` → [`Timing`](../../crates/rui/src/style/timing.rs)） |

拿 / 丢：

| 拿 | 丢 |
|----|----|
| delay / endDelay / duration / iterations / direction / fill / playbackRate / currentTime | DocumentTimeline / 多时间线 |
| keyframes：offset + 每帧 easing + 插值 | 编程面 composite add/accumulate |
| play / pause / reverse / finish / cancel / seek | 与 CSS cascade 的完整集成（last-started-wins） |
| 完成通知（callback / Signal） | Promise 原样；「合成器线程跑动画」（本引擎就是合成器） |

已收进本冻结面的延伸：

| 项 | 落地 |
|----|------|
| **编程式句柄** | `style/waapi.rs`：`Keyframe` 属性袋、`Timing`、`Animation::run` / `Element.animate`。`play`/`pause`/`reverse`/`set_rate`/`seek`/`finish`/`cancel`（cancel **不**触发 finish）、`model()`、`finished`/`cancelled` Signal。匿名 `@keyframes` intern + `playback_id`。句柄表 `get_animations()` 是注册表 dump，不是 `document.getAnimations()` |
| **spring 腿** | `TimingFunction::Spring` + `spring_step`；`transition: transform spring()` 走积分而非 duration；重定向保留 `vel`。默认 stiffness 170 / damping 26 / mass 1 |
| **`linear()` 分段 easing** | `TimingFunction::LinearStops(u16)` intern 表；与 `steps()` 同层 |
| **`clip-path: path()`** | tessellate → `ClipPath::Polygon`（超 16 点 GPU 抽样） |
| **`clip-path: url(#id)`** | 文档 clip 定义表 `register_clip_path` / `lookup_clip_path`（collect 时后写覆盖） |
| **`offset-path` + `offset-distance` / `offset-rotate`** | 路径弧长采样写入 paint translate + 切线角；`PropertyId` 61/62 |
| **`transform-origin` 动画** | `PropertyId` 60，分数 0–1 |
| **`extra_shadows` 列表插值** | `ShadowListTween`：主 `shadow_*` 为第 0 层，等长 lerp；`inset` 不对齐则 discrete |
| **两层 polygon GPU 相交** | `GpuClipPoly` 第二层 storage + `and_layers`；CPU `contains_point` AND；祖先+本地 |
| **`calc-size()`** 求值器 | `CssDim::CalcSize` + `size` 关键字；keyframes / transition 把 `calc-size(auto, size + N)` 求到 used±N。首遍 layout 仍把 `calc-size(auto,…)` 当 `auto`。 |
| **`clip-path` 几何** | `inset()` / `polygon()` / `circle()` / `ellipse()`：解析、等顶点数插值、命中、AABB；polygon/circle/ellipse 走 SDF storage even-odd / nonzero。transition 与 keyframes **共用** `ClipPath::interpolate`（`ClipPathTween`）。 |
| **具名 `scroll-timeline` / `view-timeline`** | `--name` 跨节点查找（后写 = 更近祖先）+ `animation-range`（`%` / `normal` / `cover` / `entry` / `exit`）。匿名 `scroll()` / `view()` 仍走 A6.1。 |
| **独立 `scale` / `rotate` transition** | `transition-property: scale \| rotate` 走独立 tween；**不**扩进 `transition: all`（避免与 `transform` 双写）。 |

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

已落地（不再手写测高）：

```text
1. 作者：max-height: auto（specified）；keyframes: accordion-down 仅声明 0% { max-height: 0 }
2. Layout pass（height:auto + overflow:hidden）→ 引擎写 used_layout.content_height
3. tick_element_animations 读 KeyframeBase.max_height = used_layout.content_height
4. 动画 mid 写 max_size.height（used），不反写 specified
```

组件只保留 **presence**（何时挂载/卸载）。

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

## 3. Animatable 属性矩阵（对齐 MDN，已冻结）

图例：**✅** 双引擎（keyframes + transition）共用 `animatable::interpolate`（或与之等价的同函数调用）· **—** 明确不做（§1.2）

`transition: all` 展开 `ALL_PROPS`（**shorthand 不含 longhand**，避免 padding 双写）。独立 `scale` / `rotate` **不**在 `all` 内。

### 3.1 A 层 — 合成 / 不触 layout

| CSS property | RUI 字段 | keyframes | transition | 备注 |
|--------------|----------|-----------|------------|------|
| `opacity` | `Style.opacity` | ✅ | ✅ | |
| `transform` | `translate/scale/rotate` + `TransformList` | ✅ | ✅ | `all` 走 5 元组 packed |
| `translate` | `Style.translate` | ✅ | ✅ | 显式 `transition: translate` |
| `scale` / `rotate` | `Style.scale` / `rotate_deg` | ✅ | ✅ | 显式 `transition: scale \| rotate`；不进 `all` |
| `filter` | `Style.filter` | ✅ | ✅ | `FilterTween` + `interpolate_filter_lists` |
| `backdrop-filter` | `Style.backdrop_blur` | ✅ | ✅ | 半径标量 |

### 3.2 B 层 — 绘制、不触 layout

| CSS property | RUI 字段 | keyframes | transition | 备注 |
|--------------|----------|-----------|------------|------|
| `background-color` | `Style.bg_color` | ✅ | ✅ | 色走 `lerp_text_rgba`（与 keyframes 同函数） |
| `border-color` | `border_color` / sides | ✅ | ✅ | 同上 |
| `outline-color/width/offset` | outline_* | ✅ | ✅ | |
| `box-shadow` | 主 `shadow_*` | ✅ | ✅ | `extra_shadows` 列表不插值（§1.2） |
| `border-radius` | `Style.radii` | ✅ | ✅ | |
| `background-position` | `Style.bg_position` `[x,y]` 0–1 | ✅ | ✅ | 关键词/`%`；绘制偏移 gradient UV |
| `clip-path` | `Style.clip_path` | ✅ | ✅ | `ClipPathTween` = `ClipPath::interpolate`；`path()`/`url()` — |

### 3.3 C 层 — 布局动画

| CSS property | RUI / Taffy | keyframes | transition | 备注 |
|--------------|-------------|-----------|------------|------|
| `width` / `height` | `size` | ✅ | ✅ | 关键字需 `interpolate-size` / `calc-size()` |
| `min-width/height` | `min_size` | ✅ | ✅ | |
| `max-width/height` | `max_size` | ✅ | ✅ | `auto` → `used_layout` |
| `padding-*` | `padding` | ✅ | ✅ | |
| `margin-*` | `margin` | ✅ | ✅ | |
| `gap` | `gap` | ✅ | ✅ | |
| `border-width` | `border_width*` | ✅ | ✅ | |
| `flex-grow/shrink/basis` | flex_* | ✅ | ✅ | |
| `grid-template-rows` | 等长 `fr` 列表 | ✅ | ✅ | 1 轨 Scalar，2+ FrList；不等长 discrete |
| `grid-template-columns` | 同上 | ✅ | ✅ | |
| `font-size` | `TextStyle.size` | ✅ | ✅ | |
| `line-height` | TextStyle leading | ✅ | ✅ | |
| `letter-spacing` | TextStyle | ✅ | ✅ | |
| `zoom` | `Style.zoom` | ✅ | ✅ | 缩放布局尺寸 |

### 3.4 D 层 — 文本 / 继承

| CSS property | RUI 字段 | keyframes | transition | 备注 |
|--------------|----------|-----------|------------|------|
| `color` | `TextStyle.color` | ✅ | ✅ | 继承；tick 写叶或容器 |
| `text-decoration-color` | `TextStyle.decoration_color` | ✅ | ✅ | |
| `-webkit-text-fill-color` | 映射 `TextStyle.color` | ✅ | ✅ | shadcn 兼容 |
| `caret-color` | Input chrome | — | — | 子模块，不进 `PropertyId` 表 |

tick **只改 animated layer**，不破坏 specified。契约测：`crate_tests/style/anim_freeze.rs`。

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

**冻结规则：** 禁止新增 `KeyframeStop` 字段；新 animatable 属性必须进 `PropertyId` 注册表 **并改本文 §3 / §1.2**，禁止静默扩面。`PropertyId` discriminant 须保持 **< 64**。

---

## 6. 落地史（ANIM-0～6，已全部完成）

> 以下勾选是历史记录，**不再排期**。新工作以 §1.2 / §3 冻结面为准。

### ANIM-0 — 治理与冻结（1–2 天）

- [x] **A0.1** 在 [`AUTHORITY.md`](../AUTHORITY.md) §1.2 争议路由增加 ANIM 域 → 本文  
- [x] **A0.2** [`animation_and_motion.md`](animation_and_motion.md) 顶部链接本文为「对齐执行源」  
- [x] **A0.3** 代码注释 / PR 模板：**禁止** 扩展 `KeyframeStop` 手工字段（CI 可选 grep 检查）  
- [x] **A0.4** Gallery 增「ANIM 对齐」占位页（后续逐波填 demo）

### ANIM-1 — Specified / Used 基底 + 单轨插值（核心，~1–2 周）

- [x] **A1.1** 新建 `style/animatable.rs`：`PropertyId`、`AnimatableValue`、`LayoutImpact::{None,Paint,Layout}`  
- [x] **A1.2** 新建 `style/animatable_table.rs`：注册 A 层现有属性（opacity/transform/max-height）  
- [x] **A1.3** 新建 `style/specified.rs`：`SpecifiedPresent`；`Element` 增加 `specified_present` / `present` 分槽（或等价 RefCell 侧车）  
- [x] **A1.4** 新建 `style/used_layout.rs`：`UsedLayout`；`layout_tree` layout 完成后写入 `content_height` / `content_width`  
- [x] **A1.5** 重构 `tick_element_animations`：采样前 `KeyframeBase` 来自 `specified + used_layout`  
- [x] **A1.6** 重构 `tick_element_transitions`：`specified_prop_value` 改为读 Specified 槽  
- [x] **A1.7** `AnimationLock` 改为 `EnumSet<PropertyId>` 或位图，删除手写 match  
- [x] **A1.8** 迁移 `KeyframeStop` → `HashMap<PropertyId, AnimatableValue>`（保留 builder API 兼容层）  
- [x] **A1.9** 新建 `style/easing.rs`：实现 `steps(n, start|end)`、`jump-*`  
- [x] **A1.10** 测试：cascaded base 不漂移；transition 目标不 mid 误判；animation 移除恢复 specified  
- [x] **A1.11** 测试：used_layout.content_height 在 height:auto 子树正确

### ANIM-2 — A+B 层属性全覆盖（~1 周）

- [x] **A2.1** 注册表加入：filter、backdrop-filter、bg/border/outline color、box-shadow 分量、border-radius  
- [x] **A2.2** keyframes 内置规则补全：filter fade、shadow tween（与 Toast 对齐）  
- [x] **A2.3** `apply_keyframe_sample` 泛化为表驱动  
- [x] **A2.4** Tailwind `animate-*` 回归；golden 如有则更新  
- [x] **A2.5** Gallery：合成层 + 阴影/圆角 keyframes demo

### ANIM-3 — C 层布局动画 + 优化（~2 周）

- [x] **A3.1** 注册表加入 C 层（§3.3 全表）  
- [x] **A3.2** `AppState`：`LayoutDirtySet` + 帧末 coalesced layout  
- [x] **A3.3** `mark_element_layout_dirty_subtree` 与 animation tick 集成  
- [x] **A3.4** `will_change` 字段 + compositor 快路径  
- [x] **A3.5** intrinsic `max-height` / `height` 动画：读 `used_layout`，**删除** Accordion 组件测量  
- [x] **A3.6** 备选：`grid-template-rows: 0fr→1fr` 作为 `accordion-down` 第二实现（`RUI_ACCORDION_GRID_FR=1` / `set_accordion_use_grid_fr`，默认仍走 max-height keyframes）  
- [x] **A3.7** 性能测试：Accordion 展开 layout 次数 ≤ 2；10 节点 padding 动画 1 layout/frame  
- [x] **A3.8** Gallery：layout 动画段（width/padding/gap）

### ANIM-4 — D 层文本 / 继承 + 作者面（~1–2 周）

- [x] **A4.1** `TargetSlot::TextStyle` / `InheritedText`；tick 写 `Element.text_style`  
- [x] **A4.2** 实现 `color` / `text-decoration-color` transition + keyframes  
- [x] **A4.3** `cascade_inheritable_text_style` 与 specified 文本槽协同（animated 不污染 inherit 源）  
- [x] **A4.4** `@keyframes` **CSS 文本解析** → `register_keyframes`（`from/to/百分比` + 属性名）  
- [x] **A4.5** `prefers-reduced-motion`：读 theme/media；动画 duration→0 或 disable  
- [x] **A4.6** 删除组件内重复 `dialog_css_anim` 式工厂，统一 `keyframes_anim_entry` + preset  
- [x] **A4.7** Gallery：文本 color 渐变、inherit 动画 demo

### ANIM-5 — 迁移、清理、DoD（~1 周）

- [x] **A5.1** **Accordion**：删 `measure_panel_natural_height`、`measured_height`、`exit_from_height`；仅 presence + class  
- [x] **A5.2** **Dialog/Modal**：seed 只写 specified opacity/scale；used 由引擎  
- [x] **A5.3** **Toast**：确认仍走 transition+motion；shadow/filter 走注册表  
- [x] **A5.4** 全库 `cargo test` + Gallery ANIM 页目视验收  
- [x] **A5.5** 更新 [`component_inventory.md`](../components/component_inventory.md) ANIM 列  
- [x] **A5.6** 废弃旧 API：`KeyframeStop` 字段 builder 标记 `deprecated` → 下一大版本删除  
- [x] **A5.7** 文档：§3 矩阵「✅」列与实现 1:1 对齐审计

### ANIM-6 — 延后项落地（MVP，不挡 v1 DoD）

- [x] **A6.1** Scroll/View `animation-timeline`：`scroll()` / `view()` 用最近滚动器 / 视口进度采样；不推进 elapsed，不泵 idle 帧
- [x] **A6.2** `animation-composition: add | accumulate`：按列表顺序与当前 Style（underlying）合成；标量/色/向量相加，transform 平移旋转相加、scale 相乘
- [x] **A6.3** `interpolate-size: allow-keywords`：transition 对 `auto` 尺寸读 `used_layout` 当端点；**`calc-size()`** 求值器（`size` 关键字）已接 keyframes / transition
- [x] **A6.4** `background-position` / `clip-path`（`inset` / `polygon` / `circle` / `ellipse`）进属性表 + 绘制（gradient UV 偏移 / AABB + SDF polygon）
- [x] **A6.5** 具名 `scroll-timeline` / `view-timeline`（`--name` 跨节点 + `animation-range`）

---

## 7. 冻结验收（原 ANIM v1 DoD）

1. **零组件测量**：Accordion / Collapsible 无 `measure_*_height`；intrinsic 全走 `used_layout`。  
2. **属性表**：§3 矩阵与实现 1:1（无 🔜）。  
3. **双引擎共用**：代表性属性（color / width / filter / clip-path polygon / border-radius）同时支持 transition + keyframes，同一插值函数（`anim_freeze`）。  
4. **Specified 不漂移**：动画 mid 不写入 specified；结束行为符合 `fill-mode`。  
5. **Text color**：`<Label>` / 文本叶 `transition: color` + `@keyframes` 可用。  
6. **负 delay**：`animation-delay: -500ms` 在 duration=1s 时 t=0 采样周期中点。  
7. **Reduced motion**：系统偏好开启时时长归零。  
8. **Regression**：Dialog / Toast / Carousel 现有测绿。

---

## 8. 风险与对策

| 风险 | 对策 |
|------|------|
| `Element` 增槽破坏 binary/layout | 侧车 `Rc<RefCell<AnimationPresent>>` 先落地，稳定后再 inline |
| Layout 动画掉帧 | coalescing + subtree；超预算降级 snap |
| Text 继承 + 动画冲突 | specified 文本 inherit 源与 present 分离；layout 前 cascade |
| 与 reactive_sync 写 Style 冲突 | reactive 只写 specified；tick 只写 present |
| 范围膨胀 | §1.2 硬边界；禁止静默扩面 |

---

## 9. 相关文档与代码

| 资源 | 路径 |
|------|------|
| 分层决策 | [`animation_and_motion.md`](animation_and_motion.md) |
| Layout 子树 | [`layout_subtree.md`](layout_subtree.md) |
| Damage | [`damage.md`](damage.md) |
| 引擎现状 | `crates/rui/src/style/{timing,waapi,keyframes,animation,animatable,element_style,clip_path}.rs` |
| 冻结契约测 | `crates/rui/src/crate_tests/style/anim_freeze.rs` |
| Accordion | `crates/rui/src/components/accordion.rs`（无组件测高） |
| 文本继承 | `layout_tree.rs` → `cascade_inheritable_text_style` |

---

## 10. 变更记录

| 日期 | 说明 |
|------|------|
| 2026-08-29 | 初版：三问结论 + 属性矩阵 + ANIM-0~6 一次性 To-Do |
| 2026-09-05 | ANIM-0～5 落地：Specified/Used 双槽、`PropertyId` 单轨、`steps()`、`@keyframes` 文本解析、`used_layout`、Accordion/Collapsible 去测高、文本 color transition、reduced-motion。ANIM-6 仍延后。 |
| 2026-09-05 | A3.6：`grid-template-rows` fr 插值 + Accordion `0fr→1fr` 第二路径（feature flag）。 |
| 2026-09-05 | ANIM-6 延伸：完整 `polygon`/`circle`/`ellipse` clip-path、具名 scroll/view-timeline、`calc-size()`。 |
| 2026-09-05 | **冻结**：§3 追上实现；`ClipPathTween` 双引擎同插值；独立 `scale`/`rotate` transition；色 transition 改走 `lerp_text_rgba`；`anim_freeze` 契约测。 |
| 2026-09-05 | 三层动画模型：引擎 `Timing` + spring、声明式 `spring()`、编程式句柄子集（丢掉 DocumentTimeline / Promise / 编程面 composite）。手册 §1.3。 |
