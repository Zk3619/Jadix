# 静态资源外置与图标库拆分（架构草案）

> **文档地位**：资源分层、`jadix_icons` / `jadix_devtools` 等可选 crate 边界、与 W4 伞形发布及 `jadix_components` 拆分的对齐说明。  
> **状态**：草案（未实施）；执行前以 [`AUTHORITY.md`](../AUTHORITY.md) / [`W4_PRELAUNCH_CHECKLIST.md`](../ops/W4_PRELAUNCH_CHECKLIST.md) 排期为准。  
> **交叉**：[`devtools.md`](../ops/devtools.md) · [`reactivity_and_hot_reload.md`](../ops/reactivity_and_hot_reload.md) · [`headless.md`](../components/headless.md) · [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) §3（双书写面）

---

## 1. 目标形态

```
用户应用
  assets/          ← 图片、自定义字体、可选 lucide.ttf / twemoji（用户自备）
  src/
  Cargo.toml       ← 显式依赖需要的「能力 crate」

jadix              ← 核心：reactivity / layout / renderer / IconProvider+EmojiProvider 契约
jadix_icons        ← 图标库（现 lucide 全家桶）
jadix_emoji_*      ← 可选 emoji 覆盖（Twemoji 等；默认不装）
jadix_components   ← 组件库（现 components/，未来独立）
jadix_macros
jadix_style_tokens
jadix_hot_reload   （可选，开发期 watcher）
jadix_devtools     （可选，运行时 Inspector 面板）
```

**原则**：`jadix` 核心里 **零 `include_bytes!` 产品资源**；图标、emoji 位图包、演示字体、Inspector 面板均为可选依赖。Emoji **默认系统彩色字体**，不内置 Twemoji。

---

## 2. 三层资源模型

| 层级 | 谁管 | 内容 | 加载方式 |
|------|------|------|----------|
| **用户资源** | 应用 `assets/` | PNG/WebP、自定义 SVG、字体文件、**可选** Twemoji 目录 | 运行时路径（`RUI_ASSETS_DIR` / exe 旁 `assets/`） |
| **图标库** | `jadix_icons` | `lucide.json`、`lucide_paths.json`、`lucide_animated.json`、可选子集 TTF | 该 crate 的 `build.rs` codegen → phf / 子集字体 |
| **Emoji 覆盖（可选）** | 应用 `assets/` 或 `jadix_emoji_*` | Twemoji PNG/SVG、Noto Color Emoji TTF、私有 emoji 包 | **默认不装**；经 `EmojiProvider` 注入 |
| **框架能力** | `jadix` | `IconProvider` / **`EmojiProvider`**、字形渲染、SVG 栅格、图片注册表 | **不持有** lucide / Twemoji 数据；默认走 **系统彩色字体** |

图片等已由 `image_registry` / 文件路径走向用户 `assets/`；核心里不应再 embed 演示用二进制。**Emoji 与 Lucide 同原则：核心零内置产品资源。**

### 2.1 与现有 `bundled-*` feature 的关系

| 现状（`rui`） | 目标 |
|---------------|------|
| `bundled-lucide` / `bundled-cjk` / `bundled-emoji` / `bundled-symbols` 在 crate 内 `include_bytes!` | **从核心删除** |
| `fonts-cjk` / `fonts-emoji` 等运行时磁盘加载 | 保留为 **可选路径**（`assets/fonts/`、`RUI_FONT_DIR`） |
| Twemoji 目录（`assets/twemoji/`）+ `RUI_EMOJI_CUSTOM_GLYPH` 默认开 | **默认关**（已落地）；系统字体 CBDT/COLR；`RUI_EMOJI_CUSTOM_GLYPH=1` / `RUI_TWEMOJI_DIR` 显式开位图 |
| 需要「单文件分发」 | 可选 **`jadix_icons` 的 `bundled-lucide`**、**`jadix_fonts_noto`**、**`jadix_emoji_twemoji`** 等独立 crate，而非堆在核心 |

Lucide、CJK、Emoji **不要混在同一 feature**。

---

