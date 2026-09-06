# RUI × Freya（`reference/freya-main`）全面对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) 为准；底座禁区见 [ENGINEERING_GRADE_FRAMEWORK.md](../ENGINEERING_GRADE_FRAMEWORK.md)；细粒度机制见 [REACTIVITY.md](../REACTIVITY.md)；六参考总表见 [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md)；GPUI 生态见 [GPUI_ECOSYSTEM_COMPARISON.md](GPUI_ECOSYSTEM_COMPARISON.md)。  
> **对照源**：[`reference/freya-main`](../../reference/freya-main)（Freya `0.5.0-rc.4` / main 快照）· RUI 以 `crates/` 与工程文档为准。  
> **日期**：2026-08-30；**2026-08-31** §7 扩为「你有我没有 / 我有你没有」编号清单；同日补 **§7.1-C / §7.2-C 拖拽与放置**  
> **规模快照**（非空白且非纯注释行，约数）：RUI `crates/` ≈ **162k** 行 / **339** `.rs`；Freya `crates/` ≈ **68k** 行 / **403** `.rs`。RUI 体量更大 partly 因单体 crate + 厚组件/测试；Freya 拆成更多 crate，单仓示例约 **154** 个 `.rs`。

**明细入口**：能力有无不要只看 §2–§6 的定性对照——直接翻 **[§7](#7-明细清单你有我没有--我有你没有--双方都有)**（F01–F62 缺口 · R01–R68 优势 · §7.1-C / §7.2-C **拖拽与放置** · §7.3 双方表 · §7.4 优先级）。

---

## 0. 一句话结论

两者都是 **Rust 声明式原生 GUI**，都走 **winit + AccessKit**，都强调 **保留模式 + 反应式状态**。分水岭不在「有没有组件库」，而在：

| 轴 | RUI | Freya |
|----|-----|-------|
| **图形栈** | 自研 **wgpu SDF** 管线 | **Skia**（Vulkan / GL / Metal） |
| **布局** | **taffy**（Flex/Grid，偏 Web 语义） | 自研 **torin**（Freya 专用布局） |
| **反应粒度** | **Solid 式**：挂载一次 + `bind_*` 叶子更新 | **Scope 脏标**：`use_state` → `MarkScopeAsDirty` → **组件 `render` 再跑** |
| **书写面** | **双轨**：`ui!` RSX ≡ Builder | **纯 Rust Builder** + `Component` trait（已离开 Dioxus RSX 主路径） |
| **产品美学** | **ShadCN + utility class** | 自带组件 + 可选 **Material Design** |
| **成熟度** | Alpha / 未 crates.io | **0.5 rc**、网站 / docs.rs / 生态 crate 齐全 |

**定位关系**：Freya 是「**可发布的 Skia 声明式桌面库 + 宽生态**」；RUI 是「**Solid 细粒度 × wgpu 自绘 × ShadCN 产品面**」的实验性引擎。  
[`PROJECT.md`](../archive/PROJECT.md) 明确写过：RUI 的零 VDOM 思路曾借鉴 Freya 近期重构——但实现上 RUI 把「细粒度」推到了 **叶子绑定 / 项级 For / Store 路径**，而 Freya 主路径仍是 **组件级重渲染 + PartialEq 调和**（全局细粒度靠 `freya-radio` 等另层）。

**不该做的事**：为对齐 Freya 换 Skia 主线、弃 taffy、把 Scope 脏重渲当成「已等于 Solid」、或照搬 WebView/Camera 等生态面在底座未稳时扩散。

---

## 1. 定位与体量

| 项 | RUI | Freya (`reference/freya-main`) |
|----|-----|--------------------------------|
| **一句话** | 纯 Rust 原生 UI：Solid 细粒度 + Tailwind/Uno 式 utility + ShadCN 组件 | Cross-platform declarative GUI powered by Skia |
| **版本 / 许可** | `0.1.0` · MIT · 工作区自有 | `0.5.0-rc.4` · MIT · [marc2332/freya](https://github.com/marc2332/freya) |
| **成熟度** | Alpha / 实验；API 快迭代 | RC；有官网 [freyaui.dev](https://freyaui.dev)、docs.rs、Discord、Codecov |
| **工作区 crate** | `rui` · `rui_macros` · `rui_style_tokens` · `rui_hot_reload_cli` | **30+** `freya-*` + `torin` + `ragnarok` + `pathgraph` |
| **示例** | `gallery` · `calculator` · `gpu_probe` 等 | 根目录大量 `component_*` / `feature_*` + 子工程（webview / camera / android / ai-chat…） |
| **平台叙事** | 桌面优先（Win 冒烟签署为主） | 桌面三平台 + **实验性 Android** |
| **发布面** | 未上 crates.io；文档在仓内 | crates.io 可依赖；feature 矩阵极完整（`all` / `all-publish`） |

### 1.1 仓库结构对照

```text
RUI                              Freya
crates/rui          单体核心        crates/freya           门面 re-export
crates/rui_macros   ui! / #[component]
crates/rui_style_tokens           crates/freya-core       运行时 / 树 / a11y / 渲染管线
crates/rui_hot_reload_cli         crates/freya-engine     Skia 抽象
examples/gallery                  crates/torin            布局
docs/                             crates/ragnarok         事件
                                  crates/freya-components 组件
                                  crates/freya-winit      窗口
                                  + router / i18n / video / webview / …
                                  website/                Astro 站点
```

Freya 的工程拆分更「产品化」：布局、事件、引擎、组件、平台、插件可独立版本与测试。RUI 刻意保持 **瘦工作区 + 厚内核**，便于 Alpha 期快速改契约，代价是 crate 边界与发布粒度更粗。

---

## 2. 技术栈分层

| 层 | RUI | Freya | 含义 |
|----|-----|-------|------|
| **窗口 / 输入** | `winit` 0.30 | `winit` 0.30 + `freya-winit`；无边框插件；tray | 同族窗口；Freya 壳层更成套 |
| **GPU / 2D** | **wgpu** + 自研 SDF / instance 批绘 | **Skia**（`freya-skia-safe`）：Vulkan 优先，GL 回退，macOS Metal；可 softbuffer | Freya 借成熟 2D；RUI 掌控 shader / 批 / 体积路径 |
| **布局** | **taffy** Flex/Grid | **torin**：方向轴 + Flex/Fit/Normal 等内容模式（非完整 CSS Grid 语义） | RUI 更贴 Web 布局心智；Freya 布局与控件 API 一体设计 |
| **文本** | **glyphon** + **cosmic-text** | **Skia textlayout**（引擎内）+ `freya-edit` / ropey 富文本 | Freya 编辑器/段落更深；RUI 测宽≡绘制与 glyphon 审计是硬工程债 |
| **a11y** | AccessKit MVP（主路径 Win） | AccessKit 集成更深（`freya-core/accessibility`：focus 策略、树、读屏） | 同族；Freya 模块化更完整 |
| **反应存储** | 自研 `Signal` / Owner 图 | **`generational-box`** + Scope 存储（Dioxus 系原语） | Freya 仍吃 Dioxus 生态的一部分（存储 / 热重载 CLI） |
| **事件** | 自研 hit / 冒泡 / capture | **ragnarok**（可复用事件处理器） | Freya 事件可抽库 |
| **样式** | utility class + OKLCH token + 变体前缀 | 元素链式属性 + `theming` / Material | 美学与 DX 哲学不同 |
| **热重载** | 自研 `rui_hot_reload_cli`（TCP） | **`dioxus-cli` `dx serve --hot-patch`**（`hotreload` feature） | 都依赖外部 CLI；实现路径不同 |

### 2.1 图形栈取舍（核心战略差）

| | RUI 选 wgpu | Freya 选 Skia |
|--|-------------|---------------|
| **优势** | 纯 Rust 可控；SDF 圆角阴影与 UI 批绘一体；体积 / 后端 feature 可砍（如默认不开 DX12） | 路径/渐变/模糊/SVG/文字排版工业级；跨后端成熟 |
| **代价** | 自建一切：滤镜、复杂路径、文本路径、GPU 脏区；工程债长 | 原生依赖重（Skia 构建、平台后端）；与「100% 自控 shader」目标相悖 |
| **纲领态度** | ADR：**禁止**为对齐某参考换 Skia 主线（见 drawing / 工程纲领） | Freya 身份即 Skia |

结论：两边 **都合理**，但 **不可混为一谈的「同赛道换皮」**。RUI 若学 Freya，应学 **产品壳与测试/文档**，不是图形后端。

---

## 3. 反应式模型（最关键差异）

### 3.1 Freya：Hooks + Scope 脏 → 组件重 `render`

事实来源：`freya-core` 的 `use_state` / `ReactiveContext` / `runner::MarkScopeAsDirty`；文档 `_docs/state_management.rs`、`lifecycle/base.rs`（Hooks 规则）。

```text
use_state / write
    → ReactiveContext 订阅方收到更新
    → Message::MarkScopeAsDirty(scope_id)
    → Runner 对该 Scope 再执行 Component::render
    → Element 树 PartialEq / DiffKey 调和
```

特征：

- **Hooks 规则**与 React 类似：不可条件调用、不可在循环里调用、不可在 effect 里再挂 hook。
- `State<T>` 是 **Copy** 句柄；`read()` 订阅当前组件；`peek()` 不订阅。
- 组件是 **`PartialEq` 的数据类型** + `Component::render`；props 变则再渲。
- **局部状态默认是组件粒度**；跨组件细粒度靠传递 `State` 或 **`freya-radio`（频道订阅）**。
- 仍保留 Dioxus 系痕迹：`generational-box`、热重载走 `dioxus-devtools` / `dx`。

这是 **声明式 retained UI + 组件级失效**，不是 Solid 的「组件函数只跑一次、属性叶子订阅」。

### 3.2 RUI：Solid 式图 + 叶子绑定 + 结构岛

事实来源：[`REACTIVITY.md`](../REACTIVITY.md)、`core/reactivity.rs`、`app/reactive.rs`。

```text
组件 / #[component] 挂载一次（Owner）
    → create_effect / bind_text|bind_prop|bind_class|bind_style
    → signal.set 只跑仍订阅的 effect
    → LayoutStatus::StyleChanged | LayoutChanged（非整树默认重建）
    → for_keyed_reactive / index_reactive / Store::field（项级 / 字段级）
```

特征：

- **无 Hooks 调用序铁律**（信号可在表达式里读；宏负责降绑）。
- 更新三档：叶子绑 / 结构岛（Show·For）/ 根重建降级路径并存。
- 战略北极星：**超过 Floem 纯度、逼近 Solid 机制 × 无 DOM 桌面栈**（不是追 Freya Scope 模型）。

### 3.3 对照表

| 维度 | RUI | Freya |
|------|-----|-------|
| 组件函数默认几次 | **意图上一次** | **状态变则再 `render`** |
| 文本计数器更新 | `bind_text` → 常 **paint-only** | 读了 state 的组件 **整段 UI 再声明** |
| 列表 | `for_keyed_reactive` 项级 Owner | DiffKey / keyed 调和 + 脏 scope |
| 全局细粒度 | `Store::field` / Context | **`freya-radio` 频道** |
| 异步 | `Resource` 等（自研） | lifecycle future / task + query crate |
| 与 Dioxus 关系 | 明确 **反例**（组件级 VNode） | **存储与热重载仍沾边**；UI 宏已改为自研 Builder |

**对 RUI 的含义**：Freya 证明「声明式 + Copy state + 重 render」可以做出完整产品；**不能**用 Freya 的「也有 State」证明 RUI 细粒度「已经差不多」——机制标尺仍是 Solid / Floem（见 REACTIVITY §5）。

---

## 4. 书写面与组件作者模型

| | RUI | Freya |
|--|-----|-------|
| **主 API** | `ui! { <Button>…}` **与** `button().variant(…)` **同等一等** | `rect()` / `label()` / `Button::new().child(…)` |
| **组件定义** | `#[component] fn Foo() -> Element`（默认 Owner） | `impl Component for Foo { fn render(&self) -> impl IntoElement }` |
| **根组件** | 应用闭包 / 元素树 | `fn() -> impl IntoElement` 或 `App` trait |
| **子节点** | `Element` / Builder / 宏节点互嵌 | `IntoElement`；`String`/`&str` 可直接当 label |
| **样式入口** | `class` / utility / `bind_class` / theme token；**离散属性默认字符串 + 链式**；Builder 可 `Into` 双收预设枚举（见 [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) **§3.1.1**；`variant`/`size` 实例见 [`component_variants.md`](../components/component_variants.md) §0） | 方法链：`.background` `.padding` `.font_size` …；变体多走 **枚举**（如 `ButtonStyleVariant`） |
| **扩展** | primitives + utils 行为下沉 | `KeyExt`、element expansions、插件 crate |

Freya AGENTS 强调：动态改元素时用 `when` / `map` 链式改最后返回值，而不是先存变量再改——这是 **Builder 流畅 API** 纪律。  
RUI 额外押注 **类 HTML 标签宏**，降低从 Web/ShadCN 迁移的心智税，但宏降绑与身份四轴（`id` / `focus_id` / `template_id` / 信号键）是持续工程面。

---

## 5. 布局 · 渲染 · 动画 · 文本

### 5.1 布局

| | RUI (taffy) | Freya (torin) |
|--|-------------|---------------|
| 模型 | Flexbox / CSS Grid 子集 | Direction + Content（Normal / Flex / Fit…）+ Gaps / Position |
| 与 Web | 高对齐 | 自洽 DSL，不完全是 CSS |
| 脏更新 | 子树 dirty MVP + 全树 compute | torin 自带 measure / cache 路径 |
| 虚拟化 | `VirtualList` / `VirtualMasonry` | `VirtualScrollView` 等（components） |

### 5.2 渲染与层

| | RUI | Freya |
|--|-----|-------|
| 图元 | SDF 圆角矩形 instance、文字 glyphon、可选 SVG(resvg)、canvas/scene3d | Skia 画布：路径、阴影、图片、SVG、图层 |
| 叠加 | Portal overlay band、input chrome 分带 | **Layers** 一等文档；Portal 示例与动画结合 |
| 性能叙事 | damage / scissor、scroll CPU 审计、feature 砍后端 | `hotpath`、performance plugin、devtools overlay |

### 5.3 动画

| | RUI | Freya |
|--|-----|-------|
| 路径 | CSS-ish `@keyframes` / transition + `motion` / presence（见 animation 文档） | `freya-animation`：`use_animation`、颜色/尺寸时间轴、Portal 动画示例 |
| 目标 | 对齐 CSS 最大子集（有独立 roadmap） | API 直接、与组件示例绑定紧 |

### 5.4 文本与编辑

| | RUI | Freya |
|--|-----|-------|
| 输入 | Input / Textarea；IME Win 签署 | Input + **富文本 paragraph / editable** + `freya-edit` + **code-editor** crate |
| 选区 | Label 选区、caret chrome | highlights / cursor_index / EditorLine 等更完整编辑面 |
| 测宽 | cosmic-text / glyphon；fit 再布局 | Skia 测量 + text_cache |

Freya 在 **编辑器 / Markdown / Terminal** 上明显更宽；RUI 在 **表单控件信任度 + ShadCN 交互** 上更对齐现代 Web 产品组件。

---

## 6. 样式与主题

| | RUI | Freya |
|--|-----|-------|
| 哲学 | **utility-first**（Tailwind/Uno 心智）+ 语义 token（OKLCH） | **属性方法链** + 组件主题结构 |
| 交互态 | `hover:` / `focus-visible:` / `data-[…]` 变体 | 组件内状态 + theming hooks |
| 暗色 | theme light/dark | theming 系统 |
| 设计系统包 | 内建 ShadCN 味默认 | 可选 **`freya-material-design`**（ripple、MD 控件） |
| 图标 | Lucide（可 bunded / animated JSON） | `freya-icons`（含 Lucide feature） |

RUI 的样式赌注是「**写过 Tailwind 的人零学习曲线**」；Freya 是「**纯 Rust 链式、无 class 字符串**」。两者 DX 优劣取决于目标用户，不是绝对高低。

**变体 API**：Freya 用枚举（常配 `impl Into<…>`）是自然选择（无 `variant="…"` RSX 面）。RUI **保留字符串产品面**（`ui!` / ShadCN / 热重载）；Builder 可学 Freya 做成 `impl Into<XxxVariant>` **双收**，**禁止**删掉 `&str` 只留 `Enum::Variant`。权威口径：[`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) **§3.1.1**（全属性）· [`component_variants.md`](../components/component_variants.md) §0（variant/size 冻结）· §3.1 / §4.3。

---

## 7. 明细清单：你有我没有 · 我有你没有 · 双方都有

> **口径**：「有」= 仓库内可识别的一等 API / crate / 公开组件（含 feature 门控）；不要求同等成熟度。  
> **标记**：`缺口` = Freya 有、RUI 无或明显更浅；`优势` = RUI 有、Freya 无或明显更浅；`双方` = 都有（括号内写深度差）。  
> **行动列**仅作能力表提示，**不是** AUTHORITY 勾选源。  
> 快照：2026-08-31 · `reference/freya-main` · RUI `crates/rui` + inventory。

### 7.0 怎么读

| 列 | 含义 |
|----|------|
| **Freya 证据** | crate / 模块 / feature |
| **RUI 证据** | 路径或「—」 |
| **建议** | `学形态` / `后置能力表` / `明确不做` / `加深` / `保持` |

---

### 7.1 Freya 有、RUI 无或明显更浅（你有我没有）

#### A. 生态 crate / 应用能力（Freya 最大优势面）

| # | 能力 | Freya | RUI | 建议 |
|---|------|-------|-----|------|
| F01 | **WebView** | `freya-webview` · feature `webview` | — | 明确不做（至底座稳）；体积/安全面 |
| F02 | **Video** | `freya-video` · `video` | — | 后置能力表；规范 [`engine/media.md`](../engine/media.md)（核只收帧；播放器独立 crate） |
| F03 | **Camera** | `freya-camera` · `camera` | — | 后置能力表；预览 = ImageStream（[`media.md`](../engine/media.md) M.9） |
| F04 | **Terminal** | `freya-terminal` · `terminal` | — | 后置能力表 |
| F05 | **Code editor** | `freya-code-editor` · `code-editor` | — | 学能力清单；依赖文本/编辑硬化 |
| F06 | **Markdown 渲染包** | `freya-markdown`（可挂 code-editor） | 仅 `text::parse_markdown_inline` 级；**块级 MD 缺失** | 后置；密度项 |
| F07 | **Router + 宏** | `freya-router` + `freya-router-macro`；`Link` / `ActivableRoute` / `AnimatedRouter` / `NativeRouter` | `app/navigator` + `app/router` **MVP** | 加深产品边界；可后置独立 crate |
| F08 | **i18n** | `freya-i18n` · feature `i18n` | 策略/文档有，**无对等 crate** | 后置能力表 |
| F09 | **全局 Radio 状态** | `freya-radio`（频道订阅） | Context + `Store`（模型不同，非同构） | 保持自研；勿搬 Radio API |
| F10 | **Query / 异步数据** | `freya-query` + SQLite 示例 | `Resource` 等自研；无 query crate | 后置；勿为对齐引入 |
| F11 | **Plotters 后端** | `freya-plotters-backend` · `plot` | 自有 `components/chart`（非 plotters） | 保持自研 chart；明确不做 plotters 绑死 |
| F12 | **Material Design 包** | `freya-material-design` | —（ShadCN 默认） | 明确不做换美学 |
| F13 | **Android（实验）** | `freya-android` + example | — | 长期；非巩固期 |
| F14 | **Borderless 插件** | `freya-borderless-plugin` | `window_chrome` 有，**插件化/文档示例更浅** | 学契约；加深壳 |
| F15 | **Titlebar 控件** | `TitlebarButton` · `titlebar` | 无对等一等控件 | 后置 / 与 chrome 合并评估 |
| F16 | **SDK / Tokio 集成** | `freya-sdk` · `sdk-tokio` | 弱 | 后置文档 |
| F17 | **Performance overlay 插件** | `freya-performance-plugin` · `performance` | 无对等插件；有 damage/scroll 审计文档 | 学形态 |
| F18 | **远程 HTTP 资源** | `remote-asset` → Image/Gif HTTP | **`remote-asset` feature**（默认关）：widget 接受 `http(s)` `src`；传输为 [`RemoteAssetFetcher`](../../crates/rui/src/media/remote.rs) 注入。手册 [`remote_asset.md`](../ops/remote_asset.md) | 已落地（入口，非内置 HTTP） |
| F19 | **Zoom 快捷键** | `zoom-shortcuts` | — | P2 |
| F20 | **独立 DevTools App** | `freya-devtools` + `freya-devtools-app` | 进程内 `devtools` Inspector | 学产品形态；不必拆 app |
| F21 | **无头测试一等 crate** | `freya-testing` | `crate_tests` / offscreen / `headless.md` | **学 API 形态（P0）** |
| F22 | **官网 + docs.rs 教程站** | freyaui.dev · `_docs` 进 docs.rs | 仓内 `docs/`；公开站未齐 | W4 / PUBLISH |
| F23 | **crates.io 可 `cargo add`** | `freya = "0.4"` / `0.5-rc` | 未发布 | W4 |
| F24 | **示例矩阵密度** | ~154 根目录示例 + feature_* | gallery / calculator / gpu_probe | 学「一能力一示例」 |
| F25 | **Nix flake / 工具链钉扎** | `flake.nix` · rust-toolchain | 标准 cargo | 可选 |
| F26 | **softbuffer 软件绘回退** | workspace 依赖 | wgpu 路径；无 softbuffer 一等 | 明确不做换主线 |
| F27 | **Skia 多后端**（VK/GL/Metal） | `freya-engine` | wgpu feature（GL/Metal/可选 DX12/VK） | **禁换 Skia** |

#### B. Freya 组件 / 原语侧（RUI 无对等或明显更浅）

| # | 能力 | Freya | RUI | 建议 |
|---|------|-------|-----|------|
| F30 | **Chip** | `chip.rs` | —（近 Badge，语义不全同） | 评估 vs Badge |
| F31 | **Tile** | `tile.rs` | — | 后置密度 |
| F32 | **Link（路由链）** | `link.rs` + router | 无路由一等 Link | 随 Router 加深 |
| F33 | **Skeleton** | `skeleton.rs` | inventory **planned** | 可落地 P1 |
| F34 | **Loader** | `loader.rs` | Spinner **planned**；Progress 有 | 可落地 P1 |
| F35 | **GifViewer** | `gif_viewer.rs` · `gif` | `media/gif` 解码有；**无对等 Viewer 组件** | 后置组件壳 |
| F36 | **ImageViewer**（含远程） | `image_viewer.rs` | `Image` 组件；远程弱 | 加深 |
| F37 | **SvgViewer** | `svg_viewer.rs` | `primitives/svg` + feature `svg` | 双方；Freya 更「查看器」 |
| F38 | **Selectable text** | `selectable_text.rs` | Label 选区有；**一等 Selectable 组件更浅** | 加深 |
| F39 | **Typography 套件** | `typography.rs` | Label / Text + utility | 学命名分层 |
| F40 | **Docking** | `docking.rs` | — | 后置（IDE 向） |
| F41 | **Floating tab** | `floating_tab.rs` | Tabs 有；无 floating tab | 后置 |
| F42 | **Draggable canvas 族** | `draggable_canvas.rs`（`DraggableCanvas` / `Draggable` / `ResizableDraggable`） | `canvas` 有；**无对等可拖子项容器** | 见 **§7.1-C F61–F62** |
| F43 | **Lazy（懒挂载）** | `lazy.rs` | 无对等公开 Lazy | 评估 vs Show/For |
| F44 | **Overflowed content** | `overflowed_content.rs` | truncate/overflow 分散 | 学形态 |
| F45 | **Cursor blink 原语** | `cursor_blink.rs` | 输入 caret 内嵌 | 保持内嵌即可 |
| F46 | **Activable / ActivableContext** | 路由/激活上下文 | 有 `activate_registry`；模型不同 | 保持自研 |
| F47 | **Paragraph 富文本编辑面** | `paragraph` + `freya-edit` + ropey | Input/Textarea；**无 paragraph/edit crate** | 能力表；非 P0 |
| F48 | **VirtualScrollView**（Freya 名） | `scrollviews/virtual_scrollview` | `VirtualList` / `VirtualMasonry` | 双方；命名不同 |
| F49 | **Segmented button** | `segmented_button.rs` | `ToggleGroup`（实验） | 双方近似 |
| F50 | **Theming macros / 组件主题结构** | `theming/*` · `define_theme!` | Theme OKLCH + utility；**无 Freya 式 theme macro** | 保持 token 路线 |
| F51 | **Animation 独立 crate** | `freya-animation` | `style/keyframes` + `utils/motion` + presence | 双方；Freya 示例更成套 |
| F52 | **Layers 一等文档模型** | core `layers` + docs | Portal band / overlay；文档心智不同 | 学体验 |
| F53 | **Torin 布局库（可独立）** | `torin` | `taffy`（锁定） | **禁换** |
| F54 | **Ragnarok 事件库（可独立）** | `ragnarok` | 自研 pointer/events | 学切分；不换栈 |
| F55 | **Tray 示例与 radio 联用** | feature_tray* 示例 | `system-tray` feature | 加深示例 |
| F56 | **HTTP 辅助** | components `http.rs` | — | 后置 |

#### C. 拖拽与放置（DnD · Freya 明显更成套）

> RUI 有**列表重排**与**控件内指针拖**工具（见 §7.2-C），但**没有** Freya 式「类型化 DragZone → DropZone」产品面。勿把 `pointer_drag` / `drag_reorder` 写成已对齐 Freya DnD。

| # | 能力 | Freya | RUI | 建议 |
|---|------|-------|-----|------|
| F57 | **类型化拖放：`DragZone` / `DropZone`** | `freya-components/drag_drop.rs`；payload `T: Clone+PartialEq`；释放调用 `on_drop(T)` | —（无对等公开组件） | **学 API 形态（P1）**；可自研 Solid 式，勿搬 Scope/`use_drag` 存储 |
| F58 | **全局拖中状态 `use_drag<T>()`** | 根 Context 上 `State<Option<T>>`；拖中显示落点预览 | — | 随 F57；可用 Signal/Context 自研 |
| F59 | **拖影 / 阈值 / 进出回调** | `drag_element` · `drag_threshold` · `show_while_dragging` · `on_drag_over(bool)` | — | 随 F57 能力表 |
| F60 | **跨栏看板式 DnD 示例** | `examples/kanban.rs` + `component_drag_drop.rs` | Gallery 仅有 **列表 reorder**（`drag_reorder_handle`），无跨 DropZone 看板 | 学示例矩阵 |
| F61 | **自由定位：`Draggable` / `ResizableDraggable`** | `draggable_canvas.rs` 内；画布上拖移/改尺寸 | —（无对等一等组件） | 后置；与修订后的 F42 同族 |
| F62 | **`DraggableCanvas` 容器** | `DraggableCanvas::new()`；子项可拖 | `primitives/canvas` **不是**可拖子项容器 | 后置 IDE/白板向 |

---

### 7.2 RUI 有、Freya 无或明显更浅（我有你没有）

#### A. 编程模型 / 书写面 / 样式（RUI 战略优势）

| # | 能力 | RUI | Freya | 建议 |
|---|------|-------|-------|------|
| R01 | **Solid 细粒度：`bind_*` 叶子更新** | `bind_text` / `bind_prop` / `bind_class` / `bind_style` | Scope 脏 → 组件再 `render` | **保持北极星** |
| R02 | **项级 For / Index / Store 字段** | `for_keyed_reactive` / `index_reactive` / `Store::field` | 列表偏 DiffKey；细粒度靠 radio | 保持 |
| R03 | **Owner / effect 分相 / 完整图协议** | `core/reactivity`；REACTIVITY §5.7 | Hooks + generational-box | 保持 |
| R04 | **`ui!` RSX 与 Builder 双一等** | ADR-9；prelude 双面 | **仅** Builder + `Component` | 保持；Freya 无标签面 |
| R05 | **`#[component]` 宏** | `rui_macros` | `impl Component` 手写 | 保持 |
| R06 | **utility class / 变体前缀** | `hover:` `focus-visible:` `data-[…]` + `rui_style_tokens` | 方法链属性 | 保持 |
| R07 | **OKLCH 语义 Theme** | `tokens/` + `color_space` | theming 结构不同 | 保持 |
| R08 | **离散属性字符串产品面 + 链式 + `Into` 双收** | [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) **§3.1.1** · [`component_variants.md`](../components/component_variants.md) §0 | 枚举为主 | 保持 |
| R09 | **自研热重载 CLI** | `rui_hot_reload_cli` | 依赖 `dioxus-cli` `dx` | 保持路径；可学 dx UX |
| R10 | **工程纲领 / ADR / CV 门禁** | AUTHORITY · COMPONENT_LIBRARY · inventory 分桶 | 产品文档强、纲领式分桶弱 | 保持 |
| R11 | **taffy Flex/Grid（Web 对齐）** | 锁定 | torin（非完整 CSS Grid） | 保持 |
| R12 | **wgpu SDF 自研管线** | `renderer/` | Skia | 保持；禁换 |
| R13 | **AccessKit 已作主线（相对 Iced 等）** | 有；深度仍不及 Freya 模块切分 | 更深模块 | 加深切分，保持主线 |
| R14 | **damage / scissor / 体积 feature 门** | damage 文档；默认瘦 feature | Skia 构建税 | 保持体积路径 |

#### B. ShadCN 产品组件（Freya 组件库明显更稀或无名）

| # | 能力 | RUI | Freya | 建议 |
|---|------|-----|-------|------|
| R20 | **Dialog（复合模态）** | `dialog` · verified | 多用 `popup`；无同构 ShadCN Dialog | 保持加深 |
| R21 | **Sheet** | `sheet` | — | 保持 |
| R22 | **Drawer** | `drawer` | — | 保持 |
| R23 | **Popover（复合）** | `popover` / `Popover*` | `popup` / portal 近似，复合 API 不同 | 保持 |
| R24 | **Combobox** | `combobox` · verified | Select 有；Combobox 不对位 | 保持 |
| R25 | **Command / CommandPalette** | `command` · Overlay/Dialog | — | 保持 |
| R26 | **Menubar** | `menubar` | `menu` 有；Menubar 应用栏不对位 | 保持 |
| R27 | **NavigationMenu** | `navigation_menu` | sidebar/link 向 | 保持 |
| R28 | **Breadcrumb** | `breadcrumb` | — | 保持 |
| R29 | **Bubble（聊天气泡）** | `bubble` | — | 保持 |
| R30 | **Badge** | `badge` | Chip 近；Badge 不对位 | 保持 |
| R31 | **Avatar** | `avatar` | — | 保持 |
| R32 | **AspectRatio** | `aspect_ratio` | — | 保持 |
| R33 | **Collapsible** | `collapsible` | Accordion 有；独立 Collapsible 浅 | 保持 |
| R34 | **HoverCard** | `hover_card` | Tooltip 有；HoverCard 不对位 | 保持 |
| R35 | **DatePicker / TimePicker** | 均有（experimental） | Calendar 有；Date/TimePicker 不对位 | 加深 |
| R36 | **Form helpers** | `form_provider` / `form_field` / `use_form` | — | 保持实验边界 |
| R37 | **Toast / Toaster 体系** | 六向 position · stack/list · limit/displace · swipe | —（无对等 Sonner 式） | 保持 |
| R38 | **DropdownMenu** | `dropdown_menu` | `menu` / context_menu | 双方；RUI 更贴 ShadCN 命名 |
| R39 | **Separator** | `separator` · verified | 布局间距为主 | 保持 |
| R40 | **ButtonGroup** | `button_group` | Segmented 近 | 保持 |
| R41 | **Toggle / ToggleGroup** | 均有 | Switch/Checkbox/Segmented；无 Toggle 名 | 保持 |
| R42 | **Textarea** | 独立组件 · verified | Input + paragraph/edit 路径不同 | 保持 |
| R43 | **InputAdornment / InputOTP / Kbd** | 均有（experimental） | — | 保持 |
| R44 | **Pagination** | 复合 Pagination* | — | 保持 |
| R45 | **Stepper** | `stepper` | — | 保持 |
| R46 | **Carousel** | `carousel/` 子模块 | — | 保持 |
| R47 | **TreeView** | `tree_view` | — | 保持 |
| R48 | **VirtualMasonry** | `virtual_masonry` · WP-3.10 | VirtualScrollView 有；**瀑布流无对等** | 保持 |
| R49 | **Chart 自研（笛卡尔+极坐标）** | `chart/` + presets + RSX | plotters 后端（不同栈） | 保持自研 |
| R50 | **Navbar** | `navbar` | sidebar/titlebar 向 | 保持 |
| R51 | **Progress（ShadCN 复合）** | `progress` | `progressbar` | 双方近似 |
| R52 | **ScrollArea + Scrollbar 产品封装** | verified 双件 | scrollviews 套件 | 双方；API 不同 |
| R53 | **RadioGroup** | `radio_group` | `radio_item` | 双方 |
| R54 | **Table 复合 + data_table 预设** | Table* + builder | `table.rs` | 双方；RUI 更贴 solid-ui 复合 |
| R55 | **ErrorBoundary** | `app/error_boundary` | — | 保持 |
| R56 | **公开 Svg / Canvas / scene3d** | primitives；`drawing.md` | canvas/svg_viewer；无 scene3d | 保持；scene3d 实验 |
| R57 | **SVG SMIL** | `svg_smil` | — | 实验保持 |
| R58 | **CSS filter / glyph_fx 等自绘特效** | renderer / text | Skia 滤镜另一套 | 保持边界（drawing ADR） |
| R59 | **Portal Esc LIFO + 浮层单一路径** | `portal_overlay` · WP-3.4 | Portal 有；Esc 栈文档化程度不同 | 保持 |
| R60 | **组件 inventory 分桶（verified/experimental/planned）** | `component_inventory.md` | 无对等分桶表 | 保持 |
| R61 | **多窗 / 次窗** | `secondary_window` · multi_window 文档 | 多窗/radio 全局有；深度各有千秋 | 加深冒烟 |
| R62 | **IME 专项手册 + Win 签署** | `ime.md` · desktop_smoke | 平台 IME 有；手册体系不同 | 保持并扩平台 |
| R63 | **Lucide animated recipes** | `lucide-animated` feature | icons-lucide | 双方图标；动画菜谱 RUI 特色 |
| R64 | **Calculator 等小示例 + Gallery 分区** | examples | 示例更多但形态不同 | 学密度，保持 Gallery |

#### C. 拖拽相关（RUI 有、但≠ Freya DnD）

> 下列是 RUI **已有**能力；相对 Freya 的 **DragZone/DropZone 产品面仍是缺口（F57–F60）**。对外叙事勿混为一谈。

| # | 能力 | RUI | Freya | 建议 |
|---|------|-----|-------|------|
| R65 | **列表重排：`drag_reorder` + `drag_reorder_handle`** | `utils/drag_reorder.rs`；`Element`/`BoxBuilder::drag_reorder_handle(list_id, index)`；hover/commit Signal；Gallery GStack 演示 | 可用 DragZone 自拼看板；**无一等 list-reorder helper** | **保持**；补文档/库测；非跨栏 DnD |
| R66 | **控件指针拖会话：`pointer_drag`** | `utils/pointer_drag.rs`；`DragKind`：Slider / ColorPicker / Toast / Carousel / Resizable；`window_events` 统一 tick | 各组件自管拖，无同构全局会话表 | **保持**工程下沉 |
| R67 | **Toast 滑动关闭** | `toast/swipe` 走 `pointer_drag` | — | 保持 |
| R68 | **Carousel / ColorPicker / Slider / Resizable 拖** | 上表 `DragKind` 消费方 | Freya 各有交互，无统一 registry | 保持；与 F57 正交 |

---

### 7.3 双方都有（名称/深度对照）

| 能力 | Freya | RUI | 深度差一句话 |
|------|-------|-----|----------------|
| Button | `button` + MD 变体 | `button` · ShadCN variant/size · verified | RUI 更贴 shadcn 词汇 |
| Input | `input` | `input` · verified | Freya 富文本编辑更深 |
| Checkbox / Switch / Slider | 有 | 有 · verified | 接近 |
| Select | 有 | 有 · verified | RUI + Combobox |
| Accordion | 有 | 有 | 接近 |
| Card | 有 | 有 · verified 复合 | RUI 分区更齐 |
| Calendar | 有 · feature | 有 · experimental | Freya feature 门；RUI + Date/TimePicker |
| Color picker | 有 | 有 · experimental | 接近 |
| Context menu / Menu | 有 | ContextMenu + DropdownMenu + Menubar | RUI 导航/菜单面更宽 |
| Tooltip | 有 | 有 · verified | 接近 |
| Sidebar | 有 | 有 | 接近 |
| Resizable | `resizable_container` | `resizable` | 接近 |
| Table | 有 | 有 · experimental 复合 | RUI 预设更多 |
| Portal / Popup | 有 | Portal + popup utils | 双方；交互契约不同 |
| Scroll / Virtual scroll | scrollviews | ScrollArea / VirtualList / Masonry | RUI 多 Masonry |
| Icons | `freya-icons` | Lucide + animated | 双方 |
| Clipboard | `freya-clipboard` | `clipboard-rs` | 双方 |
| Tray | `tray` feature | `system-tray` | Freya 示例更多 |
| Animation | `freya-animation` | keyframes / motion / presence | Freya 示例更炫；RUI 偏 CSS 对齐 |
| Canvas | `canvas` + **DraggableCanvas 族** | `primitives/canvas` | Freya 可拖子项容器更成套（F61–F62） |
| **拖放（跨组件 payload）** | **`DragZone` / `DropZone` / `use_drag<T>`** | — | **Freya 独有产品面**（F57–F60） |
| **列表重排 helper** | 可自拼 DnD | **`drag_reorder` + `drag_reorder_handle`** | **RUI 更现成**（R65）；≠ 跨栏 DropZone |
| **控件拖会话 registry** | 分散在各控件 | **`pointer_drag`（DragKind）** | **RUI 工程下沉**（R66–R68） |
| SVG | svg_viewer | `primitives/svg` | 双方 |
| Theme | theming macros | OKLCH + utility | 哲学不同 |
| Hot reload | `dx` | 自研 CLI | 路径不同 |
| DevTools | 独立 app | 进程内 Inspector | Freya 产品化更高 |
| a11y | core/accessibility 深 | AccessKit MVP | Freya 更深 |
| Testing | freya-testing | crate_tests / golden | Freya API 更成套 |

---

### 7.4 按优先级压缩的行动视图（仍非勾选源）

**Freya→RUI 值得跟的缺口（建议顺序）**

1. **P0**：无头测试 API 形态（F21）；a11y 模块切分（对标 F 侧深度，非换模型）  
2. **P1**：示例矩阵（F24）；发布/文档站（F22–F23）；Skeleton/Loader（F33–F34）；Router/i18n 产品边界（F07–F08）；Titlebar/无边框契约（F14–F15）；**类型化 DragZone/DropZone 产品面（F57–F60，学形态）**  
3. **P2+ / 能力表**：Markdown 块级、富文本/Code editor、Query；**DraggableCanvas 族（F61–F62）**；**明确不做进默认面**：WebView/Video/Camera/Terminal/Android/Material/Skia/torin（F01–F05、F12–F13、F27、F53）

**RUI 应对外讲清楚的「 Freya 没有 / 更浅」卖点**

1. Solid `bind_*` + 项级列表 + Store 字段（R01–R03）  
2. `ui!` ≡ Builder + utility/ShadCN（R04–R08）  
3. Dialog/Sheet/Toast/Command/Combobox/Bubble 等产品密度（R20–R49）  
4. VirtualMasonry + 自研 Chart + scene3d/自绘纪律（R48–R58）  
5. 分桶 inventory + ADR 工程面（R10、R60）  
6. **列表 `drag_reorder` + 控件 `pointer_drag` 会话下沉（R65–R68）**——同时承认 **跨栏类型化 DnD 仍缺（F57）**

---

### 7.5 组件文件量（粗）

- RUI：`components/` ≈ **90** `.rs`（含子模块 toast/carousel/chart…）  
- Freya：`freya-components` ≈ **67** `.rs` + **大量能力在外围 crate**（§7.1-A）

数量接近时，**Freya 的「完整应用能力」不在 components 目录里**——对照必须看 workspace crate 表，不能只数 `button.rs`。

---

## 8. 工具链 · 测试 · DX · 工程化

| 维度 | RUI | Freya |
|------|-----|-------|
| **无头测试** | `crate_tests` / offscreen golden / headless 文档 | **`freya-testing`** 一等 API |
| **示例密度** | Gallery 驱动 | **154+** 示例文件，feature 级 demo |
| **DevTools** | `devtools` feature Inspector（Elements / 反应图） | `freya-devtools` + **独立 devtools-app** |
| **热重载** | 自研 CLI | `dx serve --hot-patch` |
| **CI / 质量** | 三平台 CI 叙事；golden 仍少 | Codecov、justfile、layout 专项测试、文档测试 |
| **文档** | AUTHORITY 体系强（工程纪律） | `_docs` 模块进 docs.rs + 网站教程 |
| **构建** | 标准 cargo workspace | Nix flake、rust-toolchain、子模块依赖 |
| **Agent 规范** | Cursor rules / 仓内 docs | `AGENTS.md` / `CLAUDE.md` 明确架构约定 |

Freya 在 **「别人能立刻 `cargo add freya` 做出带路由/WebView 的应用」** 上完胜。  
RUI 在 **「领域规范、ADR、反应性不变量、组件 CV 门禁」** 上更「工程纲领化」——适合巩固期自控，但 **对外采用成本仍高于 Freya**。

---

## 9. 成熟度评分（分析判断 1–5）

> 非 benchmark；基于 2026-08-30 源码与文档快照。

| 维度 | RUI | Freya | 备注 |
|------|-----|-------|------|
| 原生桌面深度 | 3 | **4.5** | Freya 平台后端与无边框/托盘/Android 实验 |
| 细粒度反应机制 | **5** | 3 | Freya 组件级脏；radio 另算 |
| 布局能力 / Web 对齐 | **4.5** | 3.5 | taffy Grid vs torin |
| 2D 绘制完备 | 3 | **5** | Skia vs 自研 SDF |
| 文本 / 编辑器 | 3 | **4.5** | Freya edit/code-editor |
| 组件产品感（ShadCN） | **4.5** | 3.5 | Freya 默认更「框架原生」；MD 可选 |
| 生态宽度 | 2 | **5** | webview/video/router/i18n… |
| 测试 / 示例 | 3 | **5** | |
| 发布与文档站 | 2 | **5** | |
| Rust 纯自控 / 体积路径 | **4.5** | 3 | Skia 与构建税 |
| 热重载 DX | 3.5 | **4** | dx 生态成熟 |

---

## 10. 该学 / 不该搬

与纲领「参考各管一层」一致；Freya 专表如下。

| 该学（优先） | 不该搬 | 优先级 |
|--------------|--------|--------|
| **无头测试 API 形态**（`freya-testing` 心智） | 把 Scope 脏重渲当成细粒度终点 | P0 工程 |
| **示例矩阵**：一能力一 `feature_*` / `component_*` | 为对齐示例引入 WebView/Camera 进默认面 | P1 产品 |
| **文档进 crate（`_docs`）+ 网站** 的发布体验 | 换 Skia / 弃 wgpu | **架构禁区** |
| **a11y 模块切分**（focus 策略、树、读屏） | Hooks 调用序规则（与 Solid 路线冲突） | P0/P1 原生 |
| **Router / i18n 的产品边界**（可后置独立 crate） | Dioxus CLI 热重载替换自研（除非明确迁移） | P2 |
| **Layers / Portal 动画成品体验** | torin 替换 taffy | 禁区 |
| **feature 门控与 `all-publish` 发布契约** | Material 替换 ShadCN 默认美学 | P1 发布 |
| **富文本 / 编辑器能力清单**（作缺口表，非照抄 API） | generational-box 替换自研 Signal 图 | 禁区 |
| **Builder `Into` 双收**（字符串 + 枚举）作类型增强 | 废除 `.variant("…")` / 只留 Freya 式枚举产品面 | P2 DX |
| **DragZone/DropZone 产品形态**（payload DnD） | 把 Scope/`use_drag` 当细粒度；或宣称 `drag_reorder`≡Freya DnD | P1 交互 |

### 决策卡片

| 类别 | 内容 |
|------|------|
| **保持** | wgpu + taffy + glyphon；Solid Owner/Signal；双书写；ShadCN 产品面 |
| **吸收** | Freya 的测试/示例/文档/发布纪律；编辑器与多媒体作 **W4+ 能力表** |
| **分清** | Freya「声明式」≠ RUI「细粒度」；两边都声明式，失效粒度不同 |
| **对用户话术** | Freya = Skia 生态桌面库；RUI = Solid×自绘×ShadCN（Alpha） |

---

## 11. 与 RUI 既有对照文档的关系

| 文档 | 角色 |
|------|------|
| [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md) | GPUI / Slint / Xilem / EUI / Iced / egui **六参考** |
| [GPUI_ECOSYSTEM_COMPARISON.md](GPUI_ECOSYSTEM_COMPARISON.md) | GPUI 生态四项目专章 |
| [gap_analysis.md](gap_analysis.md) | Solid / ShadCN / EUI / Pretext 基线 |
| [REACTIVITY.md](../REACTIVITY.md) | Solid / Leptos / Floem / Dioxus 机制；Freya 属「组件级失效」近邻，**不是**细粒度标尺 |
| **本文** | **Freya 专章**：栈、反应、生态、学/禁 |

[`PROJECT.md`](../archive/PROJECT.md) §3.1 提到借鉴 Freya 重构——历史动机有效；**巩固期执行仍以 AUTHORITY + REACTIVITY 为准**，不以 Freya 为反应性北极星。

---

## 12. 能力差距 → 行动映射（非勾选源）

明细编号见 **§7.1 / §7.2 / §7.4**。下表为与巩固期 Waves 的粗映射：

| Freya 强项暴露的 RUI 缺口 | 应对归属 | 说明 |
|---------------------------|----------|------|
| 发布 / 文档站 / crates.io（F22–F23） | W4 / PUBLISH | 不换栈 |
| 无头测试 API 成套（F21） | headless / 测试工程 | 可学 API 形状 |
| Router / i18n 深度（F07–F08） | 后置 crate 或 MVP 加深 | 勿阻塞 Beta 冒烟 |
| Skeleton / Loader（F33–F34） | 组件 P1（inventory planned） | 可落地 |
| 富文本 / code editor（F05、F47） | 能力表；非 P0 | 依赖文本栈硬化 |
| **类型化 DnD（F57–F60）** | 组件 / utils 能力表 | 学 DragZone/DropZone 形态；保持 Solid，勿搬 `use_drag` Scope；**勿与 R65 list reorder 混谈** |
| **DraggableCanvas 族（F61–F62）** | 后置（白板/IDE） | 与 `primitives/canvas` 正交 |
| WebView / Video / Camera / Terminal（F01–F04） | **明确非核心**直至底座稳 | 避免体积与安全面爆炸 |
| Android / Material / Skia / torin（F13、F12、F27、F53） | 长期或禁区 | Freya Android 亦实验；栈禁换 |
| RUI 独有卖点（R01–R64） | 对外叙事 / Gallery | 勿因 Freya 生态宽而贬低 ShadCN×Solid 面 |

---

## 13. 附录：最小 API 气质对比

**Freya（Builder + use_state + 重 render）**

```rust
fn app() -> impl IntoElement {
    let mut count = use_state(|| 0);
    rect()
        .child(format!("Click -> {}", count.read()))
        .on_mouse_up(move |_| *count.write() += 1)
}
```

**RUI（Signal + 挂载一次 + 叶子 / 结构更新；示意）**

```rust
#[component]
fn Counter() -> Element {
    let count = use_signal("count", || 0);
    ui! {
        <VStack>
            {bind_text("c", move || format!("Click -> {}", count.get()))}
            <Button on_click=move |_| count.set(count.get() + 1)>"+"</Button>
        </VStack>
    }
}
```

同一「计数器」demo：Freya 典型路径会 **再执行 `app`/`render` 声明**；RUI 典型路径应 **只更新绑定叶子**（根重建仅为降级/结构路径）。

---

## 14. 总判断（给架构评审用）

1. **Freya 是最接近 RUI 品类的「已成气候」开源近邻之一**：同为 Rust 声明式桌面 GUI，且近年去掉 Dioxus RSX、走向自研 Component/Builder——与 RUI「UI as Rust」同向。  
2. **反应式哲学已经分叉**：Freya = Hooks + Scope 脏；RUI = Solid 图 + bind。对齐 Freya 反应模型等于放弃 ADR-1 北极星。  
3. **图形栈已经分叉**：Skia vs wgpu SDF。RUI 应用 Freya 当 **产品完整度标尺**（路由、编辑、多媒体、测试、发布），不当 **换栈理由**。  
4. **组件美学分叉**：Material/自研主题 vs ShadCN/utility。可共存于市场，不必合并。  
5. **短期（巩固期）**：继续用 Freya 填「示例密度 / 测试 API / 文档发布 / 编辑器缺口表」；用 GPUI/Slint 卡原生冒烟；用 Solid 卡反应性——**Freya 不进入反应性记分牌主标尺**。

---

*快照基于本地 `reference/freya-main` 与当时 RUI 工作区；上游 Freya 若大版本改反应模型，需重审 §3。*
