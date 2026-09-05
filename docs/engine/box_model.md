# RUI 类 CSS 盒模型

> **文档地位**：布局盒模型的**事实手册**（[CSS Box Model](https://www.w3.org/TR/css-box-3/) / [Box Sizing](https://www.w3.org/TR/css-sizing-3/) 的已承诺子集 + Tailwind `border-box` 默认）。  
> **冻结状态**：**已冻结**（2026-09-05 子集）。纠错（行号、测试命令、Gallery 展示色）可改；策略变更须改本文，禁止静默改默认 BorderBox。  
> **执行源**：WP-2.9 已退出（[`AUTHORITY.md`](../AUTHORITY.md)，2026-08-13）。本文不再单开 WP。  
> **实现锚点**：`style/element_style.rs` · `layout/dimension.rs`（`CssDim` / `UnitCtx`）· `core/tree.rs`（`m`/`p`/`border`/`border_box`/`gap`/`*-inline`/`*-block`/`zoom`）· `layout/layout_tree.rs`（`to_taffy_style_with` → 布局 / 绘制）· `style/used_layout.rs`（动画 used 值）· `rui_style_tokens`（`box-border` / `box-content`）· `crate_tests/layout/percent_content_box.rs` · `crate_tests/layout/box_model_contract.rs` · `crate_tests/layout/box_model_gallery_colors.rs`。  
> **日期**：2026-09-05（Gallery 走查通过；展示色与 CI 收口）。

交叉：[`layout_subtree.md`](layout_subtree.md) · [`color.md`](color.md)（`outline` ≠ 盒模型 border）· [`damage.md`](damage.md) · [`animation_and_motion.md`](animation_and_motion.md) · [`component_inventory.md`](../components/component_inventory.md)

---

## 0. 一句话

RUI 用 **Taffy** 做 Flex/Grid 盒模型：默认 **`box-sizing: border-box`**（同 Tailwind reset）；百分比尺寸相对**父级 content box**；padding/margin 的 `%` 相对包含块**宽度**（CSS）；`outline` / 阴影 / translate·rotate·scale **只绘制、不进布局、不扩大命中**。  
不追求完整 CSSOM；不做块级 margin collapse。分边 border 已落地（B15）。

---

## 1. 管道（实现 = 规范）

```text
  Builder / ui! / utility class
            │  IntoCssDim · IntoCssRectPair（vw/em/calc 延迟；开放域，见 [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) §3.1.1）
            │  离散预设属性：链式字符串默认；可选预设枚举双收（variant / pointer_events / cursor …）
            ▼
  Style { size, min/max, padding, margin, border_*, box_sizing, zoom, unit_specs, … }
            │  Style::to_taffy_style_with(UnitCtx { viewport, font_size, cb_w/h })
            ▼
  taffy::Style  →  compute  →  Element.layout_bounds (border-box 原点+尺寸)
            │  UsedLayout 盖戳（动画 `auto` → used px）
     ┌──────┼──────────┐
     ▼      ▼          ▼
   绘制背景/边框   outline/阴影   命中检测（layout_bounds）
   (border 参与)   (不进布局)
```

| 层 | 职责 |
|----|------|
| 尺寸令牌 | `px` / `rem` / 无后缀≈rem / `%` / **局部 `em`** / **真 `vw`·`vh`·`dvh`·`dvw`…** / `calc(A ± B)`（无父级 `%` 时该轴回落 `auto`） / `fit-content`·`min-content`·`max-content`（Taffy 近似，见 §2.7） |
| 盒模型参与布局 | `width/height`、`min/max`、`padding`、`margin`（含 `auto` 与负值）、**分边** `border`（Taffy `Rect`）、`gap`/`row-gap`/`column-gap`、逻辑 `*-inline`/`*-block`（可双值，随 **`writing-mode` + `direction`** 映到物理边）、`inset`、`box_sizing`、`aspect_ratio`、**`zoom`（缩放布局尺寸）** |
| 仅绘制 | `outline_*` / `outline-offset`、`box-shadow`、`opacity`、`translate` / `rotate` / `scale` / `transform`、`backdrop_blur`、**`border-image`（九宫切，不改 `layout_bounds`）** |
| 默认 display | `Flex`（非 CSS block 流）；**不做** `display:inline` / 块级 margin collapse |

---

## 2. 已落地契约（可依赖）

### 2.1 `box-sizing`

| 值 | 入口 | 语义 |
|----|------|------|
| **BorderBox（默认）** | `Style::default` · `.border_box()` · utility `box-border` | `width`/`height` 含 padding + border（Tailwind `box-border`） |
| **ContentBox** | `.content_box()` · utility `box-content` | 测宽为内容宽；padding 加在外侧。文案叶自带水平 padding + `text-align:center` 时必须用（Tabs chip 回归） |

`w(100) + px(20)`：BorderBox 外缘仍 100；ContentBox 外缘 140。库测 `box_model_contract`。

CSS 规范默认是 `content-box`；RUI 与 Tailwind reset 一致，默认 `border-box`。**禁止静默改默认。**

### 2.2 百分比 vs 包含块

已测锁定（`percent_content_box` + `box_model_contract`）：

- 子 `width: 100%` / `height: 100%` / `min/max-*` / `"1/2"` = 父 **content** 盒（父 border-box − 父 padding − 父 border）。  
- BorderBox 子项自身再 `px` 时，外缘宽仍等于父 content（padding 吃在内部）。  
- **padding / margin 的 `%`（含 `pt`/`pb`/`mt`/`mb`）相对包含块宽度**，不是该边所在轴的高度。父 content 宽 400 时 `pt("10%")` = 40px，不是父高的 10%。包含块宽度未知（循环百分比）时该 `%` 烘焙为 **0**（CSS）。  
- Flex 行内 `w(100%)` + 兄弟项会抢主轴——属 CSS 语义，文档化为 CardAction 类脚枪。

绝对定位的 `inset` 百分比：左右相对包含块**宽**，上下相对包含块**高**（CSS）。`position: fixed` 为 Absolute + 忽略祖先滚动的仿真，百分比仍相对当前 compute 的父 content box，不是视觉视口。

### 2.3 Margin / Padding / Gap / Inset / 逻辑属性

- Margin：`m` / `mx` / `my` / `mt`… → `LengthPercentageAuto`（支持 `auto`、**负值**；负 margin 会拉近后续兄弟，RUI 不做 collapse）。  
- Padding：`p` / `px` / `py` / `pt`… → `LengthPercentage`（支持 `%`，见 §2.2）。  
- Gap：`gap` / `gap_x` / `gap_y` / `column_gap` / `row_gap`（Flex 与 Grid 均写入 Taffy `gap`）。`column-gap` `%` 相对 inline 轴，`row-gap` `%` 相对 block 轴。  
- 逻辑：`padding-inline`/`padding-block`、`margin-inline`/`margin-block`、`inset-inline`/`inset-block`、`inline-size`/`block-size`；双值 `"10px 20px"` = start/end。映射随 **`writing-mode` + `direction`**（`text_direction` 同时写 `Style.direction`）。`width` 永远是物理宽；垂直模式下 `inline-size` 映到 height。padding/margin `%` 仍相对包含块**宽度**。  
- Absolute：`inset` / 单边 inset；`position_fixed` 为 Absolute + 忽略祖先滚动；`position_sticky` 为 Relative + 滚动夹紧。

### 2.4 Border vs Outline vs border-image

| | Border | Outline | border-image |
|--|--------|---------|--------------|
| 布局 | 写入 Taffy（分边 `Rect` TRBL） | **否** | **否**（`outset` 只扩大绘制） |
| 绘制 | 均匀 → SDF 描边；非均匀 → 四边填充条（角归上下边；圆角/独立旋转为近似） | 同管线外侧描边；`outline-offset` 外扩/内缩绘制盒 | 九宫切贴图（`border_image_*`） |
| 命中 | **是**（落在 `layout_bounds` 内） | **否**（描边落在 border-box 外则 miss） | 不单独扩 hit |
| API | `border` / `border_width`（1–4 值）/ `border_left`… / `border_*-color` / `border_style` | `outline` / `outline_width` / `outline_offset` / … | `border_image_source` + slice/width/outset/repeat |
| 产品命名 | 组件 `variant="outline"` = **border 观感**，见 [`color.md`](color.md) | 焦点环多用 shadow/`ring`，勿与 variant 混淆 | 装饰边，不是 layout border |

分边宽度写入 `Style.border_width_sides`（TRBL）；`border_width` 仍为均匀简写或四边 **max**（旧 `> 0` 检查兼容）。`ui!`：`borderLeft` / `borderTopWidth` / `borderWidth="4px 8px 12px 16px"`。

### 2.5 命中、裁剪与背景

- 指针命中：`layout_bounds`（border-box 矩形），**未**再扣成 content-box；border 外缘可点，outline 外扩不可点。  
- 背景/主边框绘制：相对 border-box。无 `background-clip` / `background-origin` 切换（冻结为 border-box）。  
- **overflow 裁剪边 = 父 border-box**（有意不同于 CSS 默认 padding-box）：父级 padding **不**内缩 clip 矩形（子项可画进 padding 带，但不能画出父 `layout_bounds`）。滚动裁剪与边框粘边见 `scroll_clip_border`。  
- overflow 的 **AABB clip / cull 仍是轴对齐矩形**。非零 `border-radius` 时 `GpuClipPoly` 带 **SDF 圆角盒**（与填充同一 `sd_round_box`，不是 16 顶点折线）；`clip-path` 多边形优先于 overflow 圆角槽。嵌套两个圆角 overflow：只保留内层圆角 + 已求交 AABB（近似）。命中对 overflow 祖先在 AABB 之外还做 `rounded_rect_contains`。贴图 / glyphon 文本仍主要是 AABB scissor；SDF 实例走圆角槽 / poly。  
- `text-orientation: mixed` 的 upright CJK **不做**；垂直 `writing-mode` 一律 sideways 旋转。  
- 特例：原生 `<input>` 文本叶按 **padding-box** 裁（caret / CJK bearing），与全局 overflow 规则不同。  
- 库测：`box_model_contract`（outline hit / border hit / overflow+padding clip / overflow 圆角 clip / writing-mode 逻辑边）。

### 2.5.1 可折叠 clip 壳（Collapsible / Accordion）

Taffy 默认 `min-height: auto` 解析为 **min-content**（内容 + **padding**，不含 margin）。对 **`max-height` 动画** 的外壳：

| 层 | 职责 |
|----|------|
| **外层 clip 壳** | `overflow: hidden` + `min-h(0)`；**不要**放 padding / margin |
| **内层内容壳** | `px` / `pb` / `gap` 等 spacing |

引擎在 `@keyframes` 驱动 `max-height` 期间会把 `min-height` 临时钉为 `0`（[`animation_run_active` / `tick_element_animations`](../../crates/rui/src/style/animation.rs)），动画结束恢复 cascaded 值；**padding 仍应放在 inner**，否则 min-content 下限仍由 padding 抬高。

`UsedLayout`（`style/used_layout.rs`）在 collect 后盖戳 border-box / intrinsic，供动画把 `max-height: auto` 解成 used px。组件不要自己再测一遍。

**Margin** 在 border-box **外**，不参与自身 min-content，但 flex 列里 `height≈0` 的项若带 `margin-bottom` 仍会留下**外部空白**（RUI 不做 margin collapse）。间距优先：`gap` / `border-b` / inner padding。

参考：ShadCN `accordion-content` · [`animation_and_motion.md`](animation_and_motion.md) §2.1.1。

### 2.6 单位（布局期解析）

| 单位 | 语义 |
|------|------|
| `px` | 逻辑像素 |
| `rem` / 无后缀数字 | × 根 **16px** |
| `em` | 相对**本节点** `text_style.size`（cascade 后；Builder/RSX 经 `CssDim`） |
| `%`（`width`/`height`/`min`/`max`） | 父 **content box** 对应轴（Taffy Percent） |
| `%`（`padding`/`margin`，四边） | 父 **content 宽**（CSS Box Model；烘焙成 Length） |
| `vw` / `dvw` / `svw` / `lvw` | 当前 `LayoutTree` 视口宽（`build_and_layout` 的 available width） |
| `vh` / `dvh` / `svh` / `lvh` | 当前布局视口高 |
| `calc(A ± B)` | `px`/`rem`/`em`/`vw`/`vh`/`%` 加减。`calc(100% - Npx)` 相对**父 content box** 烘焙成 Length（Taffy 无 calc）；父轴不定高/宽时该轴回落 `auto` |
| `auto` | Taffy Auto（margin 水平居中等） |
| `fit-content` / `min-content` / `max-content` | 见 §2.7 |

`IntoDimension` 仍把 `vw`/`vh` 映成 Percent、把 `em` 当根 16px（bind 旧路径 / `IntoF32`）；**盒模型 setter 走 `IntoCssDim`**，在 `to_taffy_style_with` 用视口 + 元素字号 + 包含块落地。  
窗口缩放走 LayoutChanged **fast** 路径：根尺寸更新后会再烘焙整树 `vw`/`vh`/`em`/`calc` 与 padding/margin `%`（`relayout_deferred_units_if_needed`），否则 Taffy 里仍是旧 Length。  
嵌套 Portal 的 `vw`/`vh` 仍用**当前 compute 视口**，不是 Portal 宿主盒。

### 2.7 内容关键字与 `zoom`

- `w("fit-content")` / `min-content` / `max-content`（及 Tailwind `fit` / `min` / `max`）：Taffy 0.13 flex `Dimension` 不能携带这些 tag，布局落成 **`Auto` + 交叉轴 hug**（`align-self: start` 近似）。主轴上与真 CSS `fit-content` 不完全等价。库测：`fit_content_width_hugs_in_stretch_column`。  
- **`zoom`**：CSS 式放大系数，**进布局**（缩放 size / padding / margin / border 的 Length）。默认 `1`。`.zoom(2)` 把 BorderBox `w(100)` 变成外缘 200。不是 `transform: scale`（后者只绘制）。

---

## 3. 缺口与债务

WP-2.9 BM1–BM7 已退出。下表是 **2026-09-05 冻结面** 仍存在、且已定性的项。

| ID | 缺口 | 优先级 | 状态 |
|----|------|--------|------|
| **BM1–BM7** | 手册 / utility / 测宽叶 / em / vw / 分边 border / 命中裁剪 | P1–P2 | ☑ WP-2.9 |
| **BM8** | 块级 margin collapse / 完整 CSSOM | **不做** | 默认 Flex |
| **BM9** | padding/margin `%` 相对包含块**宽** | P1 | ☑ 2026-09-05；`to_box_edge_length_percentage` |
| **BM10** | 逻辑属性随 `direction`/`writing-mode` 翻转 | P1 | ☑ 2026-09-05；Taffy 无 writing-mode，bake 期映射；`text-orientation: mixed` 不做 |
| **BM11** | `background-clip` / `background-origin` | **不做** | 背景相对 border-box |
| **BM12** | overflow 裁剪边改为 CSS padding-box | **不做** | 保持 border-box AABB；**圆角 clip 已落地**（poly + 命中） |
| **BM13** | `scroll-padding` / `scroll-margin` / `overflow-clip-margin` / `margin-trim` | **不做** | Box Model L4 / Overflow L3 加深 |
| **BM14** | 真 `fit-content` 主轴（fork Taffy 或等上游） | **不做** | 交叉轴 hug 近似 |

---

## 4. 相对 CSS 的冻结差异（明确不做 / 有意不同）

对照 CSS Box Model L3 + Box Sizing L3 + Overflow L3。下列差异**就是冻结面**：改其中任一项须先改本文。

| CSS | RUI |
|-----|-----|
| 默认 `box-sizing: content-box` | 默认 **`border-box`**（Tailwind） |
| `display: block` + margin collapse | **Flex/Grid**；相邻 margin **相加**，不 collapse |
| `display: inline` / `inline-block` | **不做** |
| overflow clip = **padding-box**（可用 `overflow-clip-margin` 外扩） | clip = 父 **border-box**；无 `overflow-clip-margin` |
| `overflow: hidden` + `border-radius` → 圆角裁剪 | AABB cull + SDF **圆角盒**（嵌套取内层圆角；贴图/glyphon 仍以 AABB scissor 为主） |
| `background-clip` / `background-origin` | 固定相对 **border-box** |
| 逻辑属性随 `direction` / `writing-mode` | bake 期映射；垂直模式 flex 轴重映射；`text-orientation: mixed` **不做** |
| 绝对定位包含块 = positioned 祖先 **padding edge** | 百分比相对父 **content box**；`fixed` 为仿真 |
| `fit-content` / `min-content` / `max-content` 为确定尺寸 | Taffy `Auto` + 交叉轴 hug |
| `box-decoration-break` | **不做** |
| 完整 CSSOM / 多值复杂 `margin` 简写解析器 | **不做** |
| `outline` / `box-shadow` 不进布局、不扩命中 | **相同**（承诺） |
| `border-image` 可影响 border-image-width 布局 | **仅绘制**，不改 `layout_bounds` |
| 换 Taffy / 自研盒模型引擎 | **不做** |

---

## 5. 验收锚点

```text
cargo test -p rui --lib percent_content_box
cargo test -p rui --lib box_model_contract
cargo test -p rui --lib writing_mode
cargo test -p rui --lib rounded_rect
cargo test -p rui --lib scroll_clip_border
cargo test -p rui --lib em_resolves_against_root
cargo test -p rui --lib css_dim_vw_vh_em_calc
cargo test -p rui --lib padding_margin_percent_bakes_against_containing_block_width
cargo test -p rui --lib box_model_gallery_colors
# Tabs chip ContentBox：components/tabs 内测
# Gallery：侧栏「布局」→「WP-2.9 / B15)」盒模型 + 「writing-mode（CSS）」
# CI：`.github/workflows/ci.yml` step「Box model contract (freeze)」
```

WP-2.9 退出：2.9.1–2.9.7 ☑（含 B15 分边 border）。2026-09-05 契约加深与展示色收口不重开 WP。

---

## 6. 已冻结（2026-09-05 子集）

与 [`stack_and_element_model.md`](stack_and_element_model.md) 同标准：**纠错可改；策略变更须改本文；开放项已收口或写入 §4。**

Gallery 走查已通过。展示层不得用浅色 zinc 的 `secondary`/`accent` 当相邻洋葱层（与 `muted` 同色，库测 `box_model_gallery_colors`）。

---

## 附录 A · 稳定面「单节点文案 + padding」审计（BM3）

口径：同一节点既 `text` 又非零 padding，且 **auto 测宽** 或 **`text-align:center`** 时，BorderBox 会把测宽当成边框宽、padding 吃掉内容区。对策：`.content_box()` / `box-content`，或 **拆成 chrome 容器 + 文案叶**。

| 控件 | 桶 | 结构 | box-sizing | 结论 |
|------|----|------|------------|------|
| **TabsTrigger** | verified | **单节点**文案 + `px/py` + `text-center` | **ContentBox** | 已修；勿改回 BorderBox |
| **Button** | verified | chrome 容器 + `build_text_node` 叶 | BorderBox | 拆叶，OK |
| **Input / Textarea** | verified | chrome + 内部文本叶 | BorderBox | 拆叶，OK |
| **Toggle** | experimental | `hstack` + 文案叶 | BorderBox | 拆叶，OK |
| **Checkbox** indicator | verified | 固定 `w/h` 图标 + center，无额外 pad | BorderBox | 固定盒，OK |
| **Radio** | verified | 控件 + caption `Label` | BorderBox | 拆叶，OK |
| **SelectTrigger** | verified | Button chrome + Value/chevron 子 | BorderBox | 拆叶，OK |
| **SelectLabel** | verified | Label + `px/pt/pb` + **`w(100%)`**，左对齐 | BorderBox | 显式宽 + 非 center，OK |
| **SelectItem** | verified | 行容器 + 文案叶 | BorderBox | 拆叶，OK |
| **Label** | verified | 文案叶；`.p()` API 存在，默认无 pad | BorderBox | 默认 OK；用户再加 **padding + center / auto 宽** → 必须 `.content_box()` |
| **Card 系列** | verified | 区容器 padding，标题为 Label 子 | BorderBox | 拆叶，OK |
| **Badge** | experimental | chrome 容器 + 文案/图标叶 | BorderBox | 已落地；拆叶，OK |
| **AspectRatio** | experimental | 比例容器（relative + overflow + aspect_ratio） | BorderBox | 已落地；子节点 absolute 铺满；Image 亦可直接 `aspect_ratio` 属性 |

Gallery / 业务自建 chip：单节点文案 + padding + 居中 → `class="box-content"` 或 `.content_box()`。

---

*修订时同步 [`AUTHORITY.md`](../AUTHORITY.md) WP-2.9 备注；争议以 Taffy + 本文契约为准，禁止静默改默认 BorderBox。*
