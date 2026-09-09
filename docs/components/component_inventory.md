# 组件清单分桶表（C-WP-0.2）

> **文档地位**：规范 [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) §5 的**控件分桶数据表**；对接 [`AUTHORITY.md`](../AUTHORITY.md) WP-CV.* 走查与身份审计。样式承诺面见 §1.8（不进 CV 分桶）。  
> **事实来源**：`crates/rui/src/components/mod.rs` `pub use` · `examples/gallery` 侧栏分区 · [`desktop_smoke.md`](../ops/desktop_smoke.md) · `crate_tests/`。  
> **参考分工**（只取所需，不搬架构）：  
> - [`Reference/ShadCN-UI`](../../reference/ShadCN-UI) → 产品面命名 / `variant`·`size` / 分区对标  
> - [`Reference/SolidJS`](../../reference/SolidJS) → 受控 Signal、细粒度 sync、非组件清单  
> - [`Reference/EUI-NEO-main`](../../reference/EUI-NEO-main) → **密度标尺**（§3）+ **视觉契约**见 [`EUI_NEO_VISUAL.md`](../comparison/EUI_NEO_VISUAL.md)；禁作 API/架构参照  
> **日期**：2026-09-05（Gallery 产品面走查；根目录文档收口）。前史：2026-08-12 CV 分桶；2026-08-13 §1.10 公开自绘面（含 B16 / B17）  

交叉：[`AUTHORITY.md`](../AUTHORITY.md) WP-CV.0 · WP-CV.1 · WP-CV.3 · WP-CV.4 · WP-3.11–3.13 · [`drawing.md`](../drawing.md) · [`element_identity.md`](../element_identity.md) · [`gap_analysis.md`](../comparison/gap_analysis.md) §6

---

## 0. 分桶口径

| 桶 | 含义 | 可否进 C1 行为下沉 |
|----|------|-------------------|
| **verified** | 主路径有 Win 冒烟签署 **和/或** 有针对性库测，且无未分类 P0 | 是 |
| **stable-candidate** | 落在规范 §5.2.1 组件稳定面内，Gallery 可演示，**尚未**完成 CV 全量走查 | 仅无 P0 时可进；优先先 CV |
| **experimental** | 已 `pub use`，超出 §5.2.1，或深度/平台差异大；SemVer 可破坏 | 否（除非单独升格） |
| **planned** | ShadCN 对位、**尚未**进 `components/mod.rs`；勿当已导出 | 否；落地后先入 experimental |
| **broken** | 已知 P0（崩溃 / 无法开合 / 无法输入 / 布局塌缩）未修 | **否** |

**硬规则（仅 `components/` 控件）**：`verified` ⊆ Gallery 可演示且无未分类 P0；已导出但明确不进 SemVer 0.1 → 仍可 `verified`（Gallery）同时在规范 §5.2.1 标实验；未导出对位 → `planned`（§1.9）。  
**WP-CV.1（2026-08-12）**：§5.2.1 行为控件已 Gallery 手工走查并升 `verified`；**Form** 降出组件稳定面（Context helpers）。Theme / utility **不进 CV**，见 §1.8。  
**Gallery 验证（2026-09-05）**：`components/` 产品控件在 Gallery 对应分区已走查；升 `verified`。仍 `experimental`：Form helpers、Chart grammar、VirtualMasonry、Scene3d、Questionnaire、块级 markdown、FileDialog。  
**WP-CV.3（2026-08-12）**：§2 稳定面文档已手工复核修错；**无未分类 P0**，`broken` = 无（不降级）。  
**WP-CV.4（2026-08-12）**：稳定面 id 分类见 §2.1；空串 I-1 已修。  
**WP-2.8（2026-08-13）**：展示型 `navbar`/`chart`/`progress` 去必填；Toggle `use_scoped_signal`；Gallery 减噪。  
**WP-3.7（2026-08-17）**：Toggle / Checkbox / Switch 共用 `utils/toggle_state`；非受控 Owner 作用域。  
**WP-2.9（2026-08-13）**：盒模型 utility `box-border`/`box-content`；测宽叶审计见 [`box_model.md`](../engine/box_model.md) 附录 A（Tabs=ContentBox；Button/Input 等拆叶）。  
**禁止**：把 `planned` 写进 prelude / Gallery 假装可 import；把 Theme/utility 当控件走 CV 改桶。

### id 策略列（对接 C-WP-V.5）

