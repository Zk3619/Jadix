# RUI 基础组件库（权威规范）

> **文档地位**：组件产品面的**单一权威规范**。选型、分层、API 契约、清单与质量门以本文为准。  
> **从属关系**：服从 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) 六条门槛与禁区；执行排期见 [`COMPONENT_LIBRARY_PLAN.md`](COMPONENT_LIBRARY_PLAN.md)；与巩固期总计划 [`ENGINEERING_IMPLEMENTATION_PLAN.md`](ENGINEERING_IMPLEMENTATION_PLAN.md) 并行，不另开架构分叉。  
> **事实来源**：`crates/rui/src/{primitives,tokens,components,style,utils}`、`examples/gallery`、[`headless.md`](headless.md)、[`gap_analysis.md`](gap_analysis.md)。  
> **日期**：2026-08-07  

交叉文档：[`custom_components.md`](custom_components.md) · [`keyboard_nav.md`](keyboard_nav.md) · [`desktop_smoke.md`](desktop_smoke.md) · [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md)

---

## 0. 一句话结论

**对外交付 ShadCN 式带默认样式的组件；对内以 Radix 式无头行为为骨架；书写上 `ui!` 标签与 Builder 纯 Rust 互为兼容的一等公民。**  
禁止二选一：纯无头拖死采用与质感验证；样式与行为粘死拖死 a11y、换肤与可测性；只保留标签或只保留 Builder 都会破坏 DX。

| 面 | 形态 | 路径 | 用户默认 |
|----|------|------|----------|
| 产品面 | shadcn：`variant` / `size` / token 色 / 焦点环 | `components/` | **是**（prelude / Gallery / 文档索引） |
| 书写面 | A: `ui!` 标签 · B: `*Builder` 链式 | 宏展开 ≡ Builder → `Element` | **双轨兼容**（见 §3） |
| 行为面 | radix：开合、roving、trap、受控 Signal | `primitives/` + `utils/{popup,roving,keyboard}` | 高级 / 自定义组件 |
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
     ┌──────┼──────────────┐
     ▼      ▼              ▼
 primitives/  tokens/   utils/popup·roving·keyboard
 (布局叶子)   (语义色)   (浮层几何 / 键盘原语)
     │
     ▼
 style/theme + utility 变体引擎 + focus_ring
```

### 2.1 职责与禁区

| 层级 | 路径 | 职责 | **禁止** |
|------|------|------|----------|
| 布局 / 交互原语 | `primitives/` | `Box`/`HStack`/`VStack`/`GStack`、`Text`/`Icon`、`interactive` | 写死 primary 色、圆角、阴影等视觉常量 |
| 行为工具 | `utils/popup.rs`、`roving.rs`、`keyboard.rs` | 浮层标记与 placement、roving 移动、键路由 | 绑定某一组件的外观变体 |
| 设计令牌 | `style/theme.rs`、`tokens/`、`rui_style_tokens` | 语义色、radius、utility 真源、dark | 业务状态机 |
| 样式组件 | `components/` | 组合以上，暴露 shadcn API | 把复杂开合/焦点状态机只藏在样式层且不可测 |

现状缺口（须按计划下沉，见实施计划 C-WP）：`popup` / `toggle` 行为仍大量内嵌于各组件；`primitives/` 模块注释已写明 Tabs/Toggle 暂在 `components/`——**允许过渡，不允许永久扩散**。

### 2.2 与 Web 对标的正确映射

| Web | RUI | 说明 |
|-----|-----|------|
| Radix Primitives | `interactive` + popup/roving 契约 + 未来抽出的 headless builders | 无 DOM；行为用 Signal + focus_id |
| shadcn/ui | `components/*` + Theme tokens + `variant`/`size` | **源码在库内**，非「复制进用户仓库」；定制靠 token / class / 组合原语 |
| Tailwind / Uno | `class`/`tw` + `rui_style_tokens` + attributify 属性 | 不全量 CSSOM |

**不采用**：npm-shadcn「复制粘贴组件文件」为默认 DX。Rust crate 的正确模型是**库内分层 + 稳定公开 API + token 覆盖**；用户要完全异形皮肤时，用 `#[component]` 组合原语（见 [`custom_components.md`](custom_components.md)）。

---

## 3. 双书写面（一等公民 · 必须兼容）

RUI **同时**提供两种写 UI 的方式，且必须**互相兼容**。禁止把「只用 `ui!` 标签」或「废弃 Builder」写成唯一正确路径。

| 书写面 | 形态 | 典型入口 | 适用 |
|--------|------|----------|------|
| **A · RSX / 标签** | `ui! { <Button variant="outline">…</Button> }` | `rui_macros::ui!`、`#[component]` | 声明式树、热重载模板、文档站默认示例 |
| **B · Builder / 纯 Rust** | `button("ok").variant("outline").on_click(…).into()` 或 `Element::from(…)` | `components::*` / `primitives::*` 的 `fn` + `*Builder` | 无类 HTML 标签、动态拼装、库测、复杂闭包 |

### 3.1 兼容契约（硬性）

1. **同一语义、同一 `Element`**：标签展开为 Builder 调用；Builder `Into`/`From` → `Element`。两种写法不得产生两套行为或两套 chrome。  
2. **可互嵌**：
   - `ui!` 子树可嵌入 `{ Element::from(button(…).variant(…)) }` 或任意已构建的 `Element`；
   - Builder / `vstack`·`hstack` 的 `.child(…)` 可接收 `ui! { … }` 的结果或 `#[component]` 产物。  
3. **API 面等价**：稳定面组件凡在 RSX 暴露的 props（`variant` / `size` / `disabled` / 受控 signal / `on_click` 等），Builder 须有对应链式方法（或文档标明「仅 RSX 糖」的极少数例外，且例外不得承载唯一行为路径）。  
4. **仅 Builder 的控件**（如部分 `dialog_panel` / `toast` / `dropdown`）：允许暂无标签，但须在文档与 Gallery 标明；新增稳定面控件**默认双入口**（标签 + Builder）。  
5. **热重载**：`template_id` 主要服务含 `ui!` 的模板；纯 Builder 树仍是一等运行时能力，不因无热补丁而降级为「非推荐」。  
6. **文档与示例**：Getting Started / 组件索引应**至少各给一种写法的完整片段**；标杆组件（Button、Input）两种都展示。

### 3.2 反例（禁止）

| 禁止 | 原因 |
|------|------|
| 新能力只进 `ui!` 属性、Builder 长期缺失 | 破坏无标签书写能力 |
| 文档暗示「现代写法 = 只能写标签」 | 与项目「掌握代码 / 纯 Rust 拼装」目标冲突 |
| RSX 与 Builder 走不同状态机或不同 token | 双源真理，无法互嵌保证观感一致 |

代码锚点示例（同一按钮）：

```rust
// A · 标签
ui! { <Button id="ok" variant="outline" on_click={move || { /* … */ }}>"保存"</Button> }

