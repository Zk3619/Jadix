# RUI 类 CSS 盒模型（现状与契约）

> **文档地位**：布局盒模型的**事实手册**（对齐 CSS / Tailwind 心智的子集）。  
> **执行源**：完善工作包见 [`AUTHORITY.md`](AUTHORITY.md) **WP-2.9**。  
> **实现锚点**：`style/element_style.rs` · `layout/dimension.rs` · `core/tree.rs`（`m`/`p`/`border`/`content_box`）· `layout/layout_tree.rs`（`to_taffy_style` → 布局 / 绘制）· `crate_tests/layout/percent_content_box.rs`。  
> **日期**：2026-08-12  

交叉：[`layout_subtree.md`](layout_subtree.md) · [`color.md`](color.md)（`outline` ≠ 盒模型 border）· [`damage.md`](damage.md)

---

## 0. 一句话

RUI 用 **Taffy** 做 Flex/Grid 盒模型：默认 **`box-sizing: border-box`**（同 Tailwind reset）；百分比尺寸相对**父级 content box**；`outline` / 阴影 / translate·rotate·scale **只绘制、不进布局**。  
工程级差距不在「换布局引擎」，而在 **边框能力、单位语义、测宽叶纪律、命中/裁剪边一致性** 的收口与可测契约。

---

## 1. 管道（实现 = 规范）

```text
  Builder / ui! / utility class
            │  IntoDimension · IntoLengthPercentage
            ▼
  Style { size, min/max, padding, margin, border_*, box_sizing, … }
            │  Style::to_taffy_style()
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
| 尺寸令牌 | `px` / `rem` / 无后缀≈rem / `%` / `vw`·`vh`·`dvh`…（后四者现映射为根上的 Percent） |
| 盒模型参与布局 | `width/height`、`min/max`、`padding`、`margin`（含 `auto`）、**均匀** `border`（`border_style ≠ None` 且 `width > 0`）、`gap`、`inset`、`box_sizing`、`aspect_ratio` |
| 仅绘制 | `outline_*`、`box-shadow`、`opacity`、`translate` / `rotate` / `scale`、`backdrop_blur`（遮罩等） |
| 默认 display | `Flex`（非 block 流）；**不做**块级 margin collapse |

---

## 2. 已落地契约（可依赖）

### 2.1 `box-sizing`

| 值 | 入口 | 语义 |
|----|------|------|
| **BorderBox（默认）** | `Style::default` | `width`/`height` 含 padding + border（Tailwind `box-border`） |
| **ContentBox** | `.content_box()` | 测宽为内容宽；padding 加在外侧。文案叶自带水平 padding + `text-align:center` 时必须用（Tabs chip 回归） |

缺：公开 `.border_box()`（虽默认已是）；utility `box-border` / `box-content` 未进 `rui_style_tokens`。

### 2.2 百分比 vs 包含块

已测锁定（`percent_content_box`）：

- 子 `width: 100%` = 父 **content** 宽 = 父 border-box − 父 padding − 父 border。  
- BorderBox 子项自身再 `px` 时，外缘宽仍等于父 content（padding 吃在内部）。  
- 高度同理。  
- Flex 行内 `w(100%)` + 兄弟项会抢主轴——属 CSS 语义，文档化为 CardAction 类脚枪。

### 2.3 Margin / Padding / Gap / Inset

- Margin：`m` / `mx` / `my` / `mt`… → `LengthPercentageAuto`（支持 `auto`）。  
- Padding：`p` / `px` / `py` / `pt`… → `LengthPercentage`（支持 `%`）。  
- Gap：`gap` / `gap_x` / `gap_y`。  
- Absolute：`inset` / 单边 inset；`position_fixed` 为 Absolute + 忽略祖先滚动的仿真。

### 2.4 Border vs Outline

| | Border | Outline |
|--|--------|---------|
| 布局 | 写入 Taffy（均匀四边） | **否** |
| 绘制 | SDF 矩形描边 | 同管线外侧描边 |
| API | `border` / `border_width` / `border_style` / `border_color` | `outline` / `outline_width` / … |
| 产品命名 | 组件 `variant="outline"` = **border 观感**，见 [`color.md`](color.md) | 焦点环多用 shadow/`ring`，勿与 variant 混淆 |

当前 **仅均匀边框宽度**（四边同一 `border_width`）；无 `border-left` 等分边 API。

### 2.5 命中与背景

- 指针命中：`layout_bounds`（border-box 矩形），**未**再扣成 content-box。  
- 背景/主边框绘制：相对 border-box；滚动裁剪与边框粘边有专项测（`scroll_clip_border`）。

---

## 3. 缺口与债务（完善范围）

| ID | 缺口 | 优先级 | 说明 |
|----|------|--------|------|
| **BM1** | 盒模型契约文档化不足 | P1 | 分散在 `element_style` 模块注释与测例；缺单一手册（本文）与 Gallery/文档站入口 |
| **BM2** | 无 `box-border` / `box-content` utility + `.border_box()` | P1 | DX 与 Tailwind 心智对齐；默认已是 BorderBox |
| **BM3** | 测宽叶 + padding 的 ContentBox 纪律未系统审计 | P1 | Tabs 已修；Button/Input/Label/Badge 类「单节点文案+padding」易再踩 BorderBox 坑 |
| **BM4** | `em` 相对根 16px，非元素自身 `font-size` | P1 | padding/margin/`IntoF32` 与 Web 局部 em 不一致 |
| **BM5** | `vw`/`vh`/`dvh` → 布局根 Percent，非真窗口视口 | P2 | 嵌套 Portal / Dialog 与 CSS 视口语义分叉；需文档诚实 + 可选真视口 API |
| **BM6** | 分边 border（left/right/top/bottom 不同宽/色） | P2 | 产品偶需（左侧强调条）；Taffy 支持 Rect，Style 需扩字段 + 绘制 |
| **BM7** | 命中 / 裁剪 / 背景边 vs padding edge 矩阵测不足 | P1 | 补：overflow+padding、border 命中、outline 不扩大 hit |
| **BM8** | 块级 margin collapse / 完整 CSSOM | **不做** | 默认 Flex；禁区见纲领（不实现完整 CSS） |

---

## 4. 明确不做

- 换掉 Taffy 或自研盒模型引擎。  
- 完整 CSSOM、`box-decoration-break`、`border-image`、多值复杂 `margin` 简写解析器。  
- 在 Flex/Grid 主路径上模拟 block margin collapse。  
- 让 `outline` / `box-shadow` 参与布局尺寸。

---

## 5. 验收锚点（已有 + 目标）

**已有**

```text
cargo test -p rui --lib percent_content_box
cargo test -p rui --lib scroll_clip_border
# Tabs chip ContentBox：components/tabs 内测
```

**目标（WP-2.9）**

- [ ] 本文 + AUTHORITY 勾选同步  
- [ ] utility / Builder：`box-border`·`box-content` / `.border_box()`  
- [ ] 测宽叶 ContentBox 审计表（至少稳定面文本控件）  
- [ ] 命中与 outline 不占命中的库测  
- [ ] （可选 P2）分边 border MVP 或明确 Backlog  

---

*修订时同步 [`AUTHORITY.md`](AUTHORITY.md) WP-2.9；争议以 Taffy + 本文契约为准，禁止静默改默认 BorderBox。*