| 标记 | 含义 |
|------|------|
| **必填实例键** | Builder/RSX 首位 `id`；非受控多实例必须稳定唯一，否则 `use_signal("{id}__…")` 撞键 |
| **可选** | 可省略或链式 `.id()`；受控 Signal 时行为不依赖 id |
| **展示型不要求** | 无内部持久信号；勿为「像 HTML」强行编 id |
| **仅内部自动** | 空 id → 进程内递增兜底（跨 rebuild **不**保证稳定） |
| **锚点用 focus_id** | 浮层实例键可派生；可编程焦点 / 锚点另走 `focus_id` / `anchor_focus_id` |

---

## 1. 按 Gallery 分区的公开 API

来源与 `components/mod.rs` `pub use` 对齐。`BaseComponentBuilder`、`internal/*` **不入表**。  
证据缩写：`smoke#N` = [`desktop_smoke.md`](../ops/desktop_smoke.md)；`lib` = `crate_tests/`。

### 1.1 基础（Gallery `basics`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Button | `button` / `Button` / `ButtonBuilder` | **verified** | 可选；空 → **仅内部自动** `btn-N` | smoke#5/#9；`button_*` lib |
| Label | `label` / `Label` / `LabelBuilder`；`text` re-export | **verified** | **展示型不要求**（RSX 仍常传/`宏生成`；空串勿写 focus） | CV.1 可见性走查；选区键见 `label_selection_signal_key` |
| Separator | `separator` / `Separator` | **verified** | **展示型不要求** | CV.1 可见性走查；无用户 id API |
| Card | `Card` / `CardHeader` / `CardTitle` / `CardDescription` / `CardAction` / `CardContent` / `CardFooter` | **verified** | **展示型不要求** | CV.1 可见性走查；复合分区对齐 solid-ui / shadcn；无用户 id API |
| Toggle | `toggle` / `Toggle` | **verified** | 可空；空 → `resolve_instance_id`；非受控 Owner 作用域 | Gallery 2026-09-05；WP-3.7 `toggle_state`；不在 SemVer §5.2.1 |
| ToggleGroup | `toggle_group` / `ToggleGroup` | **verified** | 必填实例键 | Gallery 2026-09-05；≈ EUI `segmented` |
| Progress | `progress` / `Progress*` | **verified** | 可选 / 展示偏多 | Gallery 2026-09-05；ShadCN 复合：Label/Value + Track；`value` ∈ 0..=1 |
| Image | `Image` | **verified** | 可选（测试选择器） | Gallery 2026-09-05 |
| Kbd | `kbd` / `Kbd` / `kbd_group` / `KbdGroup` | **verified** | **展示型不要求** | Gallery 2026-09-05；快捷键胶囊 |
| Badge | `badge` / `Badge` | **verified** | **展示型不要求** | Gallery 2026-09-05 |
| AspectRatio | `aspect_ratio` / `AspectRatio` | **verified** | **展示型不要求** | Gallery 2026-09-05；CSS `aspect-ratio` |
| Skeleton | `skeleton` / `Skeleton` | **verified** | **展示型不要求** | Gallery 2026-09-05 |
| Empty | `empty` / `Empty*` | **verified** | **展示型不要求** | Gallery 2026-09-05 |
| Avatar | `avatar` / `Avatar*` | **verified** | 可选 | Gallery 2026-09-05 |
| Alert | `alert` / `Alert*` | **verified** | **展示型不要求** | Gallery 2026-09-05；页内非模态横幅 |
| AlertDialog | `alert_dialog` / `AlertDialog*` | **verified** | 必填实例键 | Gallery 2026-09-05；阻塞确认（独立于 Dialog） |
| Item / List | `item` / `list` / `Item*` / `List*` | **verified** | **展示型不要求** | Gallery 2026-09-05 |
| Attachment | `attachment` / `Attachment*` | **verified** | **展示型不要求** | Gallery 2026-09-05 |
| Bubble | `bubble` / `Bubble*` | **verified** | 可选 | Gallery 2026-09-05 |
| Marker | `marker` / `Marker*`（`ui_marker`） | **verified** | **展示型不要求** | Gallery 2026-09-05；勿与 SVG `<marker>` 混淆 |
| Message / MessageScroller | `message` / `message_scroller*` | **verified** | 可选；Scroller 必填实例键 | Gallery 2026-09-05 |
| Breadcrumb | `breadcrumb` / `Breadcrumb*` | **verified** | **展示型不要求** | Gallery 2026-09-05 |
| ButtonGroup | `button_group` / `ButtonGroup*` | **verified** | **展示型不要求** | Gallery 2026-09-05 |