## 3. `jadix_icons` 职责

从 `crates/rui` 迁出：

- `assets/lucide*.json`、`lucide.ttf`（若仍提供 bundled 子集）
- `build.rs` + phf / animated recipe codegen
- `lucide.rs`、`lucide_animated.rs`、与 Lucide 绑定的 `icon_texture` 逻辑

### 3.1 对外 API（稳定面草案）

```rust
// jadix_icons::lucide
pub fn glyph(name: &str) -> Option<String>;
pub fn svg_inner(name: &str) -> Option<&'static str>;
pub fn animated_recipe(name: &str) -> Option<&AnimatedIconRecipe>;

/// 子集 TTF；None 表示走磁盘 `assets/fonts/lucide.ttf`
pub const LUCIDE_FONT: Option<&'static [u8]>;

/// 在 `main` 开头注册到 jadix 的 IconProvider
pub fn install();
```

### 3.2 Features（`jadix_icons` 草案）

| Feature | 说明 |
|---------|------|
| `lucide` | 静态 path / glyph 查表（build 期 phf，**全量**） |
| `lucide-animated` | animated recipe codegen（**全量**） |
| `bundled-lucide` | embed 子集 `lucide.ttf`（可选） |
| `catalog-full` | 与默认全量一致；保留给未来子集化 feature |

> **2026-08-28 过渡**：`rui` 已移除 workspace 源码扫描与 `lucide.subset` 子集过滤，暂时 **全量** 嵌入 Lucide JSON，避免弹窗关闭等内置图标漏扫。子集化在 `jadix_icons` 拆分后再做。

### 3.3 构建时子集（延后）

子集扫描（`icon_scan` / `lucide.subset` / `RUI_LUCIDE_SUBSET`）**已下线**，待 `jadix_icons` 独立 crate 后按应用依赖图重新设计。

---

## 4. `jadix` 核心如何接图标

用 **trait 注入**，避免 `jadix` 硬依赖 `jadix_icons`：

```rust
// jadix::icon（核心只定义契约）
pub trait IconProvider: Send + Sync {
    fn glyph(&self, name: &str) -> Option<String>;
    fn svg_inner(&self, name: &str) -> Option<&str>;
}

pub fn set_icon_provider(p: Arc<dyn IconProvider>);
pub fn lucide_glyph(name: &str) -> Option<String>; // 转发到 provider
```

| 依赖组合 | 行为 |
|----------|------|
| 仅 `jadix` | provider 未注册 → `lucide_glyph` 返回 `None`；或用户自建 `HashMap` |
| `jadix` + `jadix_icons` | `jadix_icons::install()` 注册默认 Lucide provider |
| `jadix_components` | 默认 `depends on jadix_icons`；文档写明「ShadCN 风组件需图标库」 |

`jadix_macros` 的 `<Icon name="…">` 编译期校验改读 **`jadix_icons` build 产物** 的 phf，而非 `crates/rui/assets/`。

---

## 5. 与 `jadix_components` 拆分对齐

```
jadix_components/Cargo.toml
  jadix = { path = "../jadix" }
  jadix_icons = { path = "../jadix_icons" }   # 默认或 feature "icons"
```

| 模块 | crate |
|------|--------|
| `Button` 里 `lucide_glyph("check")` | `jadix_components` → `jadix_icons` |
| `Element` / `Signal` / `wgpu` / `tree_hit` | `jadix` |
| Lucide 数据与 build | `jadix_icons` |

Gallery 依赖：`jadix` + `jadix_icons` + `jadix_components`（过渡期可用 `jadix` feature 重导出 components）。

图标库先行，作为 **components 独立化的模板**（可选 crate、feature 矩阵、build 期 codegen、应用层显式依赖）。

---

## 6. 迁移路线（分 4 步）

### Phase 0 — 契约（1–2 天）

- 在 `jadix` 加 `IconProvider` + 运行时转发
- 现有 `utils/lucide.rs` 改为调用 provider（默认 impl 仍读当前 build 产物，行为不变）

### Phase 1 — 抽 `jadix_icons`（3–5 天）

