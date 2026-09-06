# RUI 主题与令牌

> **文档地位**：运行时 **Theme / DesignTokens**、明暗切换、应用级 `theme.toml` 覆盖、多主题预设架构的**策略契约**。色空间与混色见 [`color.md`](color.md)；Display P3 / HDR 见 [`color_advanced.md`](color_advanced.md)；变体 `variant` 词汇见 [`component_variants.md`](../components/component_variants.md)。  
> **冻结状态**：**未冻结**（2026-09-06）。纠错可改；策略变更（默认令牌表、patch merge、`set_theme_mode` 语义）须改本文。  
> **实现锚点**：[`theme.rs`](../../crates/rui/src/style/theme.rs) · [`theme_override.rs`](../../crates/rui/src/style/theme_override.rs) · [`color.rs`](../../crates/rui/src/style/color.rs)（`parse_color`）· [`semantic_tone.rs`](../../crates/rui/src/style/semantic_tone.rs) · `crate_tests/style/color_freeze.rs`（主题相关用例）。

> **从属**：[COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §4.3（外观）· §5.2.2（Theme 承诺面）。组件 chrome 须走 `parse_color("primary")` 等语义名，禁止与 Theme 无关的硬编码色值（色空间规则见 [`color.md`](color.md)）。

---

## 0. 两个正交维度

| 维度 | 含义 | 框架 API | 典型配置 |
|------|------|----------|----------|
| **明暗** | light / dark 外观 | `set_theme_mode` / `toggle_theme_mode` | `ThemeOverrides.light` / `.dark` |
| **品牌预设** | ocean / forest / 用户自建多套 | 应用切换时 `install_theme_overrides` | 单文件 `[presets.*]` 或多 `themes/*.toml` |

切换「森林 · 深色」= 先 `install_theme_overrides(forest)`，再 `set_theme_mode(Dark)`。**预设名不由框架内置**；框架保证 patch 安装后所有 `parse_color("primary")` 与组件 chrome 一致。

---

## 1. 框架令牌层（`DesignTokens`）

默认主题对齐 **shadcn/ui v4** 文档站 `apps/v4/app/globals.css` 中 `:root` / `.dark` 的 **全部** `@theme inline` 语义色：

- `background` / `foreground`、`card`、`popover`、`primary`、`*-foreground`
- `secondary`、`muted`、`accent`、`destructive`、`border`、`input`、`ring`
- `chart-1` … `chart-5`（Tailwind v4 默认 `--color-blue-*`，**图表序列色**）
- `sidebar-*`、`surface`、`code*`、`selection*`
- `scrollbar-*`、`--radius: 0.625rem`（`DesignTokens.radius`）

OKLCH 源值 → display sRGB 的转换在 [`color_space.rs`](../../crates/rui/src/style/color_space.rs)（`oklch_to_display_srgba`）；**本文只约定令牌名与运行时行为**。延迟编码 / 显示变换见 [`color_advanced.md`](color_advanced.md) §3.3。

### 与语义变体色的边界

| 来源 | 用途 | 文档 |
|------|------|------|
| `DesignTokens` / `chart-*` | 组件 chrome、图表序列 | 本文 |
| `SemanticTone`（success / warning …） | Toast、Alert、Button soft tint | [`color.md`](color.md) §语义变体色 |

`chart-2` **不是** Toast success 色；success 走 `green-600`（`semantic_tone`）。

---

## 2. 运行时 API

```rust
use rui::prelude::*;

// 读取
let t = get_theme();
let dark = is_dark_theme();

// 明暗（保留已安装的 ThemeOverrides）
set_theme_mode(ThemeMode::Dark);
toggle_theme_mode();

// 焦点环几何（与配色 token 分离）
set_focus_ring_style(FocusRingStyle::Soft); // Hard | Soft
```

| API | 说明 |
|-----|------|
| `get_theme()` / `with_theme(f)` | 当前 `Theme` 快照 |
| `set_theme(Theme)` | 整表替换（高级用法；一般应用用 patch） |
| `set_theme_mode` / `toggle_theme_mode` | 在 **默认表 + 已安装 patch** 上切 light/dark |
| `set_focus_ring_style` | 切换后仍保留，除非 patch 指定 `focus_ring_style` |

---

## 3. 应用级令牌覆盖（已实现）

应用启动时加载 `theme.toml` / `theme.json`，**局部覆盖** `DesignTokens`；未出现的字段保持 shadcn 默认。

```rust
// 最早阶段（run / run_with 之前）
if let Ok(patch) = load_theme_overrides("theme.toml") {
    install_theme_overrides(patch)?;
}

// 或代码构造
install_theme_overrides(ThemeOverrides {
    light: DesignTokensPatch {
        destructive: Some("oklch(0.577 0.245 27.325)".into()),
        primary: Some("oklch(0.45 0.2 250)".into()),
        ..Default::default()
    },
    dark: DesignTokensPatch {
        primary: Some("oklch(0.92 0 0)".into()),
        ..Default::default()
    },
    focus_ring_style: Some("soft".into()),
})?;
```

- **`clear_theme_overrides()`**：测试 / 热重载。
- **颜色字符串**：与 [`parse_color`](../../crates/rui/src/style/color.rs) 同源 — `oklch(...)`、`#hex`、`primary`、`green-600`、`primary/80` 等。

### 单预设 `theme.toml`

```toml
focus_ring_style = "soft"

[light]
destructive = "oklch(0.577 0.245 27.325)"
primary = "oklch(0.205 0 0)"

[dark]
primary = "oklch(0.922 0 0)"
destructive = "oklch(0.704 0.191 22.216)"
```

---

## 4. 应用多主题架构（约定）

框架 **不** 内置主题名注册表；应用用很薄的目录层即可支持「多预设可切换 + 自有样式块」。

### 4.1 三层样式

```text
┌─────────────────────────────────────────┐
│ 1. RUI DesignTokens（本文）              │  install_theme_overrides
├─────────────────────────────────────────┤
│ 2. 应用设计令牌  hero-*, chat-* …        │  [app.tokens] → 应用 serde + Signal
├─────────────────────────────────────────┤
│ 3. 组件样式  class / RSX / 未来全局规则   │  utility；见 global-stylesheet-loader
└─────────────────────────────────────────┘
```

第 2 层颜色**应引用**框架令牌（`primary/90`、`border/50`），避免第二套 hex。

### 4.2 单文件多预设（主题少、希望一份配置）

```toml
[meta]
default_preset = "brand"

[presets.brand.light]
primary = "oklch(0.45 0.2 250)"

[presets.brand.dark]
primary = "oklch(0.92 0 0)"

[presets.forest.light]
primary = "oklch(0.42 0.12 145)"

# 框架不解析；应用自定义 struct
[app.tokens]
chat-bubble-radius = "1rem"

[app.presets.brand.tokens]
hero-gradient-start = "primary"
```

应用 deserialize `presets` → `HashMap<String, ThemeOverrides>`，`app` → 自有类型；切换时：

```rust
fn apply_preset(catalog: &Catalog, name: &str, mode: ThemeMode) {
    install_theme_overrides(catalog.presets[name].clone())?;
    APP_TOKENS.set(catalog.app.for_preset(name));
    set_theme_mode(mode);
}
```

### 4.3 多文件 + 清单（可下载主题包）

```text
themes/
  manifest.toml    # default = "brand", presets = [...]
  brand.toml       # 仅 [light] / [dark]
  forest.toml
app/
  tokens.toml      # [app.*] 或按 preset 分文件
```

```rust
fn apply_preset(name: &str, mode: ThemeMode) {
    install_theme_overrides(load_theme_overrides(format!("themes/{name}.toml"))?)?;
    // 可选：load_app_tokens(...)
    set_theme_mode(mode);
}
```

### 4.4 框架职责边界

| 框架提供 | 应用自有 |
|----------|----------|
| `ThemeOverrides` / `install_theme_overrides` | 预设名、清单、持久化用户选择 |
| `set_theme_mode` + patch merge | `[app.tokens]`、布局常数、业务渐变 |
| `parse_color` 解析 patch 内颜色串 | 全局 `data-slot` 规则（Phase 1+，见计划文档） |

发版路线图：[`plans/global-stylesheet-loader.md`](../plans/global-stylesheet-loader.md)（Phase 0 令牌覆盖 ☑；Phase 1–3 全局样式表）。

---

## 5. 与组件外观的关系

- **`variant="outline"`** = 盒模型 **border** 语义色，不是 CSS `outline` 属性（详见 [`color.md`](color.md)）。
- **焦点环**：theme `ring` 色 + `FocusRingStyle` 几何；绘制走 `Style::shadow_*`（box-shadow 语义）。
- **Button hover**：`transition-colors` 插值目标色来自当前 Theme / `parse_color`（见 [`color.md`](color.md)）。

---

## 6. 换肤冒烟（WP-3.8）

Gallery 切 **dark** 后抽查：

| 控件 | 期望 |
|------|------|
| Button default / outline / destructive | chrome 来自 Theme token |
| Input 焦点环 | theme `ring` |
| Dialog 面板 / 遮罩 | `card` / `background` |
| Toast success / warning | `semantic_tone`（非 `chart-*`） |

库测：`cargo test -p rui --lib color_freeze`（含 `theme_override_*` 用例）。签署见 [`component_variants.md`](../components/component_variants.md) §3。

---

## 7. 测试与 CI

| 用例 | 命令 |
|------|------|
| 主题 patch + 明暗往返 | `cargo test -p rui --lib color_freeze` |
| 默认 light 令牌关系 | `cargo test -p rui theme::tests` |

策略变更须同步更新 `color_freeze` 中与主题相关的测试。

---

## 8. 相关代码

| 模块 | 说明 |
|------|------|
| `style/theme.rs` | `DesignTokens`、`Theme`、`set_theme_mode` |
| `style/theme_override.rs` | `ThemeOverrides`、`load_theme_overrides`、`install_theme_overrides` |
| `style/color.rs` | `parse_color` / `try_parse_color` → `get_theme().colors` |
| `style/semantic_tone.rs` | 语气色（与 `chart-*` 分离） |
| `style/color_space.rs` | OKLCH → sRGB（色数学，见 [`color.md`](color.md)） |
