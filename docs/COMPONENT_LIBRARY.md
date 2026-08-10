# RUI 基础组件库（权威规范）

> **文档地位**：组件产品面的**单一权威规范**。选型、分层、API 契约、清单与质量门以本文为准。  
> **从属关系**：服从 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) 六条门槛与禁区；执行排期见 [`COMPONENT_LIBRARY_PLAN.md`](COMPONENT_LIBRARY_PLAN.md)；与巩固期总计划 [`ENGINEERING_IMPLEMENTATION_PLAN.md`](ENGINEERING_IMPLEMENTATION_PLAN.md) 并行，不另开架构分叉。  
> **事实来源**：`crates/rui/src/{primitives,tokens,components,style,utils}`（私有壳在 `components/internal/`）、`examples/gallery`、[`headless.md`](headless.md)、[`color.md`](color.md)。  
> **日期**：2026-08-10（§5 分桶表落地）  

交叉文档：[`component_inventory.md`](component_inventory.md) · [`element_identity.md`](element_identity.md) · [`custom_components.md`](custom_components.md) · [`keyboard_nav.md`](keyboard_nav.md) · [`desktop_smoke.md`](desktop_smoke.md) · [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md) · [`color.md`](color.md) · [`a11y_smoke.md`](a11y_smoke.md)

---

## 0. 一句话结论

**对外交付 ShadCN 式带默认样式的组件；对内以 Radix 式无头行为为骨架；书写上 `ui!` 标签与 Builder 纯 Rust 互为兼容的一等公民。**  
禁止二选一：纯无头拖死采用与质感验证；样式与行为粘死拖死 a11y、换肤与可测性；只保留标签或只保留 Builder 都会破坏 DX。

| 面 | 形态 | 路径 | 用户默认 |
|----|------|------|----------|
| 产品面 | shadcn：`variant` / `size` / token 色 / 焦点环 | `components/` | **是**（prelude / Gallery / 文档索引） |
| 书写面 | A: `ui!` 标签 · B: `*Builder` 链式 | 宏展开 ≡ Builder → `Element` | **双轨兼容**（见 §3） |
| 行为面 | radix：开合、roving、trap、受控 Signal | `primitives/` + `utils/{popup,roving,focus_trap,keyboard}` | 高级 / 自定义组件 |
| 语义面 | 设计令牌 + Theme light/dark | `style/theme.rs` + `tokens/` + `rui_style_tokens` | 换肤与 chrome 同源 |

ADR 对齐：纲领 **ADR-6**（ShadCN 无头分层）、**ADR-9**（双书写面兼容）— **接受，不可再分叉**。

---

## 1. 工程级定义（组件库专属）

组件库达到「工程可承诺」当且仅当下列同时成立（映射纲领六门槛）：

1. **可预测**：有状态控件的 Signal 键 / `focus_id` 规则文档化；受控与非受控无双源真理。  
2. **原生可信**：主路径控件支持 Tab / roving / Escape / 焦点环；浮层走统一 Esc 栈与 placement sync。  
3. **测宽≡绘制**：含文本的控件（Input / Textarea / Label / VirtualList 行）不依赖「猜行高」。  
4. **a11y**：Button / Input / 至少一种列表或菜单 Role 正确；焦点与值变化可进 AccessKit。  
5. **质量门**：每公开组件有 Gallery 页；核心 ≥5 场景 golden；浮层 / 表单有库测或冒烟条目。  
6. **发布契约**：稳定面 vs 实验面表格；破坏性变更进 CHANGELOG；文档站有组件索引。

未满足时，组件可演示（MVP），但不得标为稳定 API。

---

## 2. 分层架构（实现 = 规范）

```text
  应用 / Gallery / 自定义 #[component]
            │  默认 import
            ▼
  components/          ← 外观 + 变体 + 产品 API（ShadCN）
            │  组合
     ┌──────┼──────────────────┐
     ▼      ▼                  ▼
 primitives/  tokens/     utils/popup·roving·focus_trap·keyboard·text_field
 (布局叶子)   (语义色)     (浮层几何 / 键盘 / 文本编辑共享)
     │
     ▼
 style/theme + color_space(OKLCH) + utility 变体 + focus_ring

 私有（非产品面）：components/internal/{text_edit,dialog_shell}
```