- 新建 `crates/jadix_icons`，迁 assets + build + lucide 模块
- `jadix` 删 lucide 相关 `include_bytes!` / `build.rs` 逻辑
- `jadix_macros` 依赖 `jadix_icons` 的 build 输出做 Icon 名校验

### Phase 2 — 资源外置（2–3 天）

- 删核心内 `bundled-*` 字体 embed；`font_loader` 只走磁盘 + 懒加载
- 约定 `assets/fonts/`、`assets/images/`；Gallery 在 `examples/gallery/assets/` 放演示字体
- 大文件用 git LFS 或下载脚本，不进核心 crate

### Phase 3 — `jadix_components`（与 components 目录拆分同步）

- `components/` → 新 crate，显式依赖 `jadix_icons`
- `jadix` 默认 feature 不再包含 components

---

## 7. 用户项目示例

```toml
[dependencies]
jadix = { version = "0.1", default-features = false, features = ["wgpu"] }
jadix_icons = { version = "0.1", features = ["lucide", "lucide-animated", "subset-auto"] }
jadix_components = { version = "0.1" }
```

```text
my-app/
  assets/
    images/logo.png
    fonts/              # 可选；省略则用系统字体
  src/main.rs           # jadix_icons::install(); jadix::run(...)
```

---

## 8. Devtools 是否应作为与热重载一致的外部 crate？

### 8.1 热重载的现有拆分（对照）

| 部分 | 位置 | 性质 |
|------|------|------|
| 文件监视、`template_patch` 编译 | `rui_hot_reload_cli` | **独立 crate / 开发期二进制** |
| TCP 接收、patch 合并、`inject_hot_reload_partial` | `rui` + feature `hot-reload` | **核心内薄协议层** |
| `template_id`、reconcile 钩子 | `rui` 常驻 | 与 retained 树绑定，无法完全外置 |

热重载 **不是**「整个能力在一个外部 crate」——而是 **外部 watcher + 核心 receiver**。

### 8.2 Devtools 的耦合点

当前 `devtools` 分布在：

- `app/devtools/*`：面板 UI、`wrap_app`、样式编辑
- `layout_tree.rs`：`#[cfg(feature = "devtools")]` 高亮收集、`sync_devtools_*`
- `runtime.rs`：`patch_devtools_style` / `patch_devtools_cssom`
- `AppState`：retained 树路径 patch

与渲染循环、布局 bounds、热重载 `template_id` 解析深度绑定。

### 8.3 建议：**与热重载同模式，而非同形态**

| 维度 | 热重载 | Devtools（建议） |
|------|--------|------------------|
| 可选性 | feature + 独立 CLI | feature + **独立库 crate `jadix_devtools`** |
| 进程 | CLI 可另进程 | **进程内**（Inspector 必须跟帧同步） |
| 核心保留 | 协议 + reconcile 钩子 | **稳定 introspection API** + patch 入口 + **极薄** layout 钩子 |
| 外置 crate | watcher / 编译链 | **Dock 面板、F12、Pick UI、`wrap_app` 壳** |

**结论**：

1. **是**——Devtools **应**像热重载一样成为 **W4 伞形下的可选外部 crate**，不进默认 release / SemVer 稳定面（与 [`devtools.md`](../ops/devtools.md) §7 一致）。
2. **否**——不应期望像 `jadix_hot_reload` 那样做成 **独立 CLI 进程**；它是 **in-process 可选库**。
3. **核心不能为零钩子**：至少保留：
   - `snapshot_tree` / `pick_element_path` / `BoxModelSnapshot`（测试与 headless 工具也需要）
   - `AppState::patch_devtools_element_at_path` 或等价 trait
   - layout 阶段可选回调（或 `feature = "devtools-hooks"` 编译进核心，默认 off）

### 8.4 目标依赖图

```text
jadix                    ← 默认：无 devtools 面板；可选 devtools-hooks feature
jadix_devtools           ← depends on jadix；feature devtools 启用面板 + wrap_app
jadix_hot_reload         ← depends on jadix；开发期 watcher（现有 rui_hot_reload_cli）
```

