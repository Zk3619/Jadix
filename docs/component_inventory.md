# 组件清单分桶表（C-WP-0.2）

> **文档地位**：规范 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §5 的**可执行分桶附录**；对接 C-WP-V.1 走查、C-WP-V.5 身份审计、CV 门禁。  
> **事实来源**：`crates/rui/src/components/mod.rs` `pub use` · `examples/gallery` 侧栏分区 · [`desktop_smoke.md`](desktop_smoke.md) · `crate_tests/`。  
> **参考分工**（只取所需，不搬架构）：  
> - [`Reference/ShadCN-UI`](../Reference/ShadCN-UI) → 产品面命名 / `variant`·`size` / 分区对标  
> - [`Reference/SolidJS`](../Reference/SolidJS) → 受控 Signal、细粒度 sync、非组件清单  
> - [`Reference/EUI-NEO-main`](../Reference/EUI-NEO-main) → **仅密度标尺**（§3）；禁作 API/架构参照  
> **日期**：2026-08-10  

交叉：[`COMPONENT_LIBRARY_PLAN.md`](COMPONENT_LIBRARY_PLAN.md) C-WP-0.2 · [`element_identity.md`](element_identity.md) · [`gap_analysis.md`](gap_analysis.md) §6

---

## 0. 分桶口径

| 桶 | 含义 | 可否进 C1 行为下沉 |
|----|------|-------------------|
| **verified** | 主路径有 Win 冒烟签署 **和/或** 有针对性库测，且无未分类 P0 | 是 |
| **stable-candidate** | 落在规范 §5.2 稳定面草案内，Gallery 可演示，**尚未**完成 CV 全量走查 | 仅无 P0 时可进；优先先 CV |
| **experimental** | 已 `pub use`，超出 §5.2，或深度/平台差异大；SemVer 可破坏 | 否（除非单独升格） |
| **planned** | ShadCN 对位、**尚未**进 `components/mod.rs`；勿当已导出 | 否；落地后先入 experimental |
| **broken** | 已知 P0（崩溃 / 无法开合 / 无法输入 / 布局塌缩）未修 | **否** |

**硬规则**：`stable-candidate` ⊆ 规范 §5.2；已导出且越出 §5.2 → `experimental`；未导出对位 → `planned`（§1.9）。  
**CV 前**：多数稳定面仍为 `stable-candidate`；`verified` 仅给已有证据的窄主路径。  
**禁止**：把 `planned` 写进 prelude / Gallery 假装可 import。

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
| Label | `label` / `Label` / `LabelBuilder`；`text` re-export | **stable-candidate** | **展示型不要求**（RSX 仍常传/`宏生成`；空串勿写 focus） | 选区键见 `label_selection_signal_key` |
| Separator | `separator` / `Separator` | **stable-candidate** | **展示型不要求** | 无用户 id API |
| Card | `Card` / `CardHeader` / `CardTitle` / `CardDescription` / `CardAction` / `CardContent` / `CardFooter` | **stable-candidate** | **展示型不要求** | 复合分区对齐 solid-ui / shadcn；无用户 id API |
| Toggle | `toggle` / `Toggle` | **experimental** | 必填实例键（非受控） | 不在 §5.2；Gallery 表单区演示 |
| ToggleGroup | `toggle_group` / `ToggleGroup` | **experimental** | 必填实例键 | reactive sync lib；≈ EUI `segmented` |
| Progress | `progress` / `Progress` | **experimental** | 可选 / 展示偏多 | 不在 §5.2 |
| Image | `Image` | **experimental** | 可选（测试选择器） | 不在 §5.2 |

### 1.2 表单（Gallery `forms`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Input | `input` / `Input` / `InputBuilder` | **verified** | 多实例 **必填实例键**；空 → **仅内部自动** `__rui_input_rt_N` | smoke#2/#3/#4/#9；input_* lib |
| Textarea | `textarea` / `Textarea` | **stable-candidate** | 同 Input | textarea_* / gallery_diag lib；待 CV 主交互签署 |
| Checkbox | `checkbox` / `Checkbox` | **stable-candidate** | 非受控 **必填实例键**；受控可选 | §5.2；待 layout/走查关门 |
| Switch | `switch` / `Switch` | **stable-candidate** | 同上 | §5.2 |
| RadioGroup | `radio_group` / `RadioGroup` | **stable-candidate** | 必填 `group_id` | reactive sync lib |
| Slider | `slider` / `Slider` | **stable-candidate** | 必填实例键 | reactive sync lib |
| Form | `form_provider` / `form_field` / `form_validate` / … | **stable-candidate** | provider **必填实例键**（错误 map 命名空间） | Context 校验 MVP |
| Select | `select` / `Select` / `SelectItem` / `SelectGroup` | **verified**（开合/Esc）；锚点+同宽默认 | 必填实例键；**锚点用 focus_id**；默认 `match_trigger`；可 `menu_width`/`menu_height`/`min_height`/`max_height` | smoke#7；`mark_floating`+sync；项左对齐可覆盖 |
| Combobox | `combobox` / `ComboboxBuilder` | **verified**（键盘选中） | 必填实例键；浮层锚点 | smoke#6 |
| DatePicker | `date_picker` / `DatePicker` | **experimental** | 必填实例键 | §5.2 实验；本地化未承诺 |
| TimePicker | `time_picker` / `TimePicker` | **experimental** | 必填实例键 | 同上 |
| ColorPicker | `color_picker` / `ColorPicker` | **experimental** | 必填实例键 | 同上 |
| （能力）FileDialog | `utils::file_dialog` | **experimental** | n/a | **非** `components/` 导出；平台差异 |

### 1.3 浮层 / 反馈（Gallery `overlays`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Dialog | `dialog` / 复合 `Dialog*` / `use_dialog` | **verified**（Esc 栈） | 必填实例键（`{id}__open`）；子件可派生 | smoke#7 |
| AlertDialog | `alert_dialog` / `AlertDialogBuilder` | **stable-candidate** | 必填实例键 | 共享 `internal/dialog_shell` |
| Toast | `toast` / `toast_host` / `toast_push` / … | **stable-candidate** | 队列项 **仅内部自动** `u64`；host **展示型不要求** | host 勿占 flex（gallery 回归） |
| Tooltip | `tooltip` / `TooltipBuilder` | **stable-candidate** | 必填实例键；**锚点用 focus_id** | §5.2 |
| Sheet | `sheet` / `SheetBuilder` | **experimental** | 必填实例键 | ≈ shadcn Sheet / EUI 侧滑；不在 §5.2 |
| Popover | `popover_panel` / `PopoverBuilder` | **experimental** | **可选** `.id()`；**锚点用 focus_id** | 低层面板；可不进 prelude |
| Dropdown | `dropdown` / `DropdownBuilder` | **experimental** | 必填实例键；可选 `anchor_focus_id` | 暂仅 Builder |
| ContextMenu | `context_menu` / `ContextMenuBuilder` | **experimental** | 必填实例键 | |
| Command | `command` / `CommandBuilder` | **experimental** | 必填实例键；动作项自有 id | 搜索算法实验面 |
| （私有）dialog_shell | `internal::dialog_shell` | — | 内部 | **禁止**产品面 / prelude |

### 1.4 数据（Gallery `data`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Table | `table` / `TableBuilder` / `TableColumn` | **experimental** | 必填实例键；列 `id` 排序键 | 基础 columns+data；分组/筛选取消稳定承诺 |
| VirtualList | `virtual_list` / `VirtualListBuilder` | **experimental** | 必填实例键 | 测宽依赖；Table/Tree 底座 |
| TreeView | `tree_view` / `TreeViewBuilder` / `TreeNode` | **experimental** | 必填实例键；节点 id | 大数据实验 |
| Chart | `bar_chart` / `line_chart` | **experimental** | 展示型偏多（现 API 仍要 id） | path stub；无 pie |
| Carousel | `carousel` / `CarouselBuilder` | **experimental** | 必填实例键 | |
| Stepper | `stepper` / `StepperBuilder` | **experimental** | 必填实例键 | 产品打磨实验（Gallery 导航区） |

### 1.5 导航（Gallery `navigation`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Tabs | `tabs_headless` / `Tabs` / `tabs_list` / … | **stable-candidate** | 必填实例键；焦点前缀 `tabstrip:` | §5.2；CV 要求 layout 宽高>0 |
| Menubar | `menubar` / `MenubarBuilder` / `MenuSection` | **experimental** | 必填实例键；section/item 派生 focus | 应用命令菜单 |
| Navbar | `navbar` / `NavbarBuilder` | **experimental** | 可选 / 布局壳偏 **展示型不要求**（现 API 仍要 id） | ≠ NavigationMenu ≠ 页面栈 |
| NavigationMenu | `navigation_menu` / … | **experimental** | 必填实例键 | ShadCN 链接导览 |
| Sidebar | `sidebar` / `SidebarBuilder` | **experimental** | 可选（测试选择器常见） | Gallery 壳；布局 |
| （非组件）Navigator / Router | `app::navigator` / `app::router` | — | n/a | 勿当缺失 NavigationMenu |