### 2.1 职责与禁区

| 层级 | 路径 | 职责 | **禁止** |
|------|------|------|----------|
| 布局 / 交互原语 | `primitives/` | `Box`/`HStack`/`VStack`/`GStack`、`Text`/`Icon`、`interactive`、`overlay`、`path`、`Portal` | 写死 primary 色、圆角、阴影等视觉常量 |
| 行为工具 | `utils/popup.rs`、`roving.rs`、`focus_trap.rs`、`keyboard.rs`、`text_field.rs` | 浮层标记与 placement、roving、Tab 陷阱、键路由、受控文本写入 | 绑定某一组件的外观变体 |
| 设计令牌 | `style/theme.rs`、`color_space.rs`、`tokens/`、`rui_style_tokens` | OKLCH 语义色→sRGB、radius、utility 真源、dark；CPU 混色工具 | 业务状态机；完整 ICC/HDR（见 [`color.md`](color.md)） |
| 样式组件 | `components/` | **仅产品组件**（Button、Input、Dialog…） | 把复杂开合/焦点状态机只藏在样式层且不可测；把私有 chrome 导出为产品 API |
| 私有壳 | `components/internal/`（`pub(crate)`） | Input/Textarea chrome（`text_edit`）；Dialog/AlertDialog 模态壳（`dialog_shell`）；随组件库分 crate | 当作公开 API 文档化或 prelude 导出；挂到框架 crate 根冒充 runtime |

现状缺口（须按计划下沉，见实施计划 C-WP）：

- **浮层**：几何已统一在 `utils/popup`；开合状态仍大量内嵌各组件——允许过渡，禁止第二套 placement。  
- **Toggle**：仍在 `components/toggle`（及 Checkbox/Switch）；**尚未**抽出独立 `primitives/toggle` 或 `utils/toggle_state`——**允许过渡，不允许永久扩散**。  
- **Tabs / Toggle**：`primitives/mod.rs` 注释已写明暂在 `components/`。

### 2.2 与 Web 对标的正确映射

| Web | RUI | 说明 |
|-----|-----|------|
| Radix Primitives | `interactive` + popup/roving 契约 + 未来抽出的 headless builders | 无 DOM；行为用 Signal + focus_id |
| shadcn/ui | `components/*` + Theme tokens + `variant`/`size` | **源码在库内**，非「复制进用户仓库」；定制靠 token / class / 组合原语 |
| Tailwind / Uno | `class`/`tw` + `rui_style_tokens` + attributify 属性 | 不全量 CSSOM |

**不采用**：npm-shadcn「复制粘贴组件文件」为默认 DX。Rust crate 的正确模型是**库内分层 + 稳定公开 API + token 覆盖**；用户要完全异形皮肤时，用 `#[component]` 组合原语（见 [`custom_components.md`](custom_components.md)）。

---

## 3. 双书写面（一等公民 · 必须兼容）

RUI **同时**提供两种写 UI 的方式，且必须**互相兼容**。禁止把「只用 `ui!` 标签」或「废弃 Builder」写成唯一正确路径（**ADR-9**）。

| 书写面 | 形态 | 典型入口 | 适用 |
|--------|------|----------|------|
| **A · RSX / 标签** | `ui! { <Button variant="outline">…</Button> }` | `rui_macros::ui!`、`#[component]`；经 [`prelude`](../crates/rui/src/prelude.rs) | 声明式树、热重载模板、文档站默认示例 |
| **B · Builder / 纯 Rust** | `button("ok").variant("outline").on_click(…).into()` 或 `Element::from(…)` | 同模块 `fn` + `*Builder`；经 **同一** `prelude` | 无类 HTML 标签、动态拼装、库测、复杂闭包 |

### 3.0 默认入口：`rui::prelude`

```rust
use rui::prelude::*; // 标签入口 + Builder/`fn` + 布局原语 + Element/Signal + ui!/component
```