应用：

```toml
jadix = { features = [] }
jadix_devtools = { optional = true }   # Gallery / 本地调试
jadix_hot_reload = { optional = true } # 仅 dev profile
```

Inspector 与热重载 **联动**（`template_id` → `file:line:col`）保持分工：编译链仍在 `jadix_hot_reload`，Devtools 只 **读** `template_id`，不重复编译（见 [`devtools.md`](../ops/devtools.md) §4）。

---

## 9. 不使用 `jadix_macros` / `jadix_style_tokens` 能否用 Builder 写 UI？

### 9.1 结论（应用作者视角）

**可以。** 框架自 ADR-9 起承诺 **`ui!` ↔ Builder 双书写面**；纯 Builder 是一等公民，Gallery 中大量 `Element::from(button(…))`、`vstack()` 即为例证。见 [`headless.md`](../components/headless.md) §「与双书写面的关系」、[`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) §3。

| 能力 | 不用 `ui!` | 不用 `#[component]` | 不用直接 `use jadix_style_tokens` |
|------|------------|----------------------|-----------------------------------|
| 内置组件（Button、Input…） | ✅ `button("id").text("OK")` | ✅（内置已提供 builder fn） | ✅ |
| 布局原语 | ✅ `vstack()` / `hstack()` / `box()` | ✅ | ✅ |
| 响应式文本 | ✅ `bind_text` / `el.text(\|\| sig.get())` | ✅ | ✅ |
| 条件 / 列表 | ✅ `show_reactive` / `for_keyed_reactive` 等 | ✅ | ✅ |
| 自定义复合控件 | ✅ 手写 `fn my_card() -> Element` | ✅ `component_root` 包作用域，不必用宏 | ✅ |
| Tailwind 式 `class="flex p-4"` | ✅ Builder `.class("…")` | — | ✅ **无需 import**；解析在 `jadix` 内部调 catalog |

### 9.2 与 `jadix_macros` 的关系

| 场景 | 说明 |
|------|------|
| **你不写 `ui!`** | 不需要在应用代码里 `use jadix_macros::*`；`jadix` prelude 仍会 re-export 宏，可忽略 |
| **你不写 `#[component]`** | 用 [`component_root`](../../crates/rui/src/core/reactivity.rs) + Builder 组合 primitives；见 [`custom_components.md`](../components/custom_components.md) |
| **宏提供的自动降级** | `ui!` 会把 `.get()` 降为 `bind_text` / `bind_attr` 等；纯 Builder 需 **手动** 调 `bind_*` 或 `IntoNodeText` 闭包，否则 `button().text(sig.get())` 只捕获首帧字符串 |

**注意**：`ui!` 的编译期便利（Icon 名校验、RSX 自动 bind 降级）在纯 Builder 下需用 API 显式补齐，不是不能写，是 **样板略多**。

### 9.3 与 `jadix_style_tokens` 的关系

`jadix_style_tokens` 是 **utility class 目录**（`flex`、`p-4`、`hover:bg-primary` 等）与宏侧 unknown-class 警告的 **单一数据源**；运行时由 [`style/utility.rs`](../../crates/rui/src/style/utility.rs) 调用。

| 写法 | 是否需要用户在代码里依赖 `jadix_style_tokens` |
|------|-----------------------------------------------|
| `.class("flex gap-2 p-4")` | **否** — 字符串经 jadix 内部 `parse_class_list` |
| `.bg("primary")` / theme 令牌名 | **否** — 走 `style/theme` |
| 直接 `Style` 字段 `.width(100.)`、`.bg_color([…])` | **否** — 完全绕过 utility catalog |
| 扩展 utility 词汇表 / 代码生成 | **是** — 改 `jadix_style_tokens` crate |

### 9.4 依赖图（当前 vs 目标）

**当前**：`jadix` **硬依赖** `jadix_macros` + `jadix_style_tokens`（即使用户只用 Builder，Cargo 仍会拉取这两个 crate）。

**目标（可选瘦身，非阻塞资源拆分）**：

| Crate | 建议 |
|-------|------|
| `jadix_macros` | feature `rsx`（默认 on）；off 时核心仍可编译，无 `ui!` / `#[component]` |
| `jadix_style_tokens` | 并入 `jadix` 或 feature `utility-classes`；Builder 直写 `Style` 时可 off |

这对 **应用作者写 UI** 无强制要求；只影响 **二进制体积与编译图**。

### 9.5 Builder 最小示例（无 `ui!`）

```rust
use rui::prelude::*;
use rui::app::reactive::bind_text;

fn app() -> Element {
    let label = use_signal("Hello");
    let btn = button("ok")
        .variant("outline")
        .text({
            let label = label.clone();
            move || label.get().to_string()
        });
    bind_text(&btn, label, |el, s| el.text(s));
    Element::from(
        vstack()
            .gap(8.0)
            .child(Element::from(btn))
    )
}
```

---

## 10. 动态图标名

拆成 `jadix_icons` 之后：

- **编译期已知**：`jadix_icons` build 全量 phf（当前 `rui` 过渡态同样全量）
- **运行时未知**：`jadix_icons` feature `catalog-full`，或用户实现 `IconProvider`

`jadix` 核心不关心名字来源，只调 provider。

---

## 11. Emoji：与图标同构（系统默认 + 可选覆盖）

### 11.1 结论（与 §1 / §2 对齐）

**应当**与未来 `jadix_icons` 一样：

| 决策 | 说明 |
|------|------|
| **核心不内置 emoji 资源** | 不 `include_bytes!` Twemoji；不默认假设仓库 `assets/twemoji/` 存在 |
| **默认 = 系统彩色字体** | Segoe UI Emoji / Apple Color Emoji / Noto Color Emoji 等，经 glyphon `SwashContent::Color`（CBDT/COLR） |
| **覆盖 = 可选 provider** | 用户自备 PNG/SVG 目录、或另挂彩色 emoji 字体、或 `jadix_emoji_twemoji` 一类可选 crate |

今日债务（过渡）：~~`RUI_EMOJI_CUSTOM_GLYPH` **默认 true**~~ → **已翻转（2026-08）**：默认走系统彩色字体；`RUI_EMOJI_CUSTOM_GLYPH=1` 或有效的 `RUI_TWEMOJI_DIR` 才开启位图槽位。仓库样例 `assets/twemoji` **不再**隐式启用覆盖（半包会导致空白芯片）。

### 11.2 为何不能「像 Lucide 一样默认捆绑」

| | Lucide | Emoji |
|--|--------|--------|
| 数量级 | ~千级 path / 可选子集 TTF | 数千 PNG/SVG + 版本漂移 |
| 平台已有 | 无统一系统图标 | **各 OS 已有彩色 emoji 字体** |
| 一致性需求 | ShadCN 组件依赖固定 glyph | 多数 App 接受「跟系统长一样」 |
| 体积 | 可子集 | Twemoji 全量对默认二进制过重 |

因此：**图标外置到 `jadix_icons`；emoji 默认不外置包，只保留可插拔覆盖。**

### 11.3 `EmojiProvider` 契约（草案）

与 `IconProvider` 平行，核心只定义 trait：

```rust
// jadix::emoji
pub trait EmojiProvider: Send + Sync {
    /// 若返回 Some(rgba)，paint/Bake 走位图覆盖；None → 系统字体彩色 glyph。
    fn raster(&self, sequence: &str, pixel_size: u16) -> Option<EmojiRaster>;
    /// 可选：额外 emoji 字体路径（Noto Color Emoji 等），在 FontSystem 懒加载。
    fn font_paths(&self) -> &[&Path] { &[] }
}

pub fn set_emoji_provider(p: Option<Arc<dyn EmojiProvider>>);
/// true 仅当已注册会返回 raster 的 provider（或 env 显式强制位图模式）。
pub fn emoji_bitmap_override_active() -> bool;
```

| 配置 | 行为 |
|------|------|
| 仅 `jadix` | provider = `None` → **系统色字体光栅**（隔离 FS）；**槽位布局始终开**（字形盒内光学居中） |
| `RUI_TWEMOJI_DIR` / 应用 `assets/twemoji/` | 用户实现或内置 `TwemojiDirProvider::from_env()` 后 `set_emoji_provider` |
| `jadix_emoji_twemoji`（可选 crate） | `install()` 注册目录/LFS 资源；feature `png` / `svg` |
| `fonts-emoji` + `assets/fonts/NotoColorEmoji.ttf` | 仍走字体栈，不经 Twemoji；适合「换字体不换位图」 |

Bake / Coverage / sticky transform：**同一 provider / 同一槽位居中模型**。禁止「Coverage 用 Twemoji、Bake 用字体线框」分叉（见近期 sticky Bake 接 Twemoji 的修复动机）。

### 11.4 与现有 env / feature 的迁移

| 现状（历史） | 目标 / 现状（已落地） |
|------|------|
| `RUI_EMOJI_CUSTOM_GLYPH` 默认 **开**（兼布局+Twemoji） | **布局槽位始终开**；`RUI_EMOJI_CUSTOM_GLYPH` / `RUI_TWEMOJI_DIR` 仅切换 **位图资产**（Twemoji） |
| `RUI_TWEMOJI_DIR` / 仓库 `assets/twemoji` | `RUI_TWEMOJI_DIR` 为显式覆盖；仓库样例目录 **不** 自动启用 |
| `emoji-svg` feature | 仍可选；挂在覆盖模式或应用 feature，不进默认 |
| `bundled-emoji` | 可选；装入隔离 FS（槽位光栅回退） |
| Gallery | 默认系统色光栅 + 槽位居中；Twemoji 演示：`RUI_EMOJI_CUSTOM_GLYPH=1` + 完整 `RUI_TWEMOJI_DIR` |

### 11.5 分阶段落地（建议）

1. **契约**：加 `EmojiProvider`；`emoji_bitmap_override_active` = provider 已注册或 env 显式 on。  
2. **默认**：槽位光学居中始终开；光栅默认系统色（隔离 FS）；Gallery README 写可选 Twemoji。  
3. **Bake 对齐**：Coverage / Bake 共用槽位 + 同一光栅链（系统色或 Twemoji）。  
4. **可选 crate**：`jadix_emoji_twemoji`（或并入 `jadix_icons` 的 `emoji-twemoji` feature）——与 Lucide 拆分同一波次更干净。

### 11.6 禁区（emoji）

- 不在 `jadix` 默认路径 **硬编码** 仓库根 `assets/twemoji` 为「无则失败」。  
- 不以 MSDF / 单色 Mask 冒充彩色 emoji。  
- 不把 Twemoji 全量塞进默认 `bundled-fonts`。

---

## 12. 禁区

- 不在 `jadix` 默认 feature 中重新 embed 产品级字体 / 图标 JSON / **Twemoji**
- `jadix_devtools` 不进 Beta SemVer 稳定面（与现 `devtools` feature 策略一致）
- 不为实现「零核心钩子」而把 layout 高亮逻辑塞进应用层
- 资源拆分 **不** 废除 Builder 双书写面（ADR-9）

---

## 13. 开放问题

- [ ] `jadix_icons` 与 `jadix_macros` 的 build 依赖方向（macros 校验是否改为 `build = jadix_icons` 的 `links` 输出）
- [ ] TTF 子集化（`font-subset`）放在 `jadix_icons` 还是独立工具 crate
- [ ] `jadix_devtools` 与 layout `#[cfg]` 钩子：保留 feature vs 运行时 callback 注册表
- [ ] `jadix_macros` / `jadix_style_tokens` 可选化的 SemVer 与默认 feature 矩阵
- [ ] **`EmojiProvider` 独立 crate vs `jadix_icons` feature `emoji-twemoji`**
- [ ] 默认翻转 `RUI_EMOJI_CUSTOM_GLYPH` 的 SemVer / Gallery 回归窗口
- [ ] 系统字体缺彩色面时的降级策略（豆腐 vs 可选提示下载 Noto）