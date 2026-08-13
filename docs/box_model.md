# RUI 类 CSS 盒模型（现状与契约）

> **文档地位**：布局盒模型的**事实手册**（对齐 CSS / Tailwind 心智的子集）。  
> **执行源**：完善工作包见 [`AUTHORITY.md`](AUTHORITY.md) **WP-2.9**（2026-08-13 签署）。  
> **实现锚点**：`style/element_style.rs` · `layout/dimension.rs`（`CssDim` / `UnitCtx`）· `core/tree.rs`（`m`/`p`/`border`/`border_box`/`gap`/`*-inline`/`*-block`）· `layout/layout_tree.rs`（`to_taffy_style_with` → 布局 / 绘制）· `rui_style_tokens`（`box-border` / `box-content`）· `crate_tests/layout/percent_content_box.rs` · `crate_tests/layout/box_model_contract.rs`。  
> **日期**：2026-08-13  

交叉：[`layout_subtree.md`](layout_subtree.md) · [`color.md`](color.md)（`outline` ≠ 盒模型 border）· [`damage.md`](damage.md) · [`component_inventory.md`](component_inventory.md)

---

## 0. 一句话

RUI 用 **Taffy** 做 Flex/Grid 盒模型：默认 **`box-sizing: border-box`**（同 Tailwind reset）；百分比尺寸相对**父级 content box**；`outline` / 阴影 / translate·rotate·scale **只绘制、不进布局、不扩大命中**。  
不追求完整 CSSOM；分边 border（`border-left` 等）已落地（B15）。

---

## 1. 管道（实现 = 规范）

```text
  Builder / ui! / utility class
            │  IntoCssDim · IntoCssRectPair（vw/em/calc 延迟）
            ▼
  Style { size, min/max, padding, margin, border_*, box_sizing, unit_specs, … }
            │  Style::to_taffy_style_with(UnitCtx { viewport, font_size })
            ▼
  taffy::Style  →  compute  →  Element.layout_bounds (border-box 原点+尺寸)
            │
     ┌──────┼──────────┐
     ▼      ▼          ▼
   绘制背景/边框   outline/阴影   命中检测（layout_bounds）
   (border 参与)   (不进布局)
```

| 层 | 职责 |
|----|------|
| 尺寸令牌 | `px` / `rem` / 无后缀≈rem / `%` / **局部 `em`** / **真 `vw`·`vh`·`dvh`·`dvw`…** / `calc(A ± B)`（无父级 `%`） |
| 盒模型参与布局 | `width/height`、`min/max`、`padding`、`margin`（含 `auto`）、**分边** `border`（Taffy `Rect`）、`gap`/`row-gap`/`column-gap`、逻辑 `*-inline`/`*-block`（可双值）、`inset`、`box_sizing`、`aspect_ratio` |
| 仅绘制 | `outline_*`、`box-shadow`、`opacity`、`translate` / `rotate` / `scale`、`backdrop_blur`（遮罩等） |
| 默认 display | `Flex` / `Grid`（非 CSS block 流）；**不做** `display:inline` / 块级 margin collapse |

---

## 2. 已落地契约（可依赖）

### 2.1 `box-sizing`

| 值 | 入口 | 语义 |
|----|------|------|
| **BorderBox（默认）** | `Style::default` · `.border_box()` · utility `box-border` | `width`/`height` 含 padding + border（Tailwind `box-border`） |
| **ContentBox** | `.content_box()` · utility `box-content` | 测宽为内容宽；padding 加在外侧。文案叶自带水平 padding + `text-align:center` 时必须用（Tabs chip 回归） |

`w(100) + px(20)`：BorderBox 外缘仍 100；ContentBox 外缘 140。库测 `box_model_contract`。

### 2.2 百分比 vs 包含块

已测锁定（`percent_content_box`）：

- 子 `width: 100%` = 父 **content** 宽 = 父 border-box − 父 padding − 父 border。  
- BorderBox 子项自身再 `px` 时，外缘宽仍等于父 content（padding 吃在内部）。  
- 高度同理。  
- Flex 行内 `w(100%)` + 兄弟项会抢主轴——属 CSS 语义，文档化为 CardAction 类脚枪。

### 2.3 Margin / Padding / Gap / Inset / 逻辑属性

- Margin：`m` / `mx` / `my` / `mt`… → `LengthPercentageAuto`（支持 `auto`）。  
- Padding：`p` / `px` / `py` / `pt`… → `LengthPercentage`（支持 `%`）。  
- Gap：`gap` / `gap_x` / `gap_y` / `column_gap` / `row_gap`（Flex 与 Grid 均写入 Taffy `gap`）。  
- 逻辑（LTR 水平书写）：`padding-inline`/`padding-block`、`margin-inline`/`margin-block`、`inset-inline`/`inset-block`、`inline-size`/`block-size`；双值 `"10px 20px"` = start/end。  
- Absolute：`inset` / 单边 inset；`position_fixed` 为 Absolute + 忽略祖先滚动的仿真。

### 2.4 Border vs Outline