| 进 prelude | 不进 prelude |
|------------|--------------|
| 样式组件 PascalCase **与** 对应 `snake_case` / `*Builder` | [`utils::popup`](../crates/rui/src/utils/popup.rs) 低层几何 / placement |
| `hstack` / `vstack` / `Portal` / `text` / `icon` | `popover_panel`（低层面板；按需 `components::popover`） |
| `ThemeMode` / `set_theme_mode` | `overlay` / `path`、`BaseComponentBuilder`、`apply_bound_*` |
| `ui!` / `component` / `reactive!` 等宏 | `planned` 未落地组件；`internal/*` |

策略结论：**默认 DX = prelude 双书写面**；高级浮层能力显式路径导入，避免把 Radix 式无头工具误当成产品默认 API。

### 3.1 兼容契约（硬性）

1. **同一语义、同一 `Element`**：标签展开为 Builder 调用；Builder `Into`/`From` → `Element`。两种写法不得产生两套行为或两套 chrome。  
2. **可互嵌**：
   - `ui!` 子树可嵌入 `{ Element::from(button(…).variant(…)) }` 或任意已构建的 `Element`；
   - Builder / `vstack`·`hstack` 的 `.child(…)` 可接收 `ui! { … }` 的结果或 `#[component]` 产物。  
3. **API 面等价**：稳定面组件凡在 RSX 暴露的 props（`variant` / `size` / `disabled` / 受控 signal / `on_click` 等），Builder 须有对应链式方法（或文档标明「仅 RSX 糖」的极少数例外，且例外不得承载唯一行为路径）。  
4. **仅 Builder 的控件**（如部分 `toast` / `dropdown`）：允许暂无标签，但须在文档与 Gallery 标明；新增稳定面控件**默认双入口**（标签 + Builder），并进入 prelude。  
5. **热重载**：`template_id` 主要服务含 `ui!` 的模板；纯 Builder 树仍是一等运行时能力，不因无热补丁而降级为「非推荐」。  
6. **文档与示例**：Getting Started / 组件索引应**至少各给一种写法的完整片段**；标杆组件（Button、Input）两种都展示。

### 3.2 反例（禁止）

| 禁止 | 原因 |
|------|------|
| 新能力只进 `ui!` 属性、Builder 长期缺失 | 破坏无标签书写能力 |
| 文档暗示「现代写法 = 只能写标签」 | 与项目「掌握代码 / 纯 Rust 拼装」目标冲突 |
| RSX 与 Builder 走不同状态机或不同 token | 双源真理，无法互嵌保证观感一致 |
| prelude 只导出标签、废除 Builder re-export | 违反 ADR-9 |

代码锚点（同一按钮 · 双面）：

```rust
use rui::prelude::*;

// A · 标签（无持久状态时可省略 id）
ui! { <Button variant="outline" on_click={move || { /* … */ }}>"保存"</Button> }

// 多实例 / 测试 / 稳定焦点时再显式 id（见 §4.2）
ui! { <Button id="save" variant="outline" on_click={move || { /* … */ }}>"保存"</Button> }

// B · Builder（无类 HTML 标签）；空 id 可走自动生成，持久状态勿依赖自动 id
let _: Element = button("save")
    .variant("outline")
    .on_click(move |_| { /* … */ })
    .into();
```

互嵌（验收用例，见 `crates/rui/tests/prelude_dual_surface.rs`）：

```rust
use rui::prelude::*;

// 标签树 ← 嵌入 Builder
let _ = ui! {
    <VStack gap="0.5rem">
        { Element::from(button("x").variant("ghost").text("内嵌 Builder")) }
        <Button variant="outline">"标签子节点"</Button>
    </VStack>
};

// Builder 树 ← 嵌入 ui!
let _: Element = vstack()
    .gap("0.5rem")
    .child(ui! { <Label id="cap">"内嵌标签"</Label> })
    .into();
```

实现细节以各组件 `//!` 与 `From<*Builder> for Element` 为准；宏展开不得绕过公开 Builder 另造私有平行 API（宏生成调用公开函数除外）。

---

## 4. API 契约（所有公开组件）

### 4.1 命名与暴露

