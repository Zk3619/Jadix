# Windows 默认栈、Element 模型与对标框架

> **文档地位**：OS 线程栈预算、`Element` 按值树与对标框架差异的**领域规范**（「是什么 / 该学什么」）。  
> **执行源**：巩固期排期仍以 [`AUTHORITY.md`](AUTHORITY.md) 为准；本文**不**单独开 WP 编号。  
> **事实来源**：`crates/rui/src/core/tree.rs`、`layout/layout_tree.rs`、`core/reactivity.rs::with_ui_build_stack`、`platform/windows/stack.rs`、`crate_tests/layout/stack_budget.rs`、`examples/gallery/src/mount_tests.rs`；对标目录 `Reference/`（GPUI、Floem、egui、Slint、Xilem、Iced 等）。  
> **审计日期**：2026-09-05

---

## 0. 一句话结论

**其它保留树框架几乎都不靠把主线程 PE 栈加到 8MB 过日子。** 它们优先保证：**节点本体在堆上（或只是 ID）**，调用栈上只剩指针/句柄；深遍历改迭代或加深度帽；只有极少数递归热点才分段换栈。

RUI 的 gallery 在 Windows **默认 ~1 MiB** 主线程栈上仍可能溢出，**主因是构建帧**（巨型 `ui!` 在同一函数里按值铺开 `Element`），**不是** layout collect（已压到小帧 + 堆队列）。`with_ui_build_stack` 是框架边界的**过渡保险**，工程终态应对齐 GPUI / Iced / Floem 的**子节点指针化**，而不是在「PE 8MB」与「临时 grow 8MB」之间二选一。

---

## 1. Windows 默认栈与 RUI 当前策略

| 机制 | 行为 | 角色 |
|------|------|------|
| **PE `/STACK`** | Windows GUI 主线程默认保留 **~1 MiB**（`0x100000`） | 行业基线；RUI **不**在框架或 gallery 里 emit 链接参数 |
| **`with_ui_build_stack`** | 剩余栈 **< 1 MiB** 时，`stacker::maybe_grow` 为闭包 **临时** 再借最多 **8 MiB**；闭包结束释放 | 包在 `run`、根 `app_builder`、`RedrawRequested`、输入处理等边界（见 `lifecycle.rs` / `state.rs`） |
| **`platform/windows/stack.rs`** | 提供 `EXE_STACK_RESERVE` / `msvc_link_arg()` 供**下游二进制**在自有 `build.rs` 里 opt-in | 测过最坏路径后的最后手段，**不是**框架默认 |
| **`stack_budget` 测试** | 在 **1 MiB** 线程上断言 calculator 形树 layout 总用量 **< 768 KiB**；`collect_frame` **< 12 KiB** | 红/绿循环；防止 collect 回退到 ~21 KiB/帧 |

```text
优先级（工程标准）：

  ① 节点边界指针化 / arena（决定性）
  ② 深遍历迭代 + 深度帽 + 小栈帧（layout collect 已部分落地）
  ③ 框架边界 maybe_grow（过渡）
  ④ 应用 PE /STACK（个别二进制、测过后 opt-in）
```

**明确降级**（巩固期禁区，见 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) §7）：

- 框架或示例默认 `println!("cargo:rustc-link-arg=/STACK:…")`
- 无条件 `stacker::grow` 包整条 UI 线程
- 要求应用作者把 showcase **仅因 MSVC debug 栈**拆成 `{ fn() }`，当作长期 DX 规范

---

## 2. 总表：对标框架如何处理「栈」