### 1.2 表单（Gallery `forms`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Input | `input` / `Input` / `InputBuilder` | **verified** | 多实例 **必填实例键**；空 → **仅内部自动** `__rui_input_rt_N` | smoke#2/#3/#4/#9；input_* lib |
| InputAdornment | `input_adornment` / `InputAdornment*` | **verified** | 外壳展示型不要求；内嵌 Input/Textarea **必填字段 id** | Gallery 2026-09-05；对位 ShadCN InputGroup |
| InputOTP | `input_otp` / `InputOTP*` | **verified** | **必填实例键**；`focus_id=input-otp:{id}` | Gallery 2026-09-05 |
| Textarea | `textarea` / `Textarea` | **verified** | 同 Input | CV.1 主交互走查；textarea_* lib |
| Checkbox | `checkbox` / `Checkbox` | **verified** | 非受控建议唯一 `id`（Owner 作用域）；受控可选；空 → auto | CV.1 + CV.2；WP-3.7 `toggle_state` |
| Switch | `switch` / `Switch` | **verified** | 同上 | CV.1 + CV.2；WP-3.7 `toggle_state` |
| RadioGroup | `radio_group` / `RadioGroup` | **verified** | 必填 `group_id` | CV.1 走查；reactive sync lib |
| Slider | `slider` / `Slider` | **verified** | 必填实例键 | CV.1 走查；reactive sync lib |
| Form | `form_provider` / `form_field` / `form_validate` / `use_form` | **experimental** | provider **必填实例键**（错误 map 命名空间） | **非独立产品组件**；WP-3.9 ☑；降出 §5.2.1 |
| Select | `select` / `Select` / `SelectItem` / `SelectGroup` | **verified** | 必填实例键；**锚点用 focus_id** | CV.1 完整交互走查；smoke#7 |
| Combobox | `combobox` / `ComboboxBuilder` | **verified** | 必填实例键；浮层锚点 | CV.1 完整交互走查；smoke#6 |
| DatePicker | `date_picker` / `DatePicker` | **verified** | 必填实例键 | Gallery 2026-09-05；WP-3.3；本地化未承诺 |
| TimePicker | `time_picker` / `TimePicker` | **verified** | 必填实例键 | Gallery 2026-09-05；`HH:MM:SS` |
| ColorPicker | `color_picker` / `ColorPicker` | **verified** | 必填实例键 | Gallery 2026-09-05 |
| Calendar | `calendar` / `Calendar` | **verified** | 必填实例键 | Gallery 2026-09-05；WP-3.3 |
| Questionnaire | `questionnaire` / `Questionnaire*` | **experimental** | 必填实例键 | Gallery 可演示；DSL 未进 SemVer 0.1 |
| CodeEditor | `code_editor` / `CodeEditor` | **experimental** | **必填 URI**（`DocumentRegistry`） | B19 P4；无 LSP；不进 SemVer 0.1；不进 prelude |
| （能力）FileDialog | `utils::file_dialog` | **experimental** | n/a | **非** `components/` 导出；平台差异 |

### 1.3 浮层 / 反馈（Gallery `overlays`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Dialog | `dialog` / 复合 `Dialog*` / `use_dialog` | **verified** | 必填实例键（`{id}__open`）；子件可派生 | CV.1 开合/焦点陷阱走查；smoke#7 |
| Toast | `Toaster` / `toast` / `position` / `overflow` / `limit` | **verified** | 队列项 **仅内部自动** `u64`；host **展示型不要求** | CV.1 走查 |
| Tooltip | 复合 `Tooltip*`（`tooltip_scope`）+ Builder | **verified** | 必填实例键；**锚点用 focus_id** | CV.1 走查（含浮层 Esc） |
| Sheet | `sheet` / 复合 `Sheet*` / `use_sheet` | **verified** | 必填实例键（`{id}__open`） | Gallery 2026-09-05；侧滑面板 |
| Drawer | `drawer` / 复合 `Drawer*` / `use_drawer` | **verified** | 必填实例键 | Gallery 2026-09-05；vaul 式 snap，**不是** Sheet 别名 |
| Popover | `popover` / `Popover*` · `popover_panel` | **verified** | 复合面必填实例键 | Gallery 2026-09-05 |
| DropdownMenu | `dropdown` / `dropdown_menu` / `DropdownMenu*` | **verified** | 必填实例键；可选 `anchor_focus_id` | Gallery 2026-09-05；标签 + Builder |
| ContextMenu | `context_menu` / `ContextMenu*` | **verified** | 必填实例键 | Gallery 2026-09-05 |
| HoverCard | `hover_card` / `HoverCard*` | **verified** | 可选；**锚点用 focus_id** | Gallery 2026-09-05 |
| Command | `command` / 复合 `Command*` / Overlay / Dialog | **verified** | 必填实例键；动作项自有 id | Gallery 2026-09-05；搜索算法可加深 |
| （私有）text_edit | `internal::text_edit` | — | 内部 | Input/Textarea chrome；**禁止**产品面 / prelude |