- **RSX**：PascalCase 标签（`<Button>`）；实现侧 `#[component]` 与/或 Builder `From` → `Element`。  
- **Builder**：`snake_case` 入口函数（如 `button`、`input`）+ `*Builder` 链式方法；与标签**同一模块导出**。  
- 函数名可与标签同名（模块已 `allow(non_snake_case)`）时，须在文档区分「标签用 PascalCase / 调用用模块路径」。  
- 少数暂仅 Builder / 函数：`toast`、`dropdown` 等——文档与 Gallery 须标明；补标签属增强，不阻塞 Builder 一等地位。Dialog 为复合标签（`Dialog` / `DialogTrigger` / `DialogContent` / …）+ `dialog` Builder。

### 4.2 身份与状态（`id` 勿过度使用）

> **跨层权威契约**：不变量、AccessKit 派生、组件/宏/框架职责与事故原型见 **[`element_identity.md`](element_identity.md)**（本文仅组件 API 摘要）。  
> **现状判断**：多数 Builder 把 `id: impl Into<String>` 当作**首位必参**，Gallery 也充满 `gal-*` 字符串。这把「实例键 / 焦点 / 信号命名空间 / 测试选择器」揉进同一个 prop，造成**过度使用 `id`**；空串写入 `template_id`/`focus_id` 曾导致读屏树重复 `NodeId` 崩溃。实现分期收敛见 [COMPONENT_LIBRARY_PLAN.md](COMPONENT_LIBRARY_PLAN.md) **C-WP-V.5 / C-WP-2.6** 与总计划 **WP-2.4**。

#### 4.2.1 四条身份轴（禁止混为一谈）

| 轴 | 含义 | 典型载体 | 用户是否常写 |
|----|------|----------|--------------|
| **实例键 / 信号命名空间** | 非受控 `use_signal`、hover spring、`{id}__open` 等全局表键 | 今日多由组件 `id` 派生 | 仅多实例或跨子树持久时需要稳定键 |
| **`focus_id`** | Tab / 点击焦点、AccessKit、浮层 `anchor` | `Element.focus_id`；常带前缀（`input:`、`tabstrip:`） | 需要可编程焦点或 a11y 名时 |
| **`template_id`** | 热重载 / `reactive!` **槽位** reconcile（≠ 全局 a11y 主键） | 宏或 `reactive!(id, …)` | 热补丁或显式 anchor 时 |
| **测试 / 调试选择器** | 自动化点选、日志 | 可与上重合，**非**每个控件必填 | 测试需要时 |

**禁止**：为「看起来像 HTML」而给每个静态 Label / 布局壳强行编造 `id`。  
**禁止**：文档暗示「不用 id 就不合法」——`Separator` / `Card` 等无状态控件已证明可以没有用户 `id`。  
**禁止**：`id == ""` 原样写入 `Some("")` 的 `focus_id` / `template_id`（见身份契约 I-1）。

#### 4.2.2 何时**需要**稳定用户可见 `id`（或等价实例键）

| 场景 | 要求 |
|------|------|
| 非受控状态且**多实例**并存（多 Input、多 Dialog） | 稳定唯一键；否则全局 `use_signal("{id}__…")` **撞键串状态**（Input 空 id 注释已明示） |
| 受控 Signal 由外部持有 | **不强制**组件 `id`；状态在调用方 Signal |
| 焦点环 / 读屏 / 浮层锚点需稳定引用 | 显式 `focus_id`（或文档约定由 `id` 派生且稳定） |
| 测试选择器、IME/冒烟脚本 | 显式稳定 id / focus_id |
| 仅单次点击、无内部持久信号的 Button | **可不传**；允许会话内自动 id（见下） |

#### 4.2.3 自动生成 id 的边界

今日实现（事实）：

- `Button`：空 id → `btn-N`（进程内递增）  
- `Input`：空 id → `__rui_input_rt_N`（兜底，避免多框共用 value 信号）  
- RSX 宏：部分省略 id 的标签会生成唯一名（以宏为准）

| 可以 | 不可以指望 |
|------|------------|
| 单页内区分 hover/paint 键、避免立刻撞键 | **跨 rebuild / 热重载后仍指向同一逻辑实例** |
| 无状态展示、一次性按钮 | 表单字段持久、多实例非受控状态的长期身份 |

自动 id **不是**「用户永远不用想身份」的完整方案；长期方向是 **Owner 作用域信号**（`use_scoped_signal`），减少对全局字符串 `id` 的依赖（见 [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md)、[`custom_components.md`](custom_components.md)）。