| 框架 | 节点放哪 | 构建时调用栈上是什么 | 布局 / 绘制遍历 | 深度 / 递归保护 | 加大 OS 栈？ |
|------|----------|----------------------|-----------------|-----------------|--------------|
| **GPUI** | 帧 arena 里的 `AnyElement`（指针） | `div().child(...)` 立刻 arena 分配 | **递归** `request_layout` / `prepaint` / `paint` | 无硬上限；`stacksafe` 分段扩栈 | **不**改 PE `/STACK`；递归热点 `#[stacksafe]` |
| **Floem** | `ViewId` + `Box<dyn View>` 槽位图 | 元组/迭代器立刻 `into_any()` 装箱 | **绘制迭代 DFS**；事件仍递归；布局走 taffy | 祖先链 1000；选择器解析 20 | 无 |
| **egui** | 无保留控件树；`UiStack` 是堆上 `Arc` 链表 | 小 widget 当场画完 | 嵌套闭包 = 调用深度 | 无树深度帽；大 LUT 改堆 | 无 |
| **Slint** | 编译后 `VRc` item tree | 用户代码几乎不堆巨型临时节点 | vtable 递归 visit | 绑定环检测；预览求值 >20 截断 | **仅 lsp/viewer 工具** `/STACK:8MB`；**运行时库不强制** |
| **Xilem / Masonry** | `WidgetArena` 里 `Box<dyn Widget>`；`WidgetPod` 是 ID | `NewWidget` 构造即 `Box` | **递归** `run_layout` / paint | 无硬上限 | 无 |
| **Iced** | `Element { widget: Box<dyn Widget> }` | 构造即装箱 | 递归 layout/draw | 无 | 无 |
| **RUI 现状** | `Element` **按值** + `Vec<Element>` 子树 | 巨型 `ui!` 单函数临时槽 | collect 递归 + 堆 DFS 脏区；`CollectPainted` 小帧 | `Element::drop` 迭代释放子树 | 框架 **不** emit `/STACK`；边界 `maybe_grow` |

---

## 3. 分框架要点（学什么 / 不搬什么）

### 3.1 GPUI — 与 RUI 最可比

- **子节点立刻进 arena**，父节点里只有 `ArenaBox` / `StackSafe` 指针；`.child()` 不会在父帧按值嵌整棵子树。
- 布局/绘制**仍递归**，但 `#[stacksafe]` 在剩余栈不足时用 `stacker` **换一段栈**继续跑（热点级，非 PE bump）。
- **对 RUI**：若坚持默认 1MB 且不在每个 `paint` 上换栈，就不能只抄「递归遍历」而不抄「节点指针化」。

### 3.2 Floem — 绘制已迭代；View 是句柄

- `View` 实现体带 `ViewId`；对象在 `SlotMap` + `Box<dyn View>`。
- **`collect_visual_order`**：显式 `Vec` DFS，再按扁平 `paint_order` 绘制（*explicit traversal*）。
- 构建侧：`stack((a,b,c))` → `into_views()` → `Vec<Box<dyn View>>`，构建帧瘦。
- **对 RUI**：Floem 绘制路径 ≈ 讨论过、尚未对 `collect_instances_mut` 整段落地的迭代 DFS；构建侧 ≈ **子节点强制装箱**。

### 3.3 egui — 结构性消掉「一页 N 个肥节点」