### 1.4 数据（Gallery `data`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Table | `Table*`；预设 `table`/`data_table`/`table_compact`/`table_comfortable` | **verified** | 数据表必填实例键；列 `id` 排序键 | Gallery 2026-09-05；分组/pinning 不承诺 |
| VirtualList | `virtual_list` / `VirtualListBuilder` | **verified** | 必填实例键 | Gallery 2026-09-05；测宽依赖 |
| VirtualMasonry | `virtual_masonry` / `VirtualMasonryBuilder` | **experimental** | 必填实例键 | **WP-3.10 ☑**；不进 SemVer 0.1 |
| TreeView | `tree_view` / `TreeView*` / `TreeNode` | **verified** | 必填实例键；节点 id | Gallery 2026-09-05 |
| Chart | `define_chart` + marks + 预设 + RSX | **experimental** | **展示型不要求** | WP-3.2 grammar；禁区见 `drawing.md` §8 |
| Carousel | `carousel` / `Carousel*` | **verified** | 必填实例键 | Gallery 2026-09-05 |
| Stepper | `stepper` / `Stepper*` | **verified** | 可选实例键 | Gallery 2026-09-05；非官方 ShadCN primitive |
| Timeline | `timeline` / `Timeline*` | **verified** | 可选实例键 | Gallery 2026-09-05；对齐 solid-ui timeline |
| Pagination | `pagination` / `Pagination*` | **verified** | 可选；受控页码 Signal | Gallery 2026-09-05 |

### 1.5 导航（Gallery `navigation`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Tabs | `tabs_scope` / `Tabs*`；另 `tabs_headless` | **verified** | 复合必填实例键；无头焦点前缀 `tabstrip:` | CV.1 + CV.2 layout 测 |
| Menubar | `menubar` / `Menubar*` | **verified** | 必填实例键；section/item 派生 focus | Gallery 2026-09-05 |
| Navbar | `navbar` / `Navbar*` | **verified** | **展示型不要求** | Gallery 2026-09-05；≠ NavigationMenu ≠ 页面栈 |
| NavigationMenu | `navigation_menu` / … | **verified** | 必填实例键 | Gallery 2026-09-05 |
| Sidebar | `sidebar` / `Sidebar*` | **verified** | 可选（测试选择器常见） | Gallery 2026-09-05 |
| （非组件）Navigator / Router | `app::navigator` / `app::router` | — | n/a | 勿当缺失 NavigationMenu |

### 1.6 布局（Gallery `layout`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| ScrollArea | `scroll_area` / `ScrollArea` | **verified**（含 Input 滚动） | 必填实例键（滚动信号 / `scroll:` focus） | smoke#4 |
| Scrollbar | `scrollbar` / `Scrollbar` | **verified** | id 参数存在；常跟 target | CV.1 与 ScrollArea 连带走查 |
| ScrollViewport | `scroll_viewport` / `ScrollViewport` | **verified** | 必填；`SCROLL_VIEWPORT_FOCUS_PREFIX` | Gallery 2026-09-05 |
| Resizable | `resizable` / 复合 `ResizablePanelGroup`+`Panel`+`Handle` | **verified** | 可选实例键 | Gallery 2026-09-05 |
| Accordion | `accordion` / `Accordion*` | **verified** | 必填实例键（开合） | Gallery 2026-09-05；ANIM：`used_layout` 驱动 `accordion-down/up`，无组件测高 |
| Collapsible | `collapsible` / `Collapsible*` | **verified** | 必填实例键或受控 Signal | Gallery 2026-09-05；ANIM：同 Accordion，读引擎 `used_layout` |

### 1.7 文本（Gallery `text`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Label 选区 | `enable_label_selection` / selection keys | **verified** | 选区信号键由 id 派生 | 与 Label 同行（CV.1） |
| markdown inline | `text::parse_markdown_inline` 等 | **experimental** | n/a | 块级 md **缺失**（密度项） |

### 1.8 样式承诺面（非控件 · 不进 CV · 非 Gallery 分区）

归属规范 §5.2.2 / 工程域（ADR-5）。**不使用** verified / stable-candidate 控件分桶。