| | Border | Outline |
|--|--------|---------|
| 布局 | 写入 Taffy（分边 `Rect` TRBL） | **否** |
| 绘制 | 均匀 → SDF 描边；非均匀 → 四边填充条（角归上下边；圆角/独立旋转为近似） | 同管线外侧描边 |
| 命中 | **是**（落在 `layout_bounds` 内） | **否**（描边落在 border-box 外则 miss） |
| API | `border` / `border_width`（1–4 值）/ `border_left`… / `border_*-color` / `border_style` | `outline` / `outline_width` / … |
| 产品命名 | 组件 `variant="outline"` = **border 观感**，见 [`color.md`](color.md) | 焦点环多用 shadow/`ring`，勿与 variant 混淆 |

分边宽度写入 `Style.border_width_sides`（TRBL）；`border_width` 仍为均匀简写或四边 **max**（旧 `> 0` 检查兼容）。`ui!`：`borderLeft` / `borderTopWidth` / `borderWidth="4px 8px 12px 16px"`。

### 2.5 命中、裁剪与背景

- 指针命中：`layout_bounds`（border-box 矩形），**未**再扣成 content-box；border 外缘可点，outline 外扩不可点。  
- 背景/主边框绘制：相对 border-box。  
- **overflow 裁剪边 = 父 border-box**：父级 padding **不**内缩 clip 矩形（子项可画进 padding 带，但不能画出父 `layout_bounds`）。滚动裁剪与边框粘边见 `scroll_clip_border`。  
- 库测：`box_model_contract`（outline hit / border hit / overflow+padding clip）。

### 2.6 单位（布局期解析）

| 单位 | 语义 |
|------|------|
| `px` | 逻辑像素 |
| `rem` / 无后缀数字 | × 根 **16px** |
| `em` | 相对**本节点** `text_style.size`（cascade 后；Builder/RSX 经 `CssDim`） |
| `%` | 父 **content box**（Taffy Percent） |
| `vw` / `dvw` / `svw` / `lvw` | 当前 `LayoutTree` 视口宽（`build_and_layout` 的 available width） |
| `vh` / `dvh` / `svh` / `lvh` | 当前布局视口高 |
| `calc(A ± B)` | `px`/`rem`/`em`/`vw`/`vh`/`%` 加减。`calc(100% - Npx)` 相对**父 content box** 烘焙成 Length（Taffy 无 calc）；父轴不定高/宽时该轴回落 `auto` |
| `auto` | Taffy Auto（margin 水平居中等） |

`IntoDimension` 仍把 `vw`/`vh` 映成 Percent、把 `em` 当根 16px（bind 旧路径 / `IntoF32`）；**盒模型 setter 走 `IntoCssDim`**，在 `to_taffy_style_with` 用视口 + 元素字号落地。  
窗口缩放走 LayoutChanged **fast** 路径：根尺寸更新后会再烘焙整树 `vw`/`vh`/`em`/`calc`（`relayout_deferred_units_if_needed`），否则 Taffy 里仍是旧 Length。

---

## 3. 缺口与债务

| ID | 缺口 | 优先级 | 状态 |
|----|------|--------|------|
| **BM1** | 手册 + Gallery / 文档站入口 | P1 | ☑ WP-2.9.1 |
| **BM2** | `box-border` / `box-content` + `.border_box()` | P1 | ☑ WP-2.9.2 |
| **BM3** | 测宽叶 + padding ContentBox 纪律 | P1 | ☑ 附录 A；Tabs ContentBox，其余拆叶或显式宽 |
| **BM4** | 局部 `em`（相对本节点 `font-size`） | P1 | ☑ `em_padding_uses_element_font_size`；`IntoDimension`/`IntoF32` 仍为根 16px |
| **BM5** | 真视口 `vw`/`vh`/`dvh` | P2 | ☑ `vw_vh_dvh_resolve_against_layout_viewport`；嵌套 Portal 仍用当前 compute 视口 |
| **BM6** | 分边 border | P2 | ☑ B15 2026-08-13 |
| **BM7** | 命中 / overflow+padding 裁剪测 | P1 | ☑ `box_model_contract` |
| **BM8** | 块级 margin collapse / 完整 CSSOM | **不做** | 默认 Flex |

---

## 4. 明确不做

- 换掉 Taffy 或自研盒模型引擎。  
- 完整 CSSOM、`box-decoration-break`、`border-image`、多值复杂 `margin` 简写解析器。  
- 在 Flex/Grid 主路径上模拟 block margin collapse。  
- 让 `outline` / `box-shadow` 参与布局尺寸或命中矩形。

---

## 5. 验收锚点

```text
cargo test -p rui --lib percent_content_box
cargo test -p rui --lib box_model_contract
cargo test -p rui --lib scroll_clip_border
cargo test -p rui --lib em_resolves_against_root
cargo test -p rui --lib css_dim_vw_vh_em_calc
# Tabs chip ContentBox：components/tabs 内测
# Gallery：侧栏「布局」→「WP-2.9 / B15)」
```

WP-2.9 退出：2.9.1–2.9.7 ☑（含 B15 分边 border）。

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
| **Badge** | planned | — | — | 落地时按 Tabs 纪律：单节点 pad+center → ContentBox，否则拆叶 |

Gallery / 业务自建 chip：单节点文案 + padding + 居中 → `class="box-content"` 或 `.content_box()`。

---

*修订时同步 [`AUTHORITY.md`](AUTHORITY.md) WP-2.9；争议以 Taffy + 本文契约为准，禁止静默改默认 BorderBox。*
