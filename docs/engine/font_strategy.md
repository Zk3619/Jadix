# 字体策略（Stage 8）

> **文档地位**：字体加载、族栈、字重匹配与配置的**策略契约**。诊断手册见 [font.md](font.md)。  
> **冻结状态**：**已冻结**（2026-09-06；Gallery **7) 字体族 / 字重 / 行高覆盖（验收）** 走查通过）。纠错（行号、验收步骤、Gallery id）可改；策略变更（默认栈顺序、CSS 匹配、伪粗、懒加载边界）须改本文，禁止静默改行为。  
> **实现锚点**：[`font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs) · [`utils/font.rs`](../../crates/rui/src/utils/font.rs) · [`prepare_text.rs`](../../crates/rui/src/renderer/prepare_text.rs) · Gallery [`text.rs`](../../examples/gallery/src/pages/text.rs)（`FontOverrideShowcase`）。  
> 从属文本轨道（巩固期 WP-2.1 / golden 确定性）。Dubai→Inter 根因与提案过程见 [archive/implementation_plan.md](../archive/implementation_plan.md)（**勿与** `ENGINEERING_IMPLEMENTATION_PLAN.md` 混淆）。各 OS 字体路径仍在 [`font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs)（未迁入 [`platform/`](../../crates/rui/src/platform/)）；见 [`platform.md`](platform.md)。

