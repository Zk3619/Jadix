# 组件清单分桶表（C-WP-0.2）

> **文档地位**：规范 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §5 的**控件分桶数据表**；对接 [`AUTHORITY.md`](AUTHORITY.md) WP-CV.* 走查与身份审计。样式承诺面见 §1.8（不进 CV 分桶）。  
> **事实来源**：`crates/rui/src/components/mod.rs` `pub use` · `examples/gallery` 侧栏分区 · [`desktop_smoke.md`](desktop_smoke.md) · `crate_tests/`。  
> **参考分工**（只取所需，不搬架构）：  
> - [`Reference/ShadCN-UI`](../Reference/ShadCN-UI) → 产品面命名 / `variant`·`size` / 分区对标  
> - [`Reference/SolidJS`](../Reference/SolidJS) → 受控 Signal、细粒度 sync、非组件清单  
> - [`Reference/EUI-NEO-main`](../Reference/EUI-NEO-main) → **仅密度标尺**（§3）；禁作 API/架构参照  
> **日期**：2026-08-12（样式承诺面拆出 §1.8；§5.2 → §5.2.1/§5.2.2）；**2026-08-13** §1.10 公开自绘面（含 B16 / B17）  

交叉：[`AUTHORITY.md`](AUTHORITY.md) WP-CV.0 · WP-CV.1 · WP-CV.3 · WP-CV.4 · WP-3.11–3.13 · [`drawing.md`](drawing.md) · [`element_identity.md`](element_identity.md) · [`gap_analysis.md`](gap_analysis.md) §6

---

## 0. 分桶口径

| 桶 | 含义 | 可否进 C1 行为下沉 |
|----|------|-------------------|
| **verified** | 主路径有 Win 冒烟签署 **和/或** 有针对性库测，且无未分类 P0 | 是 |
| **stable-candidate** | 落在规范 §5.2.1 组件稳定面内，Gallery 可演示，**尚未**完成 CV 全量走查 | 仅无 P0 时可进；优先先 CV |
| **experimental** | 已 `pub use`，超出 §5.2.1，或深度/平台差异大；SemVer 可破坏 | 否（除非单独升格） |
| **planned** | ShadCN 对位、**尚未**进 `components/mod.rs`；勿当已导出 | 否；落地后先入 experimental |
| **broken** | 已知 P0（崩溃 / 无法开合 / 无法输入 / 布局塌缩）未修 | **否** |

**硬规则（仅 `components/` 控件）**：`stable-candidate` ⊆ 规范 §5.2.1 组件稳定面；已导出且越出 §5.2.1 → `experimental`；未导出对位 → `planned`（§1.9）。  
**WP-CV.1（2026-08-12）**：§5.2.1 行为控件已 Gallery 手工走查并升 `verified`；**Form** 降出组件稳定面（Context helpers）。Theme / utility **不进 CV**，见 §1.8。  
**WP-CV.3（2026-08-12）**：§2 稳定面文档已手工复核修错；**无未分类 P0**，`broken` = 无（不降级）。  
**WP-CV.4（2026-08-12）**：稳定面 id 分类见 §2.1；空串 I-1 已修。  
**WP-2.8（2026-08-13）**：展示型 `navbar`/`chart`/`progress` 去必填；Toggle `use_scoped_signal`；Gallery 减噪。  
**WP-3.7（2026-08-17）**：Toggle / Checkbox / Switch 共用 `utils/toggle_state`；非受控 Owner 作用域。  
**WP-2.9（2026-08-13）**：盒模型 utility `box-border`/`box-content`；测宽叶审计见 [`box_model.md`](box_model.md) 附录 A（Tabs=ContentBox；Button/Input 等拆叶）。  
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
证据缩写：`smoke#N` = [`desktop_smoke.md`](desktop_smoke.md)；`lib` = `crate_tests/`。

