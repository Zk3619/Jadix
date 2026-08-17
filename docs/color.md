# RUI 颜色方案

> **从属**：[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §4.3（外观）与纲领样式层。组件内禁止硬编码与 Theme 无关的色值。

本仓库的 UI 颜色分四层理解；实现上 **A（令牌）** 与 **B（混合）** 已落地在 `crates/rui/src/style/color_space.rs` 与默认主题中。

## 1. OKLab / OKLCH 作为设计令牌空间

- **用途**：主题、设计稿、与 **CSS Color 4 / Tailwind v4 / shadcn/ui v4**（`oklch(...)`）对齐；在感知上均匀的插值与变亮变暗。
- **实现**：默认 `DesignTokens` 由 **OKLCH** 数值经 `palette` 转为 **display sRGB**（`[f32; 4]`，0–1），再供 `parse_color` / 渲染使用。
- **说明**：GPU 与现有管线仍以 **display sRGB + alpha** 为主，避免一次性改动整栈。
- **sRGB 交换链**：swapchain / offscreen 目标多为 `*Srgb`。wgpu 要求片元输出 **线性光**，写入时再编码。矩形着色器（`rounded_rect.wgsl`）在写出前对 display 色做 `linearize`；否则会出现二次编码（如 `#333`→`#7B`、`#F5F5F5`→`#FBFBFB`）。主题令牌与 `oklch_to_display_srgba` / `color_mix_over_background` 在 CPU 侧与 Web 一致，勿用改 token 掩盖此问题。

## 2. 线性 sRGB 作为合成与插值空间

- **用途**：在 **CPU 上**对两色做 **lerp**、或需要「光强线性」的运算时，应在线性 sRGB 下算，再编码回 display sRGB。
- **实现**：`linearize_display_srgb`、`encode_display_srgb`、`lerp_display_srgb_linear`。
- **说明**：片段着色器经 swapchain **sRGB 格式**输出时，硬件对混合的处理与引擎内部 **手算混色** 需区分；后者以本模块为准。

## 3. 广色域（Display P3）与系统色彩管理

- **方向**：内部可扩展为 **线性 Display P3** 或 ICC 工作空间；输出前映射到目标色域。
- **现状**：未接系统 ICC；令牌仍落在 **sRGB 三角** 内，与当前 shadcn 默认浅色主题一致。

## 4. HDR / 场景线性 / PQ

- **用途**：高动态范围 UI、与 3D/视频管线统一。
- **现状**：未实现；若未来需要，应在 **场景线性 + tone-map** 后再进现有 8-bit 管线。

---

## A. 令牌层（已实现）

- 默认主题对齐 **shadcn/ui v4** 文档站 `apps/v4/app/globals.css` 中 `:root` / `.dark` 的 **全部** `@theme inline` 语义色（含 `card`、`popover`、`chart-1..5`、`sidebar-*`、`surface`、`code*`、`selection*`；`--radius: 0.625rem`）。
- `chart-*` 引用 Tailwind v4 默认 `--color-blue-300..800`（见 `Reference/TailwindCSS/packages/tailwindcss/theme.css`）。
- 转换：`oklch_to_display_srgba`（`palette::Oklch` → `Srgb`）。

## B. 线性混合（已实现）

- **线性 lerp**：`lerp_display_srgb_linear` — 两色在线性 sRGB 下插值，用于动画、渐变、任意 CPU 侧混色。
- **CSS 式混色**：`mix_display_srgb_in_oklab` — 在 **Oklab** 中按比例混合，接近 `color-mix(in oklab, ...)`，与 **令牌/设计意图** 一致时常用。
- **与半透明直通道**：仅改 `alpha`、交给 GPU alpha blend 的路径（如 `with_alpha` 只改 `w`）保持原样；物理上更敏感的是 **两色在 CPU 上合成**时用上述函数。

---

## 组件库选型：令牌一致 vs 物理混合？

**更推荐：以「令牌一致」为主，在需要处叠加「物理/感知混合工具」。**

- **shadcn 风格**的首要目标是与 **CSS 变量、Tailwind、无障碍对比度** 一致；令牌用 **OKLCH/OKLab** 与官方主题同源最省事。
- **物理混合**（线性光强）在 **渐变、动画插值、多层半透明在 CPU 上合成** 时减少发灰、偏色；不必替代 CSS 的 `color-mix(in oklab, …)` 语义，二者可并存：**设计对齐用 OKLab mix，物理光强用 linear lerp**。

---

## 与 shadcn / 浏览器：`border`、`outline`、焦点环

- **组件名 `variant="outline"`** 在 shadcn 里对应的是 Tailwind 的 **`border` 类**（盒模型边框、参与占位），**不是** CSS 属性 `outline`（画在边框外侧、默认不占流式布局尺寸）。
- **焦点环**：shadcn / Tailwind 的 **`ring-*`** 在浏览器里通常由 **`box-shadow`** 实现，与 CSS `outline` 也不是同一套属性。
- **RUI**：`outline` 变体按钮使用 **`border` 语义色**（`parse_color("border")`）+ 背景/字色；焦点高亮使用 **`Style::shadow_*`（单槽 box-shadow）**，与「边框 + 投影环」的分工和 shadcn 的常见实现方式一致。若将来要像素级复刻 `outline-style` 等，可再接入 `Style::outline_*`，但与当前 shadcn Button 默认写法无关。

## `transition` 与按钮 hover（RUI）

- 浏览器里 `transition-all` 多为 **ease + 固定时长** 的插值。
- RUI 在 **Button** 上用 **`use_ui_transition_spring` + `lerp_display_srgb_linear`**：目标值 0/1 表示「未悬停 / 悬停」，每帧由 `style::animation::tick_animations` 推进弹簧，颜色在 **线性 sRGB** 下插值，观感接近短 **`transition`**；曲线为 **欠阻尼弹簧**，与 CSS 默认 `ease` 不完全相同，但避免生硬跳变。
- 变体 utility 对齐文档站 Aria Button 采样：`primary/80`、`secondary` + `foreground 5%` mix、destructive `/10`→`/20` + 不透明 `destructive` 字色。

---

## 换肤冒烟（WP-3.8）

Gallery 切 **dark** 后抽查：

| 控件 | 期望 |
|------|------|
| Button default / outline / destructive | chrome 来自 Theme token，无残浅色硬编码 |
| Input 焦点环 | theme `ring` |
| Dialog 面板 / 遮罩 | `card`/`background` 语义色 |
| Toast success / warning | `chart-2` / `chart-4`（随主题变），不是固定 `green-600` |

库测不替代本条；签署以 [`component_variants.md`](component_variants.md) §3 为准。

---

## 相关代码

| 模块 | 说明 |
|------|------|
| `style/color_space.rs` | OKLCH 转 display sRGB、线性化/编码、linear lerp、Oklab mix |
| `style/theme.rs` | `DesignTokens` 默认值（OKLCH 源） |
| `style/color.rs` | 语义名解析为 `get_theme()` 中的 sRGB |
| `style/animation.rs` | `Spring`、`use_ui_transition_spring`、`tick_animations` |
| `components/button.rs` | hover 弹簧 + 与 shadcn 对齐的边框/阴影分工 |