短计划：UI 默认西文用 **Inter**（或平台 UI 字体），CJK / emoji / 符号仍走懒加载回退。实现入口见 [`font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs) 与 [`utils/font.rs`](../../crates/rui/src/utils/font.rs)。

## 原则

1. **禁止**启动时 `FontSystem::new()` 全量 `load_system_fonts()`（Windows 上数百 MB RSS）。
2. **启动只装**：UI sans（Inter 等**磁盘候选** + 平台 Segoe UI / SF 近亲若干字重面）+ Lucide。Inter 文件由**应用**随包提供（见下文「Inter 分发」），核心不 embed。
3. **按需打开**：CJK / Hangul / Thai / emoji / symbols / Arabic 等由 `ensure_coverage_for_text` 按码点打开。

## 全局配置入口

| 方式 | 族 | 字号 / 字重 | 说明 |
|------|----|-------------|------|
| **`RUI_FONT_FAMILY`** | ✅ 逗号分隔 CSS 族列表 | ❌ | 写入默认 sans 栈；面须已在 `fontdb` |
| **`RUI_FONT_SIZE`** / **`RUI_FONT_WEIGHT`** | ❌ | ✅ | 布局根 `cascade_root()` 默认（16 / 400） |
| **`RUI_SYNTHETIC_BOLD`** | ❌ | 合成开关 | `0`/`false`/`off` 关闭；默认开 |
| **`RUI_FONT_DIR`** | ✅ 加载 `Inter-*.ttf` 等 | ❌ | 与 `font_search_dirs()` 联动 |
| **`RUI_TEXT_LOCALE`** | ✅ Noto CJK 子族优先级 | ❌ | `zh-CN` / `ja` / `ko` 等 |
| **`TextDefaults` / `set_text_defaults`** | ✅ | ✅ | 进程级 API（`rui::set_text_defaults`） |
| **`WindowOptions::default_font*`** | ✅ | ✅ | `run_with(WindowOptions::new().default_font("Inter, sans-serif").default_font_size(15.0).default_font_weight(400), …)` |
| **`TextStyle::cascade_root()`** | ✅ 默认 sans 栈 | ✅ 读 `text_defaults()` | 子树继承 |
| **根节点 `class="font-sans text-base"`** | ✅ `font-sans` / `font-serif` / `font-mono` | ✅ `text-*` / `font-medium` / `leading-*` | 与 Web utility 对齐 |

未设置 `RUI_FONT_FAMILY` 时，默认栈为 **Inter → Inter Variable → Segoe UI / SF Pro → PingFang / YaHei UI → Noto CJK → `system-ui` / `sans-serif`**。

```rust
use rui::prelude::*;

// 启动前或 `run_with` 选项：
rui::set_text_defaults(TextDefaults {
    family: Some("Inter, Segoe UI, sans-serif".into()),
    size_px: 16.0,
    weight: 400,
    synthetic_bold: true,
});

rui::run_with(
    WindowOptions::new()
        .title("App")
        .default_font("Inter, Segoe UI, sans-serif")
        .default_font_size(16.0)
        .default_font_weight(400),
    || { /* ui */ Element::new() },
);
```

## 局部覆盖入口

| 方式 | 族 | 字重 | 字号 | 行高 |
|------|----|------|------|------|
| **RSX `class`** | ✅ `font-sans` / `font-serif` / `font-mono` | ✅ `font-medium` / `font-500` … | ✅ `text-sm` … | ✅ `leading-none` / `leading-tight` / `leading-[1.15]` |
| **RSX 属性** `font_family` / `text_font` | ✅ CSS 列表 | — | — | — |
| **RSX** `text_size` / `font_weight` / `leading` / `line_height` | — | ✅ | ✅ | ✅ |
| **`Element` / `text()` / `BaseComponentBuilder`** `.font_family()` `.font_weight()` `.text_size()` `.leading_*()` | ✅ | ✅ | ✅ | ✅ |
| **子树继承** | ✅ | ✅ | ✅ | ✅ |

```rsx
<div class="font-sans text-sm font-medium leading-tight">
    <span font_family="ui-monospace, Consolas, monospace" class="font-mono text-xs">code</span>
</div>
```

## 选族 / 选字重 / 可变字体 / 伪粗

实现于 `utils/font.rs` + `prepare_text.rs`（cosmic / fontdb / ttf-parser `wght` 轴）：

1. **选族**：`resolve_family(db, list)` — 从左到右；`sans-serif` / `system-ui` / `ui-sans-serif`、`serif` / `ui-serif`、`monospace` / `ui-monospace` 映射到默认栈。
2. **可变字体**：若该族某面含 `wght` 轴且覆盖请求值，**原样把请求字重交给 `Attrs`**，由 cosmic-text 插值（不再 snap 到 Regular 400）。
3. **静态面**：`css_fonts4_match_weight` 按 [CSS Fonts 4](https://www.w3.org/TR/css-fonts-4/#font-style-matching) 匹配离散 `face.weight`。Segoe 仅有 400/600/700 时，`font-medium`（500）匹配 **400**，而不是跳到 Semibold 600。
4. **Synthetic bold**：匹配面轻于请求且请求 ≥ 500、且无可用 `wght` 轴时，置 `ResolvedFaceWeight.synthetic_bold`。绘制阶段用与文本同色的细阴影加粗 Regular（CSS `font-synthesis: weight`）。可用 `RUI_SYNTHETIC_BOLD=0` 或 `WindowOptions::synthetic_bold(false)` 关闭。
5. **Lucide**：强制 400，不做伪粗。
6. **Fallback**：shaping 前 `ensure_coverage_for_text` 按码点懒加载 CJK/emoji。

详见 [font.md](font.md)（`RUI_TEXT_DEBUG`）。

## Gallery 视觉验收

```powershell
cargo run -p gallery
```

侧栏 **文本** → 卡片 **7) 字体族 / 字重 / 行高覆盖（验收）**：

1. `font-sans` / `font-serif` / `font-mono` 三列字形可辨（衬线、等宽明显）。
2. RSX `font_family` / `text_font`（Segoe / Consolas / Times）覆盖 class 泛型族。
3. 字重阶梯：`font-normal`（400）轻于 `font-medium`（500）；`font-bold`（700）更重。无 Medium 面时 500 为 Regular + 伪粗，不得跳到 Semibold。
4. `text-xs` … `text-2xl` 字号递增。
5. `leading-none` 的 CJK 比 `leading-normal` 更容易顶/底缺笔（已知裁切，见 [font.md](font.md) §2）。
6. 父级 `class="font-mono text-sm"` 下，子级 `font-sans` / `font_family` 能改族。

进程级 `RUI_FONT_FAMILY` / `WindowOptions::default_font*` 不在本卡切换。调试：`$env:RUI_TEXT_DEBUG="gal-font"`。

## 文档冻结

**已冻结**（2026-09-06；Gallery **7) 字体族 / 字重 / 行高覆盖（验收）** 走查通过）。与 [`box_model.md`](box_model.md) 同口径：纠错（行号、验收步骤、Gallery id）可改；策略变更（默认栈顺序、CSS 匹配、伪粗、懒加载边界）须改本文，禁止静默改行为。

**已知差距**（接受为冻结后常态，不视为欠账）：

| 项 | 状态 |
|----|------|
| Inter 核心嵌入（`bundled-inter`） | **刻意不做**；应用级 `fonts/` / `RUI_FONT_DIR` / 自行 `include_bytes!`（见 [`assets_and_icons_architecture.md`](assets_and_icons_architecture.md) §2.1） |
| CJK fallback 独立字重轴 | 不做；中文常整段 Regular |
| synthetic bold | 同色细阴影，不是轮廓加粗 |
| `leading-none` 裁切 | 布局/绘制策略，不单靠改 token |

诊断手册 [font.md](font.md) 与本文一并冻结为「契约 + 已知差距」；`RUI_TEXT_DEBUG` 输出格式仍可改，不算策略变更。

## Inter 分发（应用级）

Inter 是 UI **设计选择**，不是引擎硬依赖。框架提供加载机制与默认族栈；字体文件由**使用框架的应用**决定是否随包分发。

| 方式 | 责任方 | 说明 |
|------|--------|------|
| 应用旁路 `fonts/` | **应用作者**（推荐） | 发布时带上 `Inter-Variable.ttf`；`font_search_dirs()` 自动搜索 exe 旁 `fonts/`、`assets/` |
| `RUI_FONT_DIR` | 应用 / 运维 | 开发或 CI 指定字体目录 |
| 应用自行 embed | 应用作者 | 单文件 exe 时在应用内 `include_bytes!` 并注册 fontdb |
| 核心 `bundled-inter` | **不做** | 与 [`assets_and_icons_architecture.md`](assets_and_icons_architecture.md) 一致；未来若有需要由独立 crate（如 `jadix_fonts_inter`）提供，不堆在 `rui` feature 里 |

| 步骤 | 状态 | 内容 |
|------|------|------|
| 运行时目录 | ✅ | `RUI_FONT_DIR` / exe 旁 `fonts/` / `crates/rui/assets/` 加载 `Inter-Variable.ttf` 等（`load_eager_project_ui_fonts`） |
| 平台瘦加载 | ✅ | Windows：Regular / Light / Semilight / Semibold / Bold / Black（仍无独立 Medium 500） |
| 回退链 | ✅ | Inter VF → Segoe / SF → 懒加载 CJK |
| 测宽对齐 | ✅ | 布局与绘制共用 `resolve_face_weight` |

## 与现有加载器的关系

`font_loader` 仍是「启动瘦栈 + 脚本桶懒加载」。本策略固定 **UI 主族优先 Inter（或 env / API 覆盖）**；字重走 VF 插值或 CSS 匹配 + 可选伪粗，详见 [font.md](font.md)。