### 1.1 基础（Gallery `basics`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Button | `button` / `Button` / `ButtonBuilder` | **verified** | 可选；空 → **仅内部自动** `btn-N` | smoke#5/#9；`button_*` lib |
| Label | `label` / `Label` / `LabelBuilder`；`text` re-export | **verified** | **展示型不要求**（RSX 仍常传/`宏生成`；空串勿写 focus） | CV.1 可见性走查；选区键见 `label_selection_signal_key` |
| Separator | `separator` / `Separator` | **verified** | **展示型不要求** | CV.1 可见性走查；无用户 id API |
| Card | `Card` / `CardHeader` / `CardTitle` / `CardDescription` / `CardAction` / `CardContent` / `CardFooter` | **verified** | **展示型不要求** | CV.1 可见性走查；复合分区对齐 solid-ui / shadcn；无用户 id API |
| Toggle | `toggle` / `Toggle` | **experimental** | 可空；空 → `resolve_instance_id`；非受控 Owner 作用域 | WP-3.7 与 Checkbox/Switch 共用 `toggle_state`；不在 §5.2.1 |
| ToggleGroup | `toggle_group` / `ToggleGroup` | **experimental** | 必填实例键 | reactive sync lib；≈ EUI `segmented` |
| Progress | `progress` / `Progress` | **experimental** | 可选 / 展示偏多 | 不在 §5.2.1 |
| Image | `Image` | **experimental** | 可选（测试选择器） | 不在 §5.2.1 |

### 1.2 表单（Gallery `forms`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Input | `input` / `Input` / `InputBuilder` | **verified** | 多实例 **必填实例键**；空 → **仅内部自动** `__rui_input_rt_N` | smoke#2/#3/#4/#9；input_* lib |
| Textarea | `textarea` / `Textarea` | **verified** | 同 Input | CV.1 主交互走查；textarea_* lib |
| Checkbox | `checkbox` / `Checkbox` | **verified** | 非受控建议唯一 `id`（Owner 作用域）；受控可选；空 → auto | CV.1 + CV.2；WP-3.7 `toggle_state` |
| Switch | `switch` / `Switch` | **verified** | 同上 | CV.1 + CV.2；WP-3.7 `toggle_state` |
| RadioGroup | `radio_group` / `RadioGroup` | **verified** | 必填 `group_id` | CV.1 走查；reactive sync lib |
| Slider | `slider` / `Slider` | **verified** | 必填实例键 | CV.1 走查；reactive sync lib |
| Form | `form_provider` / `form_field` / `form_validate` / `use_form` | **experimental** | provider **必填实例键**（错误 map 命名空间） | **非独立产品组件**；WP-3.9 ☑：错误文案与 Input invalid 环同源；降出 §5.2.1 |
| Select | `select` / `Select` / `SelectItem` / `SelectGroup` | **verified** | 必填实例键；**锚点用 focus_id**；默认 `match_trigger`；可 `menu_width`/`menu_height`/`min_height`/`max_height`；`scroll_selected`=`none\|start\|center`（默认 none） | CV.1 完整交互走查；smoke#7；`mark_floating`+sync；项左对齐可覆盖；打开时可滚到选中项 |
| Combobox | `combobox` / `ComboboxBuilder` | **verified** | 必填实例键；浮层锚点 | CV.1 完整交互走查；smoke#6 |
| DatePicker | `date_picker` / `DatePicker` | **experimental** | 必填实例键 | WP-3.3：浮层内嵌 [`Calendar`]；ISO `YYYY-MM-DD` / `YYYY-MM-DDTHH:MM:SS`；`datetime-local` 日历下 `Separator` + 时分秒 Combobox；年月下拉复用 [`Select`]；Enter/Esc；本地化未承诺 |
| TimePicker | `time_picker` / `TimePicker` | **experimental** | 必填实例键 | WP-3.3：三个 Combobox 选时/分/秒；`HH:MM:SS`（兼容 `HH:MM`）；输入限 0–23 / 0–59 |
| ColorPicker | `color_picker` / `ColorPicker` | **experimental** | 必填实例键 | WP-3.3：HSV 面积+色相/透明度；`hex`/`rgb`/`hsl`(+alpha)；`variant=inline\|default`；对照 Reader color-picker |
| Calendar | `calendar` / `Calendar` | **experimental** | 必填实例键 | WP-3.3：从 DatePicker 抽出；`caption_layout=label\|dropdown`（dropdown 复用 [`Select`]）；ISO `YYYY-MM-DD`；键盘日/周/月 |
| （能力）FileDialog | `utils::file_dialog` | **experimental** | n/a | **非** `components/` 导出；平台差异 |