| 公开 API | 位置 | SemVer | 验收归属 | 笔记 |
|----------|------|--------|----------|------|
| Theme / `set_theme_mode` | `style/theme.rs` | 0.1 承诺 | WP-2.9、[`theme.md`](../engine/theme.md)、换肤冒烟 | light/dark 与 chrome 同源 |
| Theme 文件覆盖（计划） | `style/theme_load.rs`（WP-STY.0） | 0.1 承诺 | [`plans/global-stylesheet-loader.md`](../plans/global-stylesheet-loader.md) Phase 0 | `.toml`/`.json`/`.css` `:root` 变量；`ThemeColorPatch` |
| 全局样式表（计划） | `style/stylesheet/`（WP-STY.2+） | 发版后 | 同上 Phase 1–3 | 有限选择器 → utility；**非** CSSOM |
| utility `class` 主集 | `rui_style_tokens` + `class`/`tw` | 0.1 承诺 | WP-2.9 ☑、[`box_model.md`](../engine/box_model.md) | 含 `box-border` / `box-content`；未知类名编译期提示 |

### 1.9 ShadCN 对位 · 尚未实现（`planned`）

对照 [`Reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui`](../../reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui)。  
优先级：**P1** = 桌面产品常见、建议 CV 后下一波补；**P2** = 可组合或可延后；**P3** = Web 味重 / 与已有面重叠，默认不单列。

#### 建议落地（P1 / P2）

| 规划名（ShadCN） | 建议 Gallery 分区 | 优先级 | 建议 id 策略 | 依赖 / 笔记 |
|------------------|-------------------|--------|--------------|-------------|
| **Spinner** | 基础 | P1 | 展示型不要求 | 不确定进度；与 Progress 互补；**尚未** `pub use` |

上表以外的原 P1/P2（Avatar / Skeleton / Alert / Accordion / Collapsible / Breadcrumb / Empty / ButtonGroup / Badge / AspectRatio / Pagination / HoverCard / Calendar / Kbd / InputOTP / InputAdornment / AlertDialog / Drawer）**已落地**，见 §1.1–1.6。

#### 等价已有 · 不单列（P3）

| ShadCN | RUI 等价 | 说明 |
|--------|----------|------|
| Sonner | **Toast** | 队列通知同一产品位 |
| DropdownMenu | **DropdownMenu** | 已有标签 + Builder |
| NativeSelect | **Select** | 无 DOM `<select>` |
| Field | **form_field** / Form helpers | 校验 Context 已覆盖字段层 |
| Item | **Item** / **List** | 已落地复合行 |
| Direction | RTL/LTR | 主题级；未开国际化前不单列 |

Drawer **不是** Sheet 别名（独立 snap）。AlertDialog **不是** Dialog 薄封装（独立阻塞确认）。

#### 明确暂不跟

Web 专有或与底座冲突的 registry 项（若未来出现）一律进本小节「暂不跟」，不进 §5.1 已导出表。

### 1.10 公开自绘面（W3 · `planned` → 落地先 `experimental`）

规范 [`drawing.md`](../drawing.md)；执行 AUTHORITY **WP-3.11 / 3.12 / 3.13**；创意库 **B16 / B17**；动态纹理槽 **B18**（[`media.md`](../engine/media.md)，不挡 0.1）。  
**不是** ShadCN registry 对位；emoji-svg 光栅 ≠ 公开 `<svg>`。