#### 4.2.4 API 与文档约定（目标态）

| 控件类别 | `id` / 实例键 | 说明 |
|----------|---------------|------|
| **展示型**（Separator、Card、纯布局 Navbar 壳等） | **默认不要求** | 若 Builder 仍留 `id` 参数，标为可选或仅用于测试 |
| **有状态 · 非受控**（Input、Checkbox、Select、Dialog open…） | 多实例要稳定键；单实例可框架派生 | 优先受控 Signal，少强迫用户起名 |
| **有状态 · 受控** | 可选 | 行为不依赖组件 id |
| **浮层** | 组件实例键可框架派生；**锚点用 `focus_id` / `anchor_focus_id`** | 勿让用户为 trigger 与 panel 各编两个无关魔法字符串（除非要稳定引用） |

Gallery / 示例：**只在演示焦点、多实例、测试选择器处写 id**；其余按钮勿堆 `gal-btn-1…N`。

#### 4.2.5 状态规则（与身份正交）

| 规则 | 要求 |
|------|------|
| 非受控 | 内部信号键必须落入**唯一命名空间**（显式 id、宏生成 id、或 Owner 作用域） |
| 受控 | 调用方持有 `Signal<T>`；组件**不得**再为同一语义状态建第二份 signal |
| 禁用 | `disabled` 统一关闭指针与键盘激活，并参与 `disabled:` utility 变体 |
| `focus_id` 前缀 | `input:`、`tabstrip:`、`scroll:` 等；AccessKit 依赖前缀；新增前缀须改 bridge + 本文或 `keyboard_nav.md` |

### 4.3 外观（ShadCN）

| 约定 | 要求 |
|------|------|
| `variant` | 字符串或枚举；命名对齐 shadcn（`default` / `secondary` / `outline` / `ghost` / `destructive` / `link`…） |
| `size` | `xs` / `sm` / `default` / `lg` / `icon*` 等；与 Button 已实现对齐者优先复用 |
| 颜色 | 经 `parse_color("primary")` 等语义名；令牌源为 OKLCH→display sRGB（[`color.md`](color.md)）；禁止组件内硬编码与 Theme 无关的色值 |
| `outline` 变体 | 语义是 **border**（盒模型边框），**不是** CSS `outline` 属性；焦点环用 shadow/`ring`，见 [`color.md`](color.md) |
| 焦点环 | 默认走 `style/focus_ring` + theme `ring`；`focus-visible` 行为与冒烟表一致 |
| 动效 | hover/focus 可用 spring/cubic（Button 已用 spring + 线性 sRGB lerp）；paint sync 优先，避免整树 rebuild |

### 4.4 浮层

- 统一：`utils/popup` 的 `mark_floating` / placement / underlay；`app::portal_overlay` Esc 栈。  
- Escape **只关顶层**；点击外部关闭与受控 `open` 信号单一写入点。  
- 打开时焦点陷阱（`utils::sync_focus_trap` / `activate_focus_trap`）与关闭后焦点归还须可测或可冒烟。

### 4.5 无障碍最低集

| 控件类 | Role / 行为（最低） |
|--------|---------------------|
| Button / Toggle | Button；名称取自后代可见文案或 `aria-label`；pressed 时 toggled |
| Input / Textarea | TextField；值与焦点同步；可用 `aria-labelledby` 关联 Label |
| Label（静态） | **不进 Tab**；进 a11y 树为 Label（文本在 `value`）；关联控件用 `accesskit_id` + `aria-labelledby` |
| Tabs 条带 | TabList（`tabstrip:`） |
| Checkbox / Switch | 可 toggled |
| 菜单 / Command / Combobox | 打开关闭 + 键盘移动（深化见巩固期 WP） |

细节实现以 `a11y/bridge.rs` 为准；读屏步骤见 [`a11y_smoke.md`](a11y_smoke.md)；新增前缀必须同时改 bridge 与本文或 `keyboard_nav.md`。

---

## 5. 组件清单（代码事实 · 2026-08-10）