### 1.3 浮层 / 反馈（Gallery `overlays`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Dialog | `dialog` / 复合 `Dialog*` / `use_dialog` | **verified** | 必填实例键（`{id}__open`）；子件可派生 | CV.1 开合/焦点陷阱走查；smoke#7；确认框直接组合 Header/Footer；模态壳在 `dialog.rs` |
| Toast | `Toaster` / `toast` / `position`(六向) / `overflow`(`displace`\|`wait`) / `limit` | **verified** | 队列项 **仅内部自动** `u64`；host **展示型不要求** | CV.1 走查；**`limit`=展示上限**；displace 挤出进等待、有空位末尾回归；关闭/拖拽/退场动画；stack 冻全部 / list 冻单条 |
| Tooltip | 复合 `Tooltip`/`TooltipTrigger`/`TooltipContent`（`tooltip_scope`）+ Builder | **verified** | 必填实例键；**锚点用 focus_id**；`ui!` 宏展开 | CV.1 走查（含浮层 Esc） |
| Sheet | `sheet` / `SheetBuilder` | **experimental** | 必填实例键 | ≈ shadcn Sheet / EUI 侧滑；不在 §5.2.1 |
| Popover | `popover_panel` / `PopoverBuilder` | **experimental** | **可选** `.id()`；**锚点用 focus_id** | 低层面板；可不进 prelude |
| Dropdown | `dropdown` / `DropdownBuilder` | **experimental** | 必填实例键；可选 `anchor_focus_id` | 暂仅 Builder |
| ContextMenu | `context_menu` / `ContextMenuBuilder` | **experimental** | 必填实例键 | |
| Command | `command` / 复合 `Command*` / `CommandOverlay`（顶部居中浮层）/ `CommandDialog`（复用 Dialog） / `CommandBuilder` | **experimental** | 必填实例键；动作项自有 id | 浮层默认；`.dialog()` 走 Dialog；↑↓ + Enter |
| （私有）text_edit | `internal::text_edit` | — | 内部 | Input/Textarea chrome；**禁止**产品面 / prelude |

### 1.4 数据（Gallery `data`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Table | `table` / `TableBuilder` / `TableColumn` | **experimental** | 必填实例键；列 `id` 排序键 | 基础 columns+data；分组/筛选取消稳定承诺 |
| VirtualList | `virtual_list` / `VirtualListBuilder` | **experimental** | 必填实例键 | 测宽依赖；Table/Tree 底座 |
| VirtualMasonry | `virtual_masonry` / `VirtualMasonryBuilder` | **experimental** | 必填实例键 | **WP-3.10 ☑**：变高瀑布流虚拟化；`item_height` + 测宽缓存；不进 §5.2.1 |
| TreeView | `tree_view` / `TreeViewBuilder` / `TreeNode` | **experimental** | 必填实例键；节点 id | 大数据实验 |
| Chart | `define_chart` + marks（cartesian `bar_y`/`line_y`/`area_y`/`dot`/`rule_*`；polar `pie`/`radial_bar`/`radial_line`/`radial_area`/`radial_dot`；`custom_mark`）+ `ChartTheme`/`MarkStyle`；`scale_log`/`scale_time`；`.tooltip`/`.crosshair`；预设 `bar_chart`/`line_chart`/`area_chart`/`pie_chart`；RSX `Chart`/`BarChart`…；`ChartDatum`（`series`/`time`） | **experimental** | **展示型不要求**（WP-2.8：无必填 id） | WP-3.2 grammar（对照 TanStack，不搬 D3）；path 核光栅；禁区/可加深见 `drawing.md` §8 |
| Carousel | `carousel` / `CarouselBuilder` | **experimental** | 必填实例键 | |
| Stepper | `stepper` / `StepperBuilder` | **experimental** | 必填实例键 | 产品打磨实验（Gallery 导航区） |