### 1.6 布局（Gallery `layout`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| ScrollArea | `scroll_area` / `ScrollArea` | **verified**（含 Input 滚动） | 必填实例键（滚动信号 / `scroll:` focus） | smoke#4 |
| Scrollbar | `scrollbar` / `Scrollbar` | **stable-candidate** | id 参数存在；常跟 target | §5.2 与 ScrollArea 捆绑 |
| ScrollViewport | `scroll_viewport` / `ScrollViewport` | **experimental** | 必填；`SCROLL_VIEWPORT_FOCUS_PREFIX` | 产品封装；深度实验 |
| Resizable | `resizable` / `ResizableBuilder` | **experimental** | 必填实例键 | 不在 §5.2 |

### 1.7 文本（Gallery `text`）

| 公开 API | 导出符号（摘要） | 桶 | id 策略 | 证据 / 笔记 |
|----------|------------------|----|---------|-------------|
| Label 选区 | `enable_label_selection` / selection keys | **stable-candidate** | 选区信号键由 id 派生 | 与 Label 同行 |
| markdown inline | `text::parse_markdown_inline` 等 | **experimental** | n/a | 块级 md **缺失**（密度项） |

### 1.8 主题 / 样式（非 Gallery 分区，属 §5.2）

| 公开 API | 位置 | 桶 | id 策略 | 笔记 |
|----------|------|----|---------|------|
| Theme / `set_theme_mode` | `style/theme.rs` | **stable-candidate** | n/a | 稳定面草案含 |
| utility `class` 主集 | `rui_style_tokens` + `class`/`tw` | **stable-candidate** | n/a | 未知类名编译期提示 |

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
| **Alert** | 浮层 / 反馈 | P1 | 展示型不要求 | **非模态**横幅（≠ AlertDialog、≠ Toast） |
| **Accordion** | 基础 / 布局 | P1 | 必填实例键（开合） | 单/多开；可复用 Collapsible 原语 |
| **Collapsible** | 基础 / 布局 | P1 | 必填实例键或受控 Signal | Accordion 积木；headless 开合宜下沉 utils |
| **Breadcrumb** | 导航 | P1 | 展示型不要求 | 路径导航；与 Navbar / NavigationMenu 分工 |
| **Pagination** | 数据 | P1 | 可选；受控页码 Signal | 常跟 Table；先控页 API，虚拟滚动另议 |
| **HoverCard** | 浮层 / 反馈 | P2 | 可选；**锚点用 focus_id** | 悬停卡片；复用 popover placement + 延时 |
| **Calendar** | 表单 | P2 | 必填实例键 | 月网格可从 DatePicker **抽出**共享；对外可独立 |
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
| Field | **Form** / `form_field` | 校验 Context 已覆盖字段层 |
| Item | 列表行组合 | 用 HStack + Label；不必先成组件 |
| AspectRatio | 布局约束 | 宜 `primitives` / utility，非产品组件优先 |
| Direction | RTL/LTR | 主题级；未开国际化前不单列 |
| Alert（若只要确认） | 勿与 **AlertDialog** 混名 | planned Alert = 页内横幅 |

#### 明确暂不跟

Web 专有或与底座冲突的 registry 项（若未来出现）一律进本小节「暂不跟」，不进 §5.1 已导出表。

---

## 2. 稳定面草案（≤ 规范 §5.2）

| 组件 | 桶（本表） | CV 下一步 |
|------|------------|-----------|
| Button | verified | 保持；补 golden 骨架（C-WP-V.4） |
| Input | verified | 保持；IME 非 Win 仍开放 |
| Select | verified（Esc/开合） | V.1 补完整交互勾选 |
| Combobox | verified（键盘） | 同上 |
| Dialog | verified（Esc 栈） | V.1 开合/焦点陷阱勾选 |
| ScrollArea | verified（滚动+IME） | Scrollbar 连带勾选 |
| Label / Separator / Card | stable-candidate | V.1 可见性走查即可 |
| Textarea / Checkbox / Switch / RadioGroup / Slider | stable-candidate | V.2 layout 矩阵 |
| Tabs / AlertDialog / Toast / Tooltip | stable-candidate | V.1 + 浮层 Esc |
| Form / （Select·Combobox 已 verified） | Form → candidate | 校验路径走查 |
| Theme / utility class | stable-candidate | 换肤冒烟条目 |

**broken**：暂无已确认 P0 组件。若 V.1 发现阻断项 → 标 `broken` 或降出稳定面，**不得**假装 verified、**不得**进 C1。