| 规划名 | 建议 Gallery | 优先级 | id 策略 | 笔记 |
|--------|--------------|--------|---------|------|
| **Svg**（`<svg>` / `svg()`） | 绘制 | **experimental**（WP-3.11 ☑） | 展示型不要求 | 矢量场景图；`primitives/svg`；与 WP-3.2 path 核合一；emoji-svg ≠ 本包 |
| **Canvas**（`<canvas>` / `canvas().paint`） | 绘制 | **experimental**（WP-3.12 ☑） | 展示型不要求；paint 内 peek | 自定义 2D 绘制；`primitives/canvas`；非 DOM Canvas2D；同一 path 核 |
| **Scene3d**（`<scene3d>` / `scene3d()`） | 绘制（可选） | **experimental**（WP-3.13 ☑） | 展示型不要求 | feature `scene3d` 默认关；透视立方；无 glTF |
| **canvas-ui 式 fx**（Liquid/Glass/…） | 绘制 / 实验分区 | P2 · B16 | 包住子树；命中仍走底层 Element | 对照 [canvas-ui](https://github.com/DavidHDev/canvas-ui)；**不搬** HTML-in-canvas |
| **react-bits 式 bits**（BlurText / 背景 / …） | 绘制 / 动效实验区 | P2 · B17 | 展示型不要求；动效内 peek | 对照 [react-bits](https://github.com/DavidHDev/react-bits)；glyphon 字形动效 + Canvas 背景；**不搬**每字 DOM span / CSS 运行时 |
| **MediaSurface / ImageStream** | 绘制（可选） | **experimental** · B18 | 展示型不要求 | `queue_media_frame` + `<Image texture>`；RGBA ☑；`image-stream` YUV ☑；**不是**播放器；规范 [`media.md`](../engine/media.md) |

---

## 2. 组件稳定面草案（CV 范围 · ≤ 规范 §5.2.1）

| 组件 | 桶（本表） | CV 下一步 |
|------|------------|-----------|
| Button | verified | 保持；CV.2/CV.4 ☑；golden → WP-2.6 |
| Input | verified | 保持；IME 非 Win 仍开放 |
| Select / Combobox | verified | 保持 |
| Dialog | verified | 保持 |
| ScrollArea / Scrollbar | verified | 保持 |
| Label / Separator / Card | verified | 保持 |
| Textarea / Checkbox / Switch / RadioGroup / Slider | verified | 保持 |
| Tabs / Toast / Tooltip | verified | 保持 |
| 其余 Gallery 产品控件（§1.1–1.6） | verified | **2026-09-05 走查**；**不自动写入 SemVer 0.1**（规范 §5.2.1） |

**Form**：已降出组件稳定面 → `experimental`。**WP-3.9 ☑**。

**仍 experimental（即使 Gallery 有演示）**：Form helpers · Chart grammar · VirtualMasonry · Scene3d · Questionnaire · CodeEditor · FileDialog · 块级 markdown · Theme / utility。

**broken**：无。

**显式不在 SemVer 0.1**（Gallery 可为 `verified`）：见规范 §5.2.1 右列。W4 / WP-4.1 决定谁升承诺面。

---

## 2.1 WP-CV.4 · `id` 过度使用审计（稳定面）

> **日期**：2026-08-12 · 对照 [`element_identity.md`](../element_identity.md) I-1 / I-3 · 真源 `crates/rui/src/components/` + `utils/instance_id.rs`。  
> **范围**：§2 `verified` 控件。experimental 仅作备注，不挡 CV.4。  
> **空串口径**：`resolve_instance_id` → `__rui_{kind}_rt_N`（会话递增，跨 rebuild 不保证）；展示型 Label 空 id → **不写** `template_id`。

| 组件 | API | 空 id | 写入 focus / template | 信号键 | 锚点 | I-1 / 笔记 |
|------|-----|-------|----------------------|--------|------|------------|
| Button | 首位可空 | **auto** `btn-N` | focus=裸 id；template=id | 无（全局焦点键） | — | 已合规 |
| Label | 首位可空 | 展示：**不写** tid；selectable → auto | selectable → focus | 选区 `__rui_label_selection_{id}` | `for` 指他控 | I-1 ☑（CV.4 修） |
| Separator / Card | **无用户 id** | n/a | 无 | 无 | — | 展示型目标态 |
| Input | 首位可空 | **auto** `__rui_input_rt_N` | `input:{id}`；tid 派生 | value/caret/… | — | 已合规 |
| Textarea | 首位可空 | **auto** `__rui_textarea_rt_N` | `textarea:{id}` | 同 Input + h 信号 | 内 scroll | 已合规 |
| Checkbox / Switch | 首位可空 | **auto** `__rui_*_rt_N` | 裸 id | Owner 作用域 `{id}__checkbox` / `{id}__switch`（WP-3.7） | — | 已合规 |
| Slider | 首位可空 | **auto** `__rui_*_rt_N` | 裸 id | `{id}__slider` 等全局键 | — | 已合规 |
| RadioGroup | 首位 `group_id` | **auto** `__rui_radio_rt_N` | 项 `{g}:{v}` | `{id}__radio` | — | I-1 ☑（CV.4 修） |
| Select | 必填位（可空） | **auto** `__rui_select_rt_N` | `{id}:trigger` | `__select_*` | floating → trigger | I-1/I-3 ☑（CV.4 修） |
| Combobox | 必填位（可空） | **auto** `__rui_combo_rt_N`（scope 统一） | `input:{id}` | `__combo_*` | floating → `input:{id}` | 锚点与 Input 同 id（CV.4 修） |
| Dialog | 必填位（可空） | **auto** `__rui_dialog_rt_N` | Trigger 派生 | `{id}__open` | portal | I-1 ☑（CV.4 修） |
| Tooltip | 必填位（可空） | **auto** `__rui_tooltip_rt_N` | `__tooltip_trigger` | `__tooltip_open` | floating | I-1 ☑（CV.4 修） |
| Toast | Host 无 id；项 `u64` | n/a | `toast-card-{u64}` | 全局队列 | — | 已合规 |
| Tabs | 复合必填（可空） | **auto** `__rui_tabs_rt_N` | `{id}:{value}` / `tabstrip:` | `{id}__tabs` | content show | I-1 ☑（CV.4 修） |
| ScrollArea | 首位可空 | **auto** `__rui_scroll_rt_N` | `scroll:{id}` | viewport 信号 | Scrollbar target | I-1 ☑（CV.4 修） |
| Scrollbar | `id` 未用于身份；要 `target` | n/a | `scrollbar-*-{target}` | visibility by target | 跟 SA | 无空 focus 写入 |

### Gallery 示范 id（勿写成 API 要求）

- Button 大量**省略 id**（`btn-N`）；焦点/测引用才写 `gal-*`（WP-2.8 Gallery 减噪已起步）。  
- Input：冒烟 `gal-s2-input-a`、密码测 `gal-s2-input-pw`；其余同行 Input 省略（auto）。  
- Textarea/Combobox/Tabs：`gal-textarea`、`gal-combo`、`gal-tabs*`（多实例）。  
- Checkbox/Switch/Radio/Slider：保留 `gallery_*` 多实例键。  
- Dialog/Tooltip：保留实例键；Trigger 按钮可省略。  
- 分区壳 ScrollArea：`scroll-{section}`；inset 演示 `sa-*`。  
- 文档探针 Label：`gal-s6-label-cjk` / `gal-s11-virt-title` / bidi·emoji 回归 id。

### CV.4 结论

- **稳定面 id 策略已分类**（本表 + §1 各行「id 策略」列）。  
- **I-1 空串 `Some("")`**：稳定面已修（`resolve_instance_id` / Label 不写 tid）；桥侧防御测保留。  
- **WP-2.8 ☑**：展示型 `navbar`/`chart`/`progress` 去必填；Toggle `use_scoped_signal`；Gallery 减噪。显式同 id 多实例仍属调用方契约（I-3）。  
- **WP-3.7 ☑**：Toggle / Checkbox / Switch 非受控改为 Owner 作用域 `toggle_state`（不再用全局 `{id}__checkbox`）。  
- **experimental**：升格前须对齐本表；Progress 空 id 已走 `resolve_instance_id`。

---

## 3. EUI 密度标尺（只保留完整度对照）

对照 [`Reference/EUI-NEO-main/components`](../../reference/EUI-NEO-main) 头文件清单与 gap §6。  
**禁止**：搬 EUI 命令式 DSL、脏矩形架构、GL 后端叙事。  
**视觉 / workshop**：控件密度「不跟」≠ 禁止参考 2.5D / Shadertoy / 创意分桶；那一层见 [`EUI_NEO_VISUAL.md`](../comparison/EUI_NEO_VISUAL.md)，产品目录挂 B16/B17，**不进** SemVer 稳定面。

| EUI | RUI | 密度结论 |
|-----|-----|----------|
| button / input / checkbox / switch / radio / slider | 有（稳定面 `verified`） | 齐 |
| tabs / dialog / dropdown / toast / tooltip / contextmenu | 有（分层不同） | 齐（产品面 Gallery `verified`；tabs/dialog/toast/tooltip 另有 CV 测） |
| scroll / scrollview | ScrollArea / ScrollViewport / Scrollbar | 齐 |
| navbar / sidebar / progress / stepper / carousel | 有，Gallery `verified`（未全部进 SemVer 0.1） | 齐密度 |
| datatable / virtuallist | Table / VirtualList，Gallery `verified` | 齐密度 |
| barchart / linechart / piechart | 有 MVP | 齐；Area + Donut RSX |
| datepicker / timepicker / colorpicker | 有，Gallery `verified` | 齐密度 |
| markdown | inline MVP；**块级缺** | 落后 |
| segmented | 无独立模块；用 **ToggleGroup** | 可接受等价 |
| card / image / panel | Card / Image；panel≈布局原语 | 齐 |
| **virtualmasonry** | **有（experimental）** | WP-3.10 ☑ 密度空洞关闭；升格见 B5 |
| heart_switch / neumorphic / tilt_card / layout_debug | **不进稳定面**；tilt/新拟态心智 → B16/B17（[`EUI_NEO_VISUAL.md`](../comparison/EUI_NEO_VISUAL.md)） | 创意目录参考，非 shadcn 默认 |
| mousearea | ≈ `interactive` 原语 | 非组件清单项 |

**计数标尺（约）**：EUI 公开控件头 ≈ 40；RUI `components/` 产品模块 ≈ 70。密度已超过标尺；剩余差距在 **SemVer 承诺面宽度**、golden 覆盖与个别空洞（块级 md / Spinner），不在「再抄一套 EUI API」。

---

## 4. ShadCN / Solid 对照摘要

### 4.1 ShadCN registry 覆盖矩阵

源：`Reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui`（56 个 `.tsx`，不含 `_registry`）。

| 状态 | 数量（约） | 项 |
|------|------------|-----|
| **已导出对位** | 55+ | Button、Badge、Kbd、AspectRatio、Avatar、Skeleton、Empty、Alert、AlertDialog、Label、Input、InputAdornment、InputOTP、Textarea、Checkbox、Switch、RadioGroup、Slider、Select、Combobox、Card、Separator、Tabs、Dialog、Sheet、Drawer、Popover、Tooltip、HoverCard、DropdownMenu、ContextMenu、Menubar、NavigationMenu、Sidebar、ScrollArea、Resizable、Table、Pagination、Toast、Toggle、ToggleGroup、Progress、Carousel、Chart、Command、Form、Accordion、Collapsible、Breadcrumb、ButtonGroup、Calendar、Item、List、… |
| **planned P1** | 1 | Spinner |
| **等价不单列** | 4 | Sonner→Toast、NativeSelect→Select、Field→Form、Direction |
| **布局/主题级** | 1 | Direction（重申） |

完整规划行见 **§1.9**。落地顺序建议：Spinner；块级 markdown（密度项，非 ShadCN registry）。

### 4.2 SolidJS

不提供组件清单。清单层只继承其编程约束：

- 受控 = 调用方 `Signal`；禁止双源真理（规范 §4.2.5）  
- 细粒度 sync 优先于整树 rebuild（已有 slider/radio/toggle_group sync 测）  
- 组件桶与 Solid 包版本无关

---

## 5. `mod.rs` 一致性检查清单

下列 `pub use` 均已入 **§1.1–1.8**（或标明非产品面）：

`accordion`, `alert`, `alert_dialog`, `aspect_ratio`, `attachment`, `avatar`, `badge`, `breadcrumb`, `bubble`, `button`, `button_group`, `calendar`, `card`, `carousel`, `chart`, `checkbox`, `collapsible`, `color_picker`, `combobox`, `command`, `context_menu`, `date_picker`(+`time_picker`), `dialog`, `drawer`, `dropdown_menu`, `empty`, `form`, `hover_card`, `image`, `input`, `input_adornment`, `input_otp`, `item`, `kbd`, `label`, `list`, `marker`, `menubar`, `message`, `message_scroller`, `navbar`, `navigation_menu`, `pagination`, `popover`, `progress`, `questionnaire`, `radio_group`, `resizable`, `scroll_area`, `scroll_viewport`, `scrollbar`, `select`, `separator`, `sheet`, `sidebar`, `skeleton`, `slider`, `stepper`, `switch`, `table`, `tabs`, `textarea`, `timeline`, `toast`, `toggle`, `toggle_group`, `tooltip`, `tree_view`, `virtual_list`, `virtual_masonry`。

未入产品清单：`base::BaseComponentBuilder`（样式底座）、`internal::*`。  
**§1.9 `planned` 不得出现在本列表**，直到首次 `pub use` 并改桶。

---

## 6. 维护规则

1. 新增 `pub use` → 从 §1.9 删对应 `planned` 行，写入分区表 + 规范 §5.1；默认桶 = `experimental`。  
2. V.1 走查改桶（**仅控件**）：只允许 `stable-candidate` → `verified` / `broken` / 降为 `experimental`。  
3. `broken` 修复后先回 `stable-candidate`，再经证据升 `verified`。  
4. 升格进 §5.2.1 须改规范 + CHANGELOG 草稿，不得只改本表。  
5. 从 ShadCN registry 新增对位意向 → 先加 §1.9 `planned`，禁止直接写进已导出分区表。  
6. Theme / utility 变更 → 改 §1.8 + 规范 §5.2.2 + 工程 WP（WP-2.9 等）；**禁止**用 CV 控件分桶标记。