### 1.5 导航（Gallery `navigation`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Tabs | `tabs_scope` / `Tabs` / `TabsList` / `TabsTrigger` / `TabsContent`；另 `tabs_headless` / `tabs_list` | **verified** | 复合必填实例键；无头焦点前缀 `tabstrip:` | CV.1 + CV.2 layout 测 |
| Menubar | `menubar` / `MenubarBuilder` / `MenuSection` | **experimental** | 必填实例键；section/item 派生 focus | 应用命令菜单 |
| Navbar | `navbar` / `NavbarBuilder` | **experimental** | **展示型不要求**（WP-2.8：`navbar()` 无必填 id） | ≠ NavigationMenu ≠ 页面栈 |
| NavigationMenu | `navigation_menu` / … | **experimental** | 必填实例键 | ShadCN 链接导览 |
| Sidebar | `sidebar` / `SidebarBuilder` | **experimental** | 可选（测试选择器常见） | Gallery 壳；布局 |
| （非组件）Navigator / Router | `app::navigator` / `app::router` | — | n/a | 勿当缺失 NavigationMenu |

### 1.6 布局（Gallery `layout`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| ScrollArea | `scroll_area` / `ScrollArea` | **verified**（含 Input 滚动） | 必填实例键（滚动信号 / `scroll:` focus） | smoke#4 |
| Scrollbar | `scrollbar` / `Scrollbar` | **verified** | id 参数存在；常跟 target | CV.1 与 ScrollArea 连带走查 |
| ScrollViewport | `scroll_viewport` / `ScrollViewport` | **experimental** | 必填；`SCROLL_VIEWPORT_FOCUS_PREFIX` | 产品封装；深度实验 |
| Resizable | `resizable` / `ResizableBuilder` | **experimental** | 必填实例键 | 不在 §5.2.1 |

### 1.7 文本（Gallery `text`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Label 选区 | `enable_label_selection` / selection keys | **verified** | 选区信号键由 id 派生 | 与 Label 同行（CV.1） |
| markdown inline | `text::parse_markdown_inline` 等 | **experimental** | n/a | 块级 md **缺失**（密度项） |

### 1.8 样式承诺面（非控件 · 不进 CV · 非 Gallery 分区）

归属规范 §5.2.2 / 工程域（ADR-5）。**不使用** verified / stable-candidate 控件分桶。

| 公开 API | 位置 | SemVer | 验收归属 | 笔记 |
|----------|------|--------|----------|------|
| Theme / `set_theme_mode` | `style/theme.rs` | 0.1 承诺 | WP-2.9、[`color.md`](color.md)、换肤冒烟 | light/dark 与 chrome 同源 |
| utility `class` 主集 | `rui_style_tokens` + `class`/`tw` | 0.1 承诺 | WP-2.9 ☑、[`box_model.md`](box_model.md) | 含 `box-border` / `box-content`；未知类名编译期提示 |

### 1.9 ShadCN 对位 · 尚未实现（`planned`）

对照 [`Reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui`](../Reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui)。  
优先级：**P1** = 桌面产品常见、建议 CV 后下一波补；**P2** = 可组合或可延后；**P3** = Web 味重 / 与已有面重叠，默认不单列。

#### 建议落地（P1 / P2）

| 规划名（ShadCN） | 建议 Gallery 分区 | 优先级 | 建议 id 策略 | 依赖 / 笔记 |
|------------------|-------------------|--------|--------------|-------------|
| **Badge** | 基础 | P1 | 展示型不要求 | 现状用 Label 凑；应独立 `variant`（default/secondary/outline/destructive） |
| **Avatar** | 基础 | P1 | 可选 | Image + 占位字 / 回退；可组合非必须新原语 |
| **Skeleton** | 基础 | P1 | 展示型不要求 | 加载占位；脉冲可用既有动效或静态灰块 |
| **Spinner** | 基础 | P1 | 展示型不要求 | 不确定进度；与 Progress 互补 |
| **Alert** | 浮层 / 反馈 | P1 | 展示型不要求 | **非模态**横幅（≠ Dialog 确认、≠ Toast） |
| **Accordion** | 基础 / 布局 | P1 | 必填实例键（开合） | 单/多开；可复用 Collapsible 原语 |
| **Collapsible** | 基础 / 布局 | P1 | 必填实例键或受控 Signal | Accordion 积木；headless 开合宜下沉 utils |
| **Breadcrumb** | 导航 | P1 | 展示型不要求 | 路径导航；与 Navbar / NavigationMenu 分工 |
| **Pagination** | 数据 | P1 | 可选；受控页码 Signal | 常跟 Table；先控页 API，虚拟滚动另议 |
| **HoverCard** | 浮层 / 反馈 | P2 | 可选；**锚点用 focus_id** | 悬停卡片；复用 popover placement + 延时 |
| **Calendar** | 表单 | — | 必填实例键 | **已落地 experimental**（WP-3.3）；见 §1.2 |
| **Empty** | 数据 / 基础 | P2 | 展示型不要求 | 空状态插画槽 + 标题/描述/CTA |
| **Kbd** | 基础 / 文本 | P2 | 展示型不要求 | 快捷键胶囊；纯展示 |
| **ButtonGroup** | 基础 | P2 | 展示型不要求 | Button 水平/垂直组；圆角合并 |
| **InputOTP** | 表单 | P2 | 必填实例键 | 验证码格；桌面少见但表单完整度需要 |
| **InputGroup** | 表单 | P2 | 跟随 Input | addon 左/右；部分能力已在 Input adornment，可归一文档后再决定是否单列 |