- `UiStack` 是堆上 `Arc` 链表，不是 OS 栈上的控件树。
- 嵌套 `ui.horizontal(|ui| …)` 的深度 ≈ 容器层数；没有「父函数同时活着 20 个 3.6KB `Element`」。
- 栈问题多来自「大 LUT 误放栈上」→ 改 boxed slice（[#5212](https://github.com/emilk/egui/pull/5212)）。
- **对 RUI**：immediate mode 不可照搬，但 **「不要在一个栈帧里同时存在整页按值 Element」** 同样适用。

### 3.4 Slint — 8MB 只给编译器工具

- 运行时 item tree 在堆上 `VRc`；用户 `.slint` 不展开成「一个 Rust 函数里 200 个肥节点」。
- **`/STACK:8000000` 仅出现在 `tools/lsp` / `tools/viewer` 的 `build.rs`**（编译器 debug 递归下降），**不是** widget 运行时。
- **对 RUI**：同意「加大栈 = 工具/编译器权宜」；不同意「每个 app 二进制默认 8MB」。

### 3.5 Xilem / Iced — 递归能活因为每层栈帧瘦

- 容器里是 `Vec<WidgetPod>` / `Vec<Element>`（指针大小 ID 或 `Box<dyn Widget>`）。
- 布局递归，但**没有**按值肥 widget 叠在父帧里。
- **对 RUI**：collect 曾单帧 ~21 KiB 是「递归 + 肥帧」叠加；现已压帧，矛盾转到 **构建帧**。

---

## 4. RUI 现状（代码事实）

### 4.1 `Element` 仍是按值树

`style` / `text_style` 已 `Box` 化，但 **`children: Vec<Element>`** 仍按值拥有子树；MSVC debug 会为函数内每个 `Element` 临时量开槽：

```766:771:crates/rui/src/core/tree.rs
pub struct Element {
    /// Boxed so Element stays cheap to move. MSVC debug reserves stack for every
    /// `Element` temporary in a function prologue; a 3KB+ inline `Style` overflows
    /// the Windows ~1 MiB main thread on wide builder / `ui!` trees.
    pub style: Box<crate::style::Style>,
    pub children: Vec<Element>,
```

`size_of::<Element>()` 约 **3.6 KiB**（见 `stack_budget` 测试 `layout_type_sizes_are_documented`）。[`size_and_memory.md`](size_and_memory.md) §9.3 的「数百字节」口径已过时，以本文与 `stack_budget` 为准。

### 4.2 已对齐对标框架的部分

| 项 | 实现 | 对标 |
|----|------|------|
| **Drop 迭代** | `Element::drop` 用 `Vec` drain，避免按值子树递归 Drop | Floem/GPUI 靠 Box/arena 自然变浅 |
| **collect 小帧** | `CollectPainted` 等小结构；`collect_frame` 门禁 < 12 KiB | 不再 ~21 KiB/帧 |
| **堆 DFS** | 脏区 / inherit 等 walk 用 heap worklist | Floem `collect_visual_order` 方向 |
| **不强制 PE `/STACK`** | `platform/mod.rs` 明确框架不 emit | Slint 运行时库、GPUI、Iced 等 |
| **边界 `maybe_grow`** | `with_ui_build_stack` | GPUI `stacksafe` 的**粗粒度**过渡版 |

### 4.3 仍未对齐、gallery 仍易爆的部分

- **无人**把「样式 + 事件 + 子树 + 动画快照」打成 ~3.6 KiB 按值节点后，再让宏在**同一函数**构造整页（如大卡片 `MarkerShowcase`）。
- 子节点边界**没有**统一为 `Box<Element>` / `ElementId` + arena / 构造即堆分配。
- collect **仍部分递归**；Floem 式整段绘制迭代 DFS 未完全落地。
- 应用层拆 `ui!` / `#[component]` 可缓解 MSVC debug，**不应**当作框架终态 DX。

### 4.4 测试契约

- **`stack_budget`**：`cargo test -p rui stack_budget` — 1 MiB 线程 + calculator 形树。
- **Gallery `mount_tests`**：`run_on_gallery_build_stack` = 1 MiB 线程 + `with_ui_build_stack`，模拟 PE 默认 + 框架边界保险。

---

## 5. `with_ui_build_stack` 在优先级中的位置

| 档位 | 机制 | 评价 |
|------|------|------|
| **第 1 档** | 子节点指针化 / arena / `ui!` lowering 构造即堆分配 | **终态**；对标 GPUI / Iced / Floem |
| **第 2 档** | collect 全迭代、深度帽、小栈帧、`stack_budget` | **已做 / 持续** |
| **第 3 档** | `with_ui_build_stack`（`maybe_grow` 8 MiB） | **过渡**；`(1)` 完成后应收窄调用面 |
| **第 4 档** | 应用 `build.rs` + `platform/windows/stack.rs` 常量 | **opt-in**；Slint 仅工具链 |

`with_ui_build_stack` **不是** Slint 式「运行时库默认大栈」，也**不完全等于** GPUI 的 per-function `#[stacksafe]`：它包的是整段 `app_builder` / 整帧 redraw，粒度更粗。

---

## 6. 工程终态方向（未承诺排期）

按行业共识的落地顺序：

1. **子节点堆分配 / arena** — 改 `Element` 子树存储，或改 `ui!` lowering（对标 GPUI `AnyElement`、Iced `Box<dyn Widget>`）。
2. **collect 绘制遍历迭代化** — 对标 Floem `collect_visual_order`。
3. **评估收窄 `with_ui_build_stack`** — 在 (1)(2) 绿后再测是否仅 debug / 极端宏路径需要。
4. **禁止**把 gallery 旧 `build.rs` `/STACK:16MB` 或无条件 `grow` 拉回默认路径。

---

## 7. 相关文档

| 文档 | 关系 |
|------|------|
| [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) | ADR-10；巩固期禁区 |
| [`REFERENCE_COMPARISON.md`](REFERENCE_COMPARISON.md) | 六参考「该学 / 不该搬」总表 |
| [`size_and_memory.md`](size_and_memory.md) | RSS / Element 节点计数（栈预算见本文） |
| [`bugs.md`](bugs.md) | 历史「计算器需 8MB」已关闭，指向本文 |
| [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md) | 结构岛 / `isolate_reads` 与构建帧 |

---

*修订栈策略或 `Element` 子树存储时，须同步更新本文、`stack_budget` 测试与 ADR-10。*