来源：`components/mod.rs` `pub use` + Gallery 分区。  
**分桶权威表**（`experimental` / `stable-candidate` / `verified` / `planned` / `broken` + **id 策略**）：[`component_inventory.md`](component_inventory.md)（C-WP-0.2 ☑）。  
粗口径：**MVP** = Gallery 可演示；**工程** = 满足 §1；未过 CV 的 §5.2 项多为 `stable-candidate`；ShadCN 未落地对位为 `planned`（§5.1 附表 / inventory §1.9）。

参考分工：ShadCN → 产品命名与分区；Solid → 受控 Signal / 细粒度 sync（非组件表）；EUI → **仅密度标尺**（见 inventory §3），不搬架构。

### 5.1 按 Gallery 分区

| 分区 | 组件（`components/` 模块） | 分桶摘要（详见 inventory） |
|------|----------------------------|----------------------------|
| **基础** | Button、Label、Separator、Card、Toggle、ToggleGroup、Progress、Image | Button=`verified`；Label/Separator/Card=`stable-candidate`；其余=`experimental`（越出 §5.2） |
| **表单** | Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Form、Select、Combobox、DatePicker、TimePicker、ColorPicker；文件选择为 `utils/file_dialog`（能力，非组件） | Input/Select/Combobox=`verified`（窄主路径）；其余稳定面项=`stable-candidate`；Date/Time/Color/File=`experimental` |
| **浮层 / 反馈** | Dialog、AlertDialog、Sheet、Popover、Tooltip、Dropdown、ContextMenu、Toast、Command；（共享壳 `internal/dialog_shell`） | Dialog=`verified`（Esc 栈）；AlertDialog/Toast/Tooltip=`stable-candidate`；Sheet/Popover/Dropdown/ContextMenu/Command=`experimental` |
| **数据** | VirtualList、Table、TreeView、Chart（`bar_chart`/`line_chart`）、Carousel、Stepper | 全部 `experimental`（§5.2 外或深度未承诺） |
| **导航** | Tabs、Menubar、Navbar、NavigationMenu、Sidebar | Tabs=`stable-candidate`；其余=`experimental` |
| **布局** | ScrollArea、ScrollViewport、Scrollbar、Resizable | ScrollArea=`verified`（含 Input 滚动）；Scrollbar=`stable-candidate`；Viewport/Resizable=`experimental` |
| **文本** | Label 选区、markdown inline / 块级 md（`text/`） | 选区随 Label；inline=`experimental`；块级 md **缺失**（密度项） |

**ShadCN `planned`（尚未 `pub use`，详见 inventory §1.9）**：

| 优先级 | 规划组件 | 建议分区 |
|--------|----------|----------|
| P1 | Badge、Avatar、Skeleton、Spinner、Alert、Accordion、Collapsible、Breadcrumb、Pagination | 基础 / 浮层 / 导航 / 数据 |
| P2 | HoverCard、Calendar、Empty、Kbd、ButtonGroup、InputOTP、InputGroup | 浮层 / 表单 / 基础 |
| 等价不单列 | Drawer→Sheet、Sonner→Toast、NativeSelect→Select、Field→Form 等 | — |

**不在 `components/`、勿当缺失组件**：`Portal`/`overlay`/`path`→`primitives/`；`popup`/`roving`/`focus_trap`/`text_field`→`utils/`。`planned` 未落地前**禁止**写入 prelude 或假装已导出。

### 5.2 稳定面 vs 实验面（发布口径草案）

| 稳定面（0.1 起承诺 semver） | 实验面（可破坏，须标 `experimental` 或文档警告） |
|-----------------------------|--------------------------------------------------|
| Button、Label、Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Separator、Card | Chart / Path 绘制细节、Carousel、Command 搜索算法 |
| ScrollArea / Scrollbar、Tabs、Dialog、AlertDialog、Toast、Tooltip | Date/Time/Color 本地化与时区、FilePicker 平台差异 |
| Form（校验 Context）、Select、Combobox（基础开合） | TreeView 大数据、Table 分组/筛选深化 |
| Theme / `set_theme_mode`、utility `class` 主集 | VirtualMasonry、托盘、多窗协作细节、Stepper 产品打磨；**ShadCN planned**（Badge/Avatar/Skeleton/Spinner/Alert/Accordion/…，见 inventory §1.9） |