#### 等价已有 · 不单列（P3）

| ShadCN | RUI 等价 | 说明 |
|--------|----------|------|
| Drawer | **Sheet** | 侧滑同一产品位 |
| Sonner | **Toast** | 队列通知同一产品位 |
| DropdownMenu | **Dropdown** | 已有 Builder；补标签属增强 |
| NativeSelect | **Select** | 无 DOM `<select>` |
| Field | **form_field** / Form helpers | 校验 Context 已覆盖字段层；Form **不**单列产品组件 |
| Item | 列表行组合 | 用 HStack + Label；不必先成组件 |
| AspectRatio | 布局约束 | 宜 `primitives` / utility，非产品组件优先 |
| Direction | RTL/LTR | 主题级；未开国际化前不单列 |
| Alert（若只要确认） | 用 **Dialog** Header/Footer | planned Alert = 页内横幅；确认框勿复活 AlertDialog |

#### 明确暂不跟

Web 专有或与底座冲突的 registry 项（若未来出现）一律进本小节「暂不跟」，不进 §5.1 已导出表。

### 1.10 公开自绘面（W3 · `planned` → 落地先 `experimental`）

规范 [`drawing.md`](drawing.md)；执行 AUTHORITY **WP-3.11 / 3.12 / 3.13**；创意库 **B16 / B17**。  
**不是** ShadCN registry 对位；emoji-svg 光栅 ≠ 公开 `<svg>`。