**显式不在稳定候选**（即使 Gallery 有演示）：Toggle / ToggleGroup / Progress / Image / Sheet / Popover / Dropdown / ContextMenu / Command / 一切 Date·Time·Color / Table / VirtualList / TreeView / Chart / Carousel / Stepper / Menubar / Navbar / NavigationMenu / Sidebar / ScrollViewport / Resizable / 块级 markdown / FileDialog。  
**全部 `planned`（§1.9）亦不在稳定候选**——落地后先 `experimental`，再经 CV 决定是否升 §5.2。

---

## 3. EUI 密度标尺（只保留完整度对照）

对照 [`Reference/EUI-NEO-main/components`](../Reference/EUI-NEO-main) 头文件清单与 gap §6。  
**禁止**：搬 EUI 命令式 DSL、脏矩形架构、GL 后端叙事。

| EUI | RUI | 密度结论 |
|-----|-----|----------|
| button / input / checkbox / switch / radio / slider | 有（稳定面候选或 verified） | 齐 |
| tabs / dialog / dropdown / toast / tooltip / contextmenu | 有（分层不同） | 齐（深度待 CV） |
| scroll / scrollview | ScrollArea / ScrollViewport / Scrollbar | 齐 |
| navbar / sidebar / progress / stepper / carousel | 有，**experimental** | 齐密度、未承诺 semver |
| datatable / virtuallist | Table / VirtualList，experimental | 齐密度 |
| barchart / linechart | 有 MVP | 齐；**piechart 缺** |
| datepicker / timepicker / colorpicker | 有，experimental | 齐密度 |
| markdown | inline MVP；**块级缺** | 落后 |
| segmented | 无独立模块；用 **ToggleGroup** | 可接受等价 |
| card / image / panel | Card / Image；panel≈布局原语 | 齐 |
| **virtualmasonry** | **缺失** | 密度空洞；总计划 P2，不进本波 |
| heart_switch / neumorphic / tilt_card / layout_debug | 不跟 | 非目标 |
| mousearea | ≈ `interactive` 原语 | 非组件清单项 |

**计数标尺（约）**：EUI 公开控件头 ≈ 40；RUI `components/` 产品模块 ≈ 43（含复合 Dialog 子 API）。密度已接近；差距在 **信任度（分桶/冒烟/golden）** 与个别空洞（pie / 块级 md / virtualmasonry），不在「再抄一套 EUI API」。

---

## 4. ShadCN / Solid 对照摘要

### 4.1 ShadCN registry 覆盖矩阵

源：`Reference/ShadCN-UI/apps/v4/registry/new-york-v4/ui`（56 个 `.tsx`，不含 `_registry`）。

| 状态 | 数量（约） | 项 |
|------|------------|-----|
| **已导出对位** | 35+ | Button、Label、Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Select、Combobox、Card、Separator、Tabs、Dialog、AlertDialog、Sheet、Popover、Tooltip、Dropdown、ContextMenu、Menubar、NavigationMenu、Sidebar、ScrollArea、Resizable、Table、Toast、Toggle、ToggleGroup、Progress、Carousel、Chart、Command、Form、… |
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

`alert_dialog`, `button`, `card`, `carousel`, `chart`(`bar_chart`/`line_chart`), `checkbox`, `color_picker`, `command`, `combobox`, `context_menu`, `table`, `date_picker`(+`time_picker`), `dialog`(+复合标签/`use_dialog`), `dropdown`, `form`(+helpers), `image`, `input`, `label`(+selection helpers), `menubar`, `navbar`, `navigation_menu`, `popover`(`popover_panel`), `progress`, `radio_group`, `resizable`, `scroll_area`, `scroll_viewport`, `scrollbar`, `select`, `separator`, `sheet`, `sidebar`, `slider`, `stepper`, `switch`, `tabs`, `textarea`, `toast`, `toggle`, `toggle_group`, `tooltip`, `tree_view`, `virtual_list`。

未入产品清单：`base::BaseComponentBuilder`（样式底座）、`internal::*`。  
**§1.9 `planned` 不得出现在本列表**，直到首次 `pub use` 并改桶为 `experimental`。

---

## 6. 维护规则

1. 新增 `pub use` → 从 §1.9 删对应 `planned` 行，写入分区表 + 规范 §5.1；默认桶 = `experimental`。  
2. V.1 走查改桶：只允许 `stable-candidate` → `verified` / `broken` / 降为 `experimental`。  
3. `broken` 修复后先回 `stable-candidate`，再经证据升 `verified`。  
4. 升格进 §5.2 须改规范 + CHANGELOG 草稿，不得只改本表。  
5. 从 ShadCN registry 新增对位意向 → 先加 §1.9 `planned`，禁止直接写进已导出分区表。
