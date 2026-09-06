# RUI 颜色方案

> **文档地位**：**色空间、混色数学、语义变体色**的策略契约（不含 Theme 运行时与 `theme.toml`）。主题 / 明暗 / 多预设见 [`theme.md`](theme.md)。Display P3 / 色彩管理 / HDR 见 [`color_advanced.md`](color_advanced.md)。变体词汇见 [`component_variants.md`](../components/component_variants.md)。  
> **冻结状态**：**未冻结**（2026-09-06；`color_freeze` CI 已接入）。纠错可改；策略变更须改本文。  
> **实现锚点**：`style/color_space.rs` · `style/color.rs` · `style/semantic_tone.rs` · `renderer/rounded_rect.wgsl` · `crate_tests/style/color_freeze.rs`。

> **从属**：[COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §4.3（外观）。组件内禁止硬编码与 Theme 无关的色值；语义名解析依赖 [`theme.md`](theme.md) 中的 `DesignTokens`。

本仓库已落地的 UI **颜色数学**是 **display-referred sRGB LDR**（§1–§2 / §A–§B）；**主题令牌表**见 [`theme.md`](theme.md)。广色域与 HDR **未实现**，设计方案在 [`color_advanced.md`](color_advanced.md)，不改变本文默认输出编码。

## 1. OKLab / OKLCH 作为设计令牌空间

- **用途**：主题录入、设计稿、与 **CSS Color 4 / Tailwind v4 / shadcn/ui v4**（`oklch(...)`）对齐；感知均匀插值。
- **实现**：[`theme.rs`](../../crates/rui/src/style/theme.rs) 中 `DesignTokens` 由 OKLCH 经 `oklch_to_display_srgba` 转为 **display sRGB**（`[f32; 4]`），再供 `parse_color` / 渲染使用（见 [`theme.md`](theme.md)）。
- **说明**：GPU 管线仍以 **display sRGB + alpha** 为主。
- **sRGB 交换链**：swapchain / offscreen 多为 `*Srgb`。wgpu 要求片元输出 **线性光**再编码。`rounded_rect.wgsl` 写出前 `linearize`；否则二次编码（`#333`→`#7B`）。勿用改 token 掩盖此问题。

## 2. 线性 sRGB 作为合成与插值空间

- **用途**：CPU 上 **lerp**、光强线性运算。
- **实现**：`linearize_display_srgb`、`encode_display_srgb`、`lerp_display_srgb_linear`。
- **说明**：swapchain sRGB 输出的硬件混合 vs 引擎手算混色须区分；后者以本模块为准。

## 3. 广色域与 HDR

未实现。令牌与交换链仍在 **sRGB LDR**。设计方案（P3-A/B/C、HDR-A/B/C、`DisplayTransform`、默认像素不变）见 [`color_advanced.md`](color_advanced.md)。落地前 transition / `color-mix` 走下文 §B。视频 / 动态纹理作为 HDR **媒体叶**的边界见 [`media.md`](media.md)，不在本文扩播放器。

---

## A. 色空间与默认令牌来源（已实现）

- shadcn v4 `@theme inline` 语义色定义在 [`theme.rs`](../../crates/rui/src/style/theme.rs)；**表结构与运行时**见 [`theme.md`](theme.md) §1。
- 转换：`oklch_to_display_srgba`（`palette::Oklch` → `Srgb`）。

## B. 线性混合（已实现）

- **线性 lerp**：`lerp_display_srgb_linear` — 动画、渐变、CPU 混色。
- **CSS 式混色**：`mix_display_srgb_in_oklab` — 接近 `color-mix(in oklab, ...)`。
- **半透明直通道**：仅改 `alpha`、GPU blend 的路径保持原样；CPU 两色合成用上式。

---

## 语义变体色（`semantic_tone.rs`）

**产品 `variant` 语气色**与 **Theme `chart-*`** 分离，由 [`semantic_tone.rs`](../../crates/rui/src/style/semantic_tone.rs) 管理（主题边界见 [`theme.md`](theme.md) §1）：

| 语气 | accent 前景 | 边框（非填充） | 阴影色相 | 默认图标 |
|------|-------------|----------------|----------|----------|
| `info` | `blue-600` | `blue-300` | `blue-500` | `info` |
| `success` | `green-600` | `green-300` | `green-500` | `circle-check` |
| `warning` | `orange-600` | `orange-300` | `orange-500` | `circle-alert` |
| `destructive` | theme `destructive` | `red-300` | `red-500` | `circle-x` |

**消费方**：Button soft tint、Alert、Badge、Toast、Bubble 等。不得单独硬编码 `green-600` / `chart-2` 作 success。

---

## 组件库选型：令牌一致 vs 物理混合？

**更推荐：以「令牌一致」为主，在需要处叠加「物理/感知混合工具」。**

- shadcn 首要对齐 **CSS 变量 / Tailwind / 对比度**；令牌 OKLCH 与官方同源（[`theme.md`](theme.md)）。
- 物理混合用于渐变、动画插值、CPU 多层合成；**设计对齐用 Oklab mix，光强用 linear lerp**。

---

## 与 shadcn / 浏览器：`border`、`outline`、焦点环

- **`variant="outline"`** = Tailwind **`border` 类**（盒模型），不是 CSS `outline`。
- **焦点环**：Web 上 `ring-*` 多为 **box-shadow**；RUI 用 `parse_color("border")` + `Style::shadow_*` + theme `ring`（几何见 [`theme.md`](theme.md) §5）。

## `transition` 与按钮 hover（RUI）

- **Button** hover 由 **`transition-colors`**（~150ms）插值；目标色来自 Theme / `parse_color`。
- **Switch** 同理 `transition-transform` / `transition-colors`。
- 变体采样：`primary/80`、`secondary` + `foreground 5%` mix、destructive `/10`→`/20`。

换肤验收见 [`theme.md`](theme.md) §6。

---

## CI 契约

| 门禁 | 命令 |
|------|------|
| 颜色 + 主题相关冻结 | `cargo test -p rui --lib color_freeze` |
| 色空间单测 | `cargo test -p rui --lib color_space` |
| 裸 RGBA 扫描（本地，WP-STY.1） | `scripts/check-token-colors.ps1` / `.sh` |

策略变更（混色函数、`semantic_tone` 表）须改本文；**令牌默认 / patch merge** 须改 [`theme.md`](theme.md)；**P3 / HDR / 输出编码** 须改 [`color_advanced.md`](color_advanced.md)。

---

## 相关代码

| 模块 | 说明 |
|------|------|
| `style/color_space.rs` | OKLCH 转 sRGB、linearize/encode、lerp、Oklab mix |
| `style/color.rs` | `parse_color`、语义名、`color_mix_over_background` |
| `style/semantic_tone.rs` | 跨组件语气色 |
| `style/theme.rs` | 默认 `DesignTokens`（见 [`theme.md`](theme.md)） |
| `style/animation.rs` | 色 transition：`lerp_text_rgba` |
| `components/button.rs` | hover transition-colors |
| （未实现） | P3 / HDR 设计见 [`color_advanced.md`](color_advanced.md) |