| 规划名 | 建议 Gallery | 优先级 | id 策略 | 笔记 |
|--------|--------------|--------|---------|------|
| **Svg**（`<svg>` / `svg()`） | 绘制 | **experimental**（WP-3.11 ☑） | 展示型不要求 | 矢量场景图；`primitives/svg`；与 WP-3.2 path 核合一；emoji-svg ≠ 本包 |
| **Canvas**（`<canvas>` / `canvas().paint`） | 绘制 | **experimental**（WP-3.12 ☑） | 展示型不要求；paint 内 peek | 自定义 2D 绘制；`primitives/canvas`；非 DOM Canvas2D；同一 path 核 |
| **Scene3d**（`<scene3d>` / `scene3d()`） | 绘制（可选） | **experimental**（WP-3.13 ☑） | 展示型不要求 | feature `scene3d` 默认关；透视立方；无 glTF |
| **canvas-ui 式 fx**（Liquid/Glass/…） | 绘制 / 实验分区 | P2 · B16 | 包住子树；命中仍走底层 Element | 对照 [canvas-ui](https://github.com/DavidHDev/canvas-ui)；**不搬** HTML-in-canvas |
| **react-bits 式 bits**（BlurText / 背景 / …） | 绘制 / 动效实验区 | P2 · B17 | 展示型不要求；动效内 peek | 对照 [react-bits](https://github.com/DavidHDev/react-bits)；glyphon 字形动效 + Canvas 背景；**不搬**每字 DOM span / CSS 运行时 |

---

## 2. 组件稳定面草案（CV 范围 · ≤ 规范 §5.2.1）

| 组件 | 桶（本表） | CV 下一步 |
|------|------------|-----------|
| Button | verified | 保持；CV.2/CV.4 ☑；golden → WP-2.6（Button/Input/Dialog/Tabs/ScrollArea） |
| Input | verified | 保持；CV.2/CV.4 ☑；IME 非 Win 仍开放 |
| Select | verified | 保持；CV.4 空串 auto ☑；CV.2 外扩 layout 可选 |
| Combobox | verified | 同上（scope 统一 resolve） |
| Dialog | verified | 保持；CV.2/CV.4 ☑ |
| ScrollArea / Scrollbar | verified | 保持；CV.4 ScrollArea 空串 auto ☑ |
| Label / Separator / Card | verified | 保持；Label 空 id 不写 tid（CV.4） |
| Textarea / Checkbox / Switch / RadioGroup / Slider | verified | CV.2 Checkbox/Switch ☑；Radio CV.4 auto ☑ |
| Tabs / Toast / Tooltip | verified | Tabs CV.2+CV.4 ☑；Tooltip CV.4 auto ☑ |

**Form**：已降出组件稳定面 → `experimental`（Context helpers；`form_provider` / `form_field` / `form_validate` / `use_form`）。**WP-3.9 ☑**：错误文案与 Input invalid 环同源；不挡 CV.1、不升 `verified`。

**样式承诺面**（Theme / utility）：见 §1.8；**不在本表**，不走 CV 改桶。

**broken**：无（WP-CV.3 签署时确认）。若后续发现阻断项 → 标 `broken` 或降出稳定面，**不得**假装 verified、**不得**进 C1。

**显式不在组件稳定候选**（即使 Gallery 有演示）：**Form** / Toggle / ToggleGroup / Progress / Image / Sheet / Popover / Dropdown / ContextMenu / Command / 一切 Date·Time·Color / Table / VirtualList / **VirtualMasonry** / TreeView / Chart / Carousel / Stepper / Menubar / Navbar / NavigationMenu / Sidebar / ScrollViewport / Resizable / 块级 markdown / FileDialog / **Theme / utility**。  
**全部 `planned`（§1.9）亦不在稳定候选**——落地后先 `experimental`，再经 CV 决定是否升 §5.2.1。

---

## 2.1 WP-CV.4 · `id` 过度使用审计（稳定面）

> **日期**：2026-08-12 · 对照 [`element_identity.md`](element_identity.md) I-1 / I-3 · 真源 `crates/rui/src/components/` + `utils/instance_id.rs`。  
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

对照 [`Reference/EUI-NEO-main/components`](../Reference/EUI-NEO-main) 头文件清单与 gap §6。  
**禁止**：搬 EUI 命令式 DSL、脏矩形架构、GL 后端叙事。

| EUI | RUI | 密度结论 |
|-----|-----|----------|
| button / input / checkbox / switch / radio / slider | 有（稳定面 `verified`） | 齐 |
| tabs / dialog / dropdown / toast / tooltip / contextmenu | 有（分层不同） | 齐（tabs/dialog/toast/tooltip 已 CV.1；dropdown/contextmenu 仍 experimental） |
| scroll / scrollview | ScrollArea / ScrollViewport / Scrollbar | 齐 |
| navbar / sidebar / progress / stepper / carousel | 有，**experimental** | 齐密度、未承诺 semver |
| datatable / virtuallist | Table / VirtualList，experimental | 齐密度 |
| barchart / linechart / piechart | 有 MVP | 齐；Area + Donut RSX |
| datepicker / timepicker / colorpicker | 有，experimental | 齐密度 |
| markdown | inline MVP；**块级缺** | 落后 |
| segmented | 无独立模块；用 **ToggleGroup** | 可接受等价 |
| card / image / panel | Card / Image；panel≈布局原语 | 齐 |
| **virtualmasonry** | **有（experimental）** | WP-3.10 ☑ 密度空洞关闭；升格见 B5 |
| heart_switch / neumorphic / tilt_card / layout_debug | 不跟 | 非目标 |
| mousearea | ≈ `interactive` 原语 | 非组件清单项 |

**计数标尺（约）**：EUI 公开控件头 ≈ 40；RUI `components/` 产品模块 ≈ 44（含复合 Dialog 子 API）。密度已接近；差距在 **信任度（测网/golden/身份审计）** 与个别空洞（块级 md），不在「再抄一套 EUI API」。

---

## 4. ShadCN / Solid 对照摘要

### 4.1 ShadCN registry 覆盖矩阵

源：`Reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui`（56 个 `.tsx`，不含 `_registry`）。

| 状态 | 数量（约） | 项 |
|------|------------|-----|
| **已导出对位** | 34+ | Button、Label、Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Select、Combobox、Card、Separator、Tabs、Dialog、Sheet、Popover、Tooltip、Dropdown、ContextMenu、Menubar、NavigationMenu、Sidebar、ScrollArea、Resizable、Table、Toast、Toggle、ToggleGroup、Progress、Carousel、Chart、Command、Form、… |
| **planned P1** | 9 | Badge、Avatar、Skeleton、Spinner、Alert、Accordion、Collapsible、Breadcrumb、Pagination |
| **planned P2** | 7 | HoverCard、Calendar、Empty、Kbd、ButtonGroup、InputOTP、InputGroup |
| **等价不单列** | 6 | Drawer→Sheet、Sonner→Toast、DropdownMenu→Dropdown、NativeSelect→Select、Field→Form、Item→组合 |
| **布局/主题级** | 2 | AspectRatio、Direction |

完整规划行见 **§1.9**。落地顺序建议：Badge / Skeleton / Spinner / Alert → Accordion+Collapsible → Avatar / Breadcrumb / Pagination → 其余 P2。

### 4.2 SolidJS

不提供组件清单。清单层只继承其编程约束：

- 受控 = 调用方 `Signal`；禁止双源真理（规范 §4.2.5）  
- 细粒度 sync 优先于整树 rebuild（已有 slider/radio/toggle_group sync 测）  
- 组件桶与 Solid 包版本无关

---

## 5. `mod.rs` 一致性检查清单

下列 `pub use` 均已入 **§1.1–1.8**（或标明非产品面）：

`button`, `calendar`, `card`, `carousel`, `chart`(`define_chart`+cartesian/polar marks / `bar_chart`/`line_chart`/`area_chart`/`pie_chart` + log/time/tooltip), `checkbox`, `color_picker`, `command`, `combobox`, `context_menu`, `table`, `date_picker`(+`time_picker`), `dialog`(+复合标签/`use_dialog`), `dropdown`, `form`(+helpers), `image`, `input`, `label`(+selection helpers), `menubar`, `navbar`, `navigation_menu`, `popover`(`popover_panel`), `progress`, `radio_group`, `resizable`, `scroll_area`, `scroll_viewport`, `scrollbar`, `select`, `separator`, `sheet`, `sidebar`, `slider`, `stepper`, `switch`, `tabs`, `textarea`, `toast`, `toggle`, `toggle_group`, `tooltip`(+复合标签/`use_tooltip`), `tree_view`, `virtual_list`, `virtual_masonry`。

**已移除（勿再导出）**：`alert_dialog` / `AlertDialogBuilder` — 确认为 Dialog 薄封装，2026-08-12 废弃；确认 UI 用 `Dialog` + Header/Footer。

未入产品清单：`base::BaseComponentBuilder`（样式底座）、`internal::*`。  
**§1.9 `planned` 不得出现在本列表**，直到首次 `pub use` 并改桶为 `experimental`。

---

## 6. 维护规则

1. 新增 `pub use` → 从 §1.9 删对应 `planned` 行，写入分区表 + 规范 §5.1；默认桶 = `experimental`。  
2. V.1 走查改桶（**仅控件**）：只允许 `stable-candidate` → `verified` / `broken` / 降为 `experimental`。  
3. `broken` 修复后先回 `stable-candidate`，再经证据升 `verified`。  
4. 升格进 §5.2.1 须改规范 + CHANGELOG 草稿，不得只改本表。  
5. 从 ShadCN registry 新增对位意向 → 先加 §1.9 `planned`，禁止直接写进已导出分区表。  
6. Theme / utility 变更 → 改 §1.8 + 规范 §5.2.2 + 工程 WP（WP-2.9 等）；**禁止**用 CV 控件分桶标记。