// B · Builder（无类 HTML 标签）
Element::from(
    button("ok")
        .variant("outline")
        .on_click(move |_| { /* … */ })
        // 子文本按各 Builder 约定：.text / .child / Into 等
)
```

实现细节以各组件 `//!` 与 `From<*Builder> for Element` 为准；宏展开不得绕过公开 Builder 另造私有平行 API（宏生成调用公开函数除外）。

---

## 4. API 契约（所有公开组件）

### 4.1 命名与暴露

- **RSX**：PascalCase 标签（`<Button>`）；实现侧 `#[component]` 与/或 Builder `From` → `Element`。  
- **Builder**：`snake_case` 入口函数（如 `button`、`input`）+ `*Builder` 链式方法；与标签**同一模块导出**。  
- 函数名可与标签同名（模块已 `allow(non_snake_case)`）时，须在文档区分「标签用 PascalCase / 调用用模块路径」。  
- 少数暂仅 Builder / 函数：`dialog_panel`、`toast`、`dropdown` 等——文档与 Gallery 须标明；补标签属增强，不阻塞 Builder 一等地位。

### 4.2 身份与状态

| 规则 | 要求 |
|------|------|
| `id` | 有状态控件尽量稳定唯一；空 id 可自动生成（如 `btn-N`），但焦点 / 测试选择器应显式传 |
| `focus_id` | 使用约定前缀（`input:`、`tabstrip:`、`scroll:` 等）；AccessKit 依赖前缀映射 Role |
| 非受控 | 组件按 `id` 内部 `use_signal` |
| 受控 | 调用方持有 `Signal<T>`；组件**不得**再为同一语义状态建第二份 signal |
| 禁用 | `disabled` 统一关闭指针与键盘激活，并参与 `disabled:` utility 变体 |

### 4.3 外观（ShadCN）

| 约定 | 要求 |
|------|------|
| `variant` | 字符串或枚举；命名对齐 shadcn（`default` / `secondary` / `outline` / `ghost` / `destructive` / `link`…） |
| `size` | `xs` / `sm` / `default` / `lg` / `icon*` 等；与 Button 已实现对齐者优先复用 |
| 颜色 | 经 `parse_color("primary")` 等语义名；禁止组件内硬编码与 Theme 无关的 OKLCH |
| 焦点环 | 默认走 `style/focus_ring` + theme `ring`；`focus-visible` 行为与冒烟表一致 |
| 动效 | hover/focus 可用 spring/cubic；paint sync 优先，避免整树 rebuild |

### 4.4 浮层

- 统一：`utils/popup` 的 `mark_floating` / placement / underlay；`app::portal_overlay` Esc 栈。  
- Escape **只关顶层**；点击外部关闭与受控 `open` 信号单一写入点。  
- 打开时焦点陷阱（`FocusTrap`）与关闭后焦点归还须可测或可冒烟。