**CV 分桶快照（2026-08-10）**：`verified` = Button、Input、Select、Combobox、Dialog（Esc）、ScrollArea；其余 §5.2 行为控件 = `stable-candidate`；**broken = 无**；**planned** = ShadCN 未落地对位（不进稳定面）。升格/降级只改 [`component_inventory.md`](component_inventory.md) 并回写本小节。最终发布表以巩固期 **WP-4.1** CHANGELOG 为准。

### 5.3 明确不做（组件库范围）

- 换 VDOM / 弃 wgpu / 引入浏览器控件。  
- 为「纯无头公共 API」另起一套与 `components/` 平行的完整产品面（可渐进抽出 headless builder，但默认文档与 prelude 仍指向样式组件）。  
- **废除或边缘化 Builder / 纯 Rust 书写面**（或反之只保留 Builder、废弃 `ui!`）——双书写面均为一等公民，见 §3。  
- **把用户 `id` 当作每个控件的必填 HTML 式属性**——身份轴拆分见 §4.2；过度必填是债务，不是设计目标。  
- 移动 / 鸿蒙一等公民组件变体。  
- 完整 CSSOM、DOM 选择器、Web 专有 a11y 树。

---

## 6. 质量门与验收模板

### 6.1 每个新组件 / 行为破坏性改动的 PR 检查表

- [ ] 分层：新状态机是否应在 `utils`/`primitives` 而非仅组件内复制  
- [ ] 受控 / 非受控单一写入点  
- [ ] **身份**：未把 `id` 当作万金油；区分实例键 / `focus_id` / `template_id`（[`element_identity.md`](element_identity.md) + §4.2）；空串不写入 focus/template；展示型不强制用户起 id  
- [ ] Theme token，无硬编码色  
- [ ] **双书写面**：稳定面新 props 同时进 RSX 与 Builder（或文档标明例外）；互嵌仍编译  
- [ ] Gallery 对应分区可见；**示例不堆砌无必要 id**  
- [ ] 库测或 `desktop_smoke` 条目（浮层 / 焦点至少其一）  
- [ ] 若属稳定面：更新组件索引与 CHANGELOG 草稿  

### 6.2 自动化基线

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test -p rui --lib
cargo test -p rui --lib offscreen
cargo check -p gallery
```

组件 golden 目标（巩固期）：Button、Input、Dialog、VirtualList、Markdown ≥5 场景进 CI。

### 6.3 Gallery 契约

- 侧栏分区与 §5.1 一致：基础 · 表单 · 浮层/反馈 · 数据 · 导航 · 布局 · 文本 · 关于。  
- **每公开稳定面组件至少一处可运行示例**。  
- 变体矩阵优先展示在「基础 / 表单」（Button variants/sizes 已是标杆）。  
- 「关于」或基础区宜保留 **Builder 写法** 对照示例，避免用户以为只能写标签。

---

## 7. 文档拓扑（组件相关）

```text
ENGINEERING_GRADE_FRAMEWORK.md     ← 总纲领（ADR-6 / ADR-9）
ENGINEERING_IMPLEMENTATION_PLAN.md ← 巩固期 Wave 1–4
COMPONENT_LIBRARY.md               ← 本文件（组件权威规范；§3 prelude + 双书写面）
COMPONENT_LIBRARY_PLAN.md          ← 组件库专项 WP / 波次（含 CV 门禁）
component_inventory.md             ← §5 分桶附录（verified/broken + id 策略 + EUI 密度）
crates/rui/src/prelude.rs          ← `use rui::prelude::*`（标签+Builder，不含低层 popup）
element_identity.md                ← 跨层身份契约（id / focus / template / a11y）
headless.md / custom_components.md / keyboard_nav.md / color.md / a11y_smoke.md
gap_analysis.md §6                 ← 历史 EUI 密度摘要（真源转 inventory §3）
```

争议处理：先 ADR，再改本规范；禁止静默改变「默认交付面 = shadcn 样式组件」或「双书写面互相兼容」。

---

*规范随清单与 SemVer 面更新而修订；新增公开组件必须落入 §5 分区并更新 Gallery。*