### 4.5 无障碍最低集

| 控件类 | Role / 行为（最低） |
|--------|---------------------|
| Button / Toggle | Button；pressed 时 toggled |
| Input / Textarea | TextField；值与焦点同步 |
| Tabs 条带 | TabList（`tabstrip:`） |
| Checkbox / Switch | 可 toggled |
| 菜单 / Command / Combobox | 打开关闭 + 键盘移动（深化见巩固期 WP） |

细节实现以 `accesskit_bridge.rs` 为准；新增前缀必须同时改 bridge 与本文或 `keyboard_nav.md`。

---

## 5. 组件清单（代码事实 · 2026-08-07）

来源：`components/mod.rs` + Gallery 分区。状态口径：**MVP** = 可演示；**工程** = 满足 §1；**Stub/实验** = README / SemVer 实验面。

### 5.1 按 Gallery 分区

| 分区 | 组件（文件） | 基线目标 |
|------|----------------|----------|
| **基础** | Button、Label、Separator、Card、Toggle、ToggleGroup、Segmented、Progress、Image、Badge 类若有 | Button/Label → 工程；其余 MVP→工程 |
| **表单** | Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Form、Select、Combobox、DatePicker、TimePicker、ColorPicker、FilePicker/DirectoryPicker | Input/Form → 工程；选器键盘深化对齐巩固期 W3 |
| **浮层 / 反馈** | Dialog、AlertDialog、Sheet、Popover、Tooltip、Dropdown、ContextMenu、Toast、Command、Portal、FocusTrap | 统一 popup + Esc；Dialog/Tooltip 优先工程 |
| **数据** | VirtualList、DataTable、TreeView、Chart（Bar/Line）、Carousel、Stepper | VirtualList 依赖测宽；Chart/Path 深化；Masonry 延后 |
| **导航** | Tabs、Menubar、Navbar、Sidebar、NavigationHost、Router 相关宿主 | Tabs headless + 外壳对齐 W3 |
| **布局** | ScrollArea、ScrollViewport、Scrollbar、Resizable | 滚动 focus 前缀与 IME 光标区协同 |
| **文本** | Label 选区、markdown inline、（块级 md 深化） | 与文本管线同源 |

### 5.2 稳定面 vs 实验面（发布口径草案）

| 稳定面（0.1 起承诺 semver） | 实验面（可破坏，须标 `experimental` 或文档警告） |
|-----------------------------|--------------------------------------------------|
| Button、Label、Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Separator、Card | Chart / Path 绘制细节、Carousel、Command 搜索算法 |
| ScrollArea / Scrollbar、Tabs、Dialog、AlertDialog、Toast、Tooltip | Date/Time/Color 本地化与时区、FilePicker 平台差异 |
| Form（校验 Context）、Select、Combobox（基础开合） | TreeView 大数据、DataTable 高级排序筛选 |
| Theme / `set_theme_mode`、utility `class` 主集 | VirtualMasonry、托盘、多窗协作细节 |

最终表以巩固期 **WP-4.1** CHANGELOG 为准；本文草案供组件 PR 归类。

### 5.3 明确不做（组件库范围）

- 换 VDOM / 弃 wgpu / 引入浏览器控件。  
- 为「纯无头公共 API」另起一套与 `components/` 平行的完整产品面（可渐进抽出 headless builder，但默认文档与 prelude 仍指向样式组件）。  
- **废除或边缘化 Builder / 纯 Rust 书写面**（或反之只保留 Builder、废弃 `ui!`）——双书写面均为一等公民，见 §3。  
- 移动 / 鸿蒙一等公民组件变体。  
- 完整 CSSOM、DOM 选择器、Web 专有 a11y 树。

---

## 6. 质量门与验收模板

### 6.1 每个新组件 / 行为破坏性改动的 PR 检查表

- [ ] 分层：新状态机是否应在 `utils`/`primitives` 而非仅组件内复制  
- [ ] 受控 / 非受控单一写入点  
- [ ] `id` / `focus_id` 前缀文档或常量导出  
- [ ] Theme token，无硬编码色  
- [ ] **双书写面**：稳定面新 props 同时进 RSX 与 Builder（或文档标明例外）；互嵌仍编译  
- [ ] Gallery 对应分区可见  
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
COMPONENT_LIBRARY.md               ← 本文件（组件权威规范）
COMPONENT_LIBRARY_PLAN.md          ← 组件库专项 WP / 波次
headless.md                        ← 三层契约细则（从属本规范）
custom_components.md               ← 用户扩展指南（双书写面示例）
gap_analysis.md §6                 ← 历史清单与 EUI 密度对照
```

争议处理：先 ADR，再改本规范；禁止静默改变「默认交付面 = shadcn 样式组件」或「双书写面互相兼容」。

---

*规范随清单与 SemVer 面更新而修订；新增公开组件必须落入 §5 分区并更新 Gallery。*
