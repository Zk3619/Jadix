# Windows 默认栈、Element 模型与对标框架

> **文档地位**：OS 线程栈预算、`Element` 堆句柄与对标框架差异的**领域规范**（「是什么 / 该学什么」）。**已冻结**（2026-09-05）。  
> **执行源**：巩固期排期仍以 [`AUTHORITY.md`](../AUTHORITY.md) 为准；本文**不**单独开 WP 编号。  
> **事实来源**：`crates/rui/src/core/tree.rs`、`layout/layout_tree.rs`、`core/reactivity.rs::with_ui_build_stack`、`platform/windows/stack.rs`、`crate_tests/layout/stack_budget.rs`、`examples/gallery/src/mount_tests.rs`；对标目录 `Reference/`（GPUI、Floem、egui、Slint、Xilem、Iced 等）。  
> **审计日期**：2026-09-05（句柄 / 堆 DFS / stacksafe 同构 / TLS 节点 slab / 组件壳走树）。  
> **禁区**：禁止用 PE `/STACK`、无条件 `stacker::grow` 或 **fork taffy** 绕过栈预算。策略变更须修订 ADR-10，不得默默回潮。

---

## 0. 一句话结论

**其它保留树框架几乎都不靠把主线程 PE 栈加到 8MB 过日子。** 它们优先保证：**节点本体在堆上（或只是 ID）**，调用栈上只剩指针/句柄；深遍历改迭代或加深度帽；只有极少数递归热点才分段换栈。

RUI 的 gallery 在 Windows **默认 ~1 MiB** 主线程栈上曾溢出，**主因是构建帧**（巨型 `ui!` 在同一函数里按值铺开肥 `Element`），不是 Taffy。**第 1–3 档已落地**：`Element` 堆句柄；collect / class·CSS / 命中 / overlay 查找堆 DFS；`with_ui_build_stack` = GPUI `#[stacksafe]` 同构（**64 KiB 红区 / 1 MiB 段**），打在用户闭包与 Taffy **入口**。绘制与命中已迭代（GPUI 的 paint / hit-test 仍递归 + stacksafe）。**节点分配**：TLS 回收 `Box<ElementInner>`（保留树的 arena 对应物；**不是** GPUI 每帧 bump）。**Taffy `compute_layout` 内部仍递归**——与 GPUI 同一限制；深链测试帽 **128** wrappers。不要用加大 PE 栈或 fork taffy 来绕。

---

## 1. Windows 默认栈与 RUI 当前策略

| 机制 | 行为 | 角色 |
|------|------|------|
| **PE `/STACK`** | Windows GUI 主线程默认保留 **~1 MiB**（`0x100000`） | 行业基线；RUI **不**在框架或 gallery 里 emit 链接参数 |
| **`with_ui_build_stack`** | 剩余栈 **< 64 KiB** 时 `stacker::maybe_grow` 再借 **1 MiB** 段（可叠） | GPUI `#[stacksafe]` 同构：`run_effect` 用户体、`component_root`、Taffy `compute_layout` 入口、每条 UI 任务、file-drop 回调。**仅** HMR 全量 `app_builder` 仍在事件处理里显式包（不走 `run_effect`）。**不**包 click/key/touch/redraw 整段 |
| **`platform/windows/stack.rs`** | 提供 `EXE_STACK_RESERVE` / `msvc_link_arg()` 供**下游二进制**在自有 `build.rs` 里 opt-in | 测过最坏路径后的最后手段，**不是**框架默认 |
| **`stack_budget` 测试** | 在 **1 MiB** 线程上断言 calculator 形树 layout 总用量 **< 768 KiB**；`collect_frame` **< 12 KiB**；`size_of::<Element>()` = 指针 | 红/绿循环；防止 collect 回退到 ~21 KiB/帧，防止 `Element` 重新变肥 |

```text
优先级（工程标准，①②③ 已落地；④ 仍为应用 opt-in）：

  ① 节点边界指针化 / arena（句柄 ☑；TLS `ElementInner` slab ☑。不是 GPUI 每帧 bump）
  ② 深遍历迭代 + 深度帽 + 小栈帧（RUI 走树已落地；Taffy 递归 + 128 帽为接受差距）
  ③ 热点 maybe_grow（GPUI stacksafe 同构：64 KiB 红区 / 1 MiB 段）
  ④ 应用 PE /STACK（个别二进制、测过后 opt-in）
```

**明确降级**（巩固期禁区，见 [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) §7）：

- 框架或示例默认 `println!("cargo:rustc-link-arg=/STACK:…")`
- 无条件 `stacker::grow` 包整条 UI 线程
- 要求应用作者把 showcase **仅因 MSVC debug 栈**拆成 `{ fn() }`，当作长期 DX 规范
- **fork taffy** 只为给 `compute_layout` 逐层加 stacksafe（见 §4.5）

---

## 2. 总表：对标框架如何处理「栈」

| 框架 | 节点放哪 | 构建时调用栈上是什么 | 布局 / 绘制遍历 | 深度 / 递归保护 | 加大 OS 栈？ |
|------|----------|----------------------|-----------------|-----------------|--------------|
| **GPUI** | 帧 arena 里的 `AnyElement`（指针） | `div().child(...)` 立刻 arena 分配 | **递归** `request_layout` / `prepaint` / `paint`；Taffy `compute_layout_with_measure` | 无硬上限；`stacksafe` 分段扩栈（**挡不住 Taffy 内部**，见 [taffy#853](https://github.com/DioxusLabs/taffy/discussions/853)） | **不**改 PE `/STACK` |
| **Floem** | `ViewId` + `Box<dyn View>` 槽位图 | 元组/迭代器立刻 `into_any()` 装箱 | **绘制迭代 DFS**；事件仍递归；布局走 taffy | 祖先链 1000；选择器解析 20 | 无 |
| **egui** | 无保留控件树；`UiStack` 是堆上 `Arc` 链表 | 小 widget 当场画完 | 嵌套闭包 = 调用深度 | 无树深度帽；大 LUT 改堆 | 无 |
| **Slint** | 编译后 `VRc` item tree | 用户代码几乎不堆巨型临时节点 | vtable 递归 visit | 绑定环检测；预览求值 >20 截断 | **仅 lsp/viewer 工具** `/STACK:8MB`；**运行时库不强制** |
| **Xilem / Masonry** | `WidgetArena` 里 `Box<dyn Widget>`；`WidgetPod` 是 ID | `NewWidget` 构造即 `Box` | **递归** `run_layout` / paint | 无硬上限 | 无 |
| **Iced** | `Element { widget: Box<dyn Widget> }` | 构造即装箱 | 递归 layout/draw | 无 | 无 |
| **RUI** | `Element` **句柄**（8B）+ 堆上 `ElementInner`（≈2.4 KiB）；卸载后进 TLS slab | `ui!` 临时量是指针；`Element::new` 优先 slab，否则 `Box` | collect / class·CSS / 命中 / overlay 查找 = 堆 DFS；Taffy 仍递归 | `Element::drop` / Owner dispose 迭代；`stack_budget` 128（Taffy）/ 4k（RUI 走树与 template 查找） | **不** emit `/STACK`；`stacksafe` 式 64 KiB 红区 |

---

## 3. 分框架要点（学什么 / 不搬什么）

### 3.1 GPUI — 与 RUI 最可比

- **子节点立刻进 arena**，父节点里只有 `ArenaBox` / `StackSafe` 指针；`.child()` 不会在父帧按值嵌整棵子树。
- 布局/绘制**仍递归**，但 `#[stacksafe]` 在剩余栈不足时用 `stacker` **换一段栈**继续跑（热点级，非 PE bump）。
- **Taffy 用法**：每帧 `TaffyTree::clear` + `request_layout` 重建；`#[stacksafe] compute_layout` → `compute_layout_with_measure`；`disable_rounding`。Zed **明确不 fork taffy**（#853）；入口 stacksafe 仍挡不住算法内部按树深递归。
- **对 RUI**：节点句柄已对齐「构造即堆分配」。GPUI 的帧 bump 不能照搬（RUI 是保留树，见 ADR-2）；对应物是 **TLS 回收 `Box<ElementInner>`**（跨帧 free-list）。绘制、命中、overlay 查找是 Floem 式**堆 DFS**（超过 GPUI 的递归 paint / hit-test）。`with_ui_build_stack` 与 `#[stacksafe]` 红区语义相同。保留树 + `mark_dirty` 是相对 GPUI 每帧重建的差异，与是否 fork taffy 无关。

### 3.2 Floem — 绘制已迭代；View 是句柄

- `View` 实现体带 `ViewId`；对象在 `SlotMap` + `Box<dyn View>`。
- **`collect_visual_order`**：显式 `Vec` DFS，再按扁平 `paint_order` 绘制（*explicit traversal*）。
- 构建侧：`stack((a,b,c))` → `into_views()` → `Vec<Box<dyn View>>`，构建帧瘦。
- **对 RUI**：Floem 绘制路径 ≈ 已对 `collect_instances_mut` 落地的迭代 DFS；构建侧 ≈ **子节点强制装箱**。扁平 `paint_order` **不必再做**：RUI 已在 collect 里用 `OverlayPaintSplit`（portal bands + `page_stack_bands`）按卡 **rects→text**，覆盖 Floem 拆第二层列表所解决的 z-index / 字形盖住面板问题。再摊一张 `Vec<NodeId>` 会与 clip / sticky / portal hoist 双源，高风险且不减栈。

### 3.3 egui — 结构性消掉「一页 N 个肥节点」

- `UiStack` 是堆上 `Arc` 链表，不是 OS 栈上的控件树。
- 嵌套 `ui.horizontal(|ui| …)` 的深度 ≈ 容器层数；没有「父函数同时活着 20 个肥节点」。
- 栈问题多来自「大 LUT 误放栈上」→ 改 boxed slice（[#5212](https://github.com/emilk/egui/pull/5212)）。
- **对 RUI**：immediate mode 不可照搬，但 **「不要在一个栈帧里同时存在整页肥节点」** 同样适用（现已改为指针句柄）。

### 3.4 Slint — 8MB 只给编译器工具

- 运行时 item tree 在堆上 `VRc`；用户 `.slint` 不展开成「一个 Rust 函数里 200 个肥节点」。
- **`/STACK:8000000` 仅出现在 `tools/lsp` / `tools/viewer` 的 `build.rs`**（编译器 debug 递归下降），**不是** widget 运行时。
- **对 RUI**：同意「加大栈 = 工具/编译器权宜」；不同意「每个 app 二进制默认 8MB」。

### 3.5 Xilem / Iced — 递归能活因为每层栈帧瘦

- 容器里是 `Vec<WidgetPod>` / `Vec<Element>`（指针大小 ID 或 `Box<dyn Widget>`）。
- 布局递归，但**没有**按值肥 widget 叠在父帧里。
- **对 RUI**：collect 曾单帧 ~21 KiB 是「递归 + 肥帧」叠加；现已压帧并改堆 DFS。构建帧已是指针句柄。

---

## 4. RUI 现状（代码事实）

### 4.1 `Element` 是堆句柄（第 1 档）

`Element` 是 `Box<ElementInner>` 句柄（对标 Iced `Box<dyn Widget>` / GPUI arena-box）。`Element::new` 标 `#[inline(never)]`，避免 MSVC debug 把 `ElementInner` 临时量内联进宽 `ui!` 调用帧。公开字段经 `Deref` 保持 `el.style` / `el.children` 写法；同一调用要拆两个字段时走 `el.inner` 或 `tick_css_*`。

```802:805:crates/rui/src/core/tree.rs
pub struct ElementInner {
    pub style: Box<crate::style::Style>,
    pub children: Vec<Element>,
```

```996:1000:crates/rui/src/core/tree.rs
pub struct Element {
    /// Heap payload. Prefer `el.field` via `Deref`; use `el.inner.a` / `el.inner.b`
    /// when one call must mutably borrow two fields (Deref cannot split-borrow).
    pub inner: Box<ElementInner>,
}
```

`stack_budget` 测试 `layout_type_sizes_are_documented` 实测（debug）：`size_of::<Element>()` = **8**；`size_of::<ElementInner>()` ≈ **2.4 KiB**（2408）。旧「按值 Element ≈ 3.6 KiB」口径作废。子树仍是 `Vec<Element>`（句柄向量）。卸载的子节点 `Box<ElementInner>` 进 TLS slab（cap 4096），`Element::new` 优先 pop；**不是** GPUI 那种每帧 `clear` 的 bump arena（保留树跨帧持有 `layout_node`）。

### 4.2 已落地（相对对标）

| 项 | 实现 | 对标 |
|----|------|------|
| **节点句柄** | `Element` = `Box<ElementInner>`；`new` 即堆分配 | Iced `Box<dyn Widget>`；GPUI 立刻 arena |
| **TLS 节点 slab** | Drop 子节点回收 `Box<ElementInner>`；cap 4096；根节点本次 Drop 不回收 | 保留树对应物；**禁止**每帧 bump（会毁掉跨帧 `layout_node` / reconcile） |
| **Drop / Owner** | `Element::drop`、`dispose_owner` 用堆 worklist | 避免子树 / 深 Owner 递归 Drop |
| **collect 小帧** | `CollectPainted` 等小结构；`collect_frame` 门禁 < 12 KiB | 不再 ~21 KiB/帧 |
| **堆 DFS** | collect / build / cascade / 每帧 class·CSS / 命中 / overlay `template_id` / portal 探测 / a11y `walk_subtree` | Floem `collect_visual_order`；命中超过 GPUI |
| **组件壳 find / stamp / 每帧 subtree sync** | `find_in_tree` / `find_by_attr` / `walk_elements_mut(_pruned)` | 槽位探测、caption、Button hover、Sidebar rail / icon chrome 不再按树深递归；4k 测 `component_slot_find_stays_on_heap` |
| **不强制 PE `/STACK`** | `platform/mod.rs` 明确框架不 emit | Slint 运行时库、GPUI、Iced 等 |
| **边界 `maybe_grow`** | 64 KiB 红区 / 1 MiB 段；用户闭包 + Taffy **入口**；HMR `app_builder` 显式包 | 与 GPUI `stacksafe` **同级**（两边都挡不住 Taffy 内部） |

### 4.3 冻结边界与接受差距

| 项 | 口径 | 状态 |
|------|------|------|
| **Taffy `compute_layout` 仍递归** | 深链帽 **128**；入口 `maybe_grow` 只避免在红区里**启动** Taffy | **接受差距**（见 §4.5；与 GPUI 同一限制；**不 fork**） |
| **TLS 节点 slab** | 卸载子节点回收；根 Drop 不回收；reset 会重建内层 `Box<Style>` / `Box<TextStyle>` | **☑** `element_inner_slab_recycles_unmounted_children` |
| **组件壳走树** | 生产路径槽位 / caption / template / 每帧 subtree sync / Sidebar icon chrome 走 `find_in_tree` · `walk_elements_mut(_pruned)` | **☑** `component_slot_find_stays_on_heap` |
| **测试浅树 `find_slot`** | 单测夹具上的局部递归辅助 | **非热路径**；不挡冻结 |
| **扁平 `paint_order`** | Floem 两阶段再画 | **不做**（见 §3.2；`OverlayPaintSplit` 已覆盖 stacking） |
| **HMR `app_builder` 显式包** | 全量重建不经过 `run_effect` | 正确的闭包边界，不是粗粒度事件 wrap |
| **应用拆 `ui!`** | 可缓解其它肥栈帧 | **不是**框架终态 DX |

### 4.4 测试契约

- **`stack_budget`**：`cargo test -p rui stack_budget` — 1 MiB 线程 + calculator 形树；断言 `size_of::<Element>()` 等于指针大小；TLS slab 回收；4k 深链上跑 class/CSS 走树、overlay `template_id` 查找与组件槽位 find（**不**跑 Taffy）。Taffy 深链测帽 [`DEEP_CHAIN_WRAPPERS = 128`](../../crates/rui/src/crate_tests/layout/stack_budget.rs)。
- **Gallery `mount_tests`**：`run_on_gallery_build_stack` = 1 MiB + `with_ui_build_stack`（宽页构建保险）；`*_without_grow` / `button_showcase_fits_pe_stack` = 纯 1 MiB，断言句柄化后默认壳与 basics 不再依赖 grow。

### 4.5 Taffy：保持上游、不 fork

两边都是 **`TaffyTree` + `compute_layout_with_measure` + `disable_rounding`**。RUI 锁 **taffy 0.13.0**；树跨帧保留（`set_style` / `set_children` / `mark_dirty`），live 从 app root compute。GPUI 每帧 `clear` 再建，测宽闭包挂在 `NodeContext`。

入口加 stacksafe / `with_ui_build_stack` **不能**给 Taffy 每一层换栈。Zed 在 [taffy#853](https://github.com/DioxusLabs/taffy/discussions/853) 复现后选择 **不 fork**；维护者建议深度帽、低级 `LayoutPartialTree`、或上游 feature flag。截至 2026-09-05 crates.io **没有** `stacksafe` feature。真实 UI 深度多在 10–30，极端网页约 60；128 帽宽于该范围。

**决定**：保持 taffy；低级 API / 上游 PR 仅当合法 UI 深度 > ~100 且 1 MiB 上 compute 必炸时再开 ADR。低级 API 工作量接近半套自研，不比 fork 轻。这是写入本文的**接受差距**，不是未完成项。

---

## 5. `with_ui_build_stack` 在优先级中的位置

| 档位 | 机制 | 评价 |
|------|------|----------|
| **第 1 档** | 子节点指针化 / TLS `ElementInner` slab / `ui!` lowering 构造即堆分配 | **☑** 每节点 `Box<ElementInner>` + 卸载回收 |
| **第 2 档** | collect 全迭代、深度帽、小栈帧、`stack_budget` | **☑** RUI 走树；Taffy + 128 帽为接受差距 |
| **第 3 档** | `with_ui_build_stack`（64 KiB 红区 / 1 MiB 段） | **☑** 用户闭包 + Taffy 入口 |
| **第 4 档** | 应用 `build.rs` + `platform/windows/stack.rs` | **opt-in**；Slint 仅工具链 |

遍历侧**超过** GPUI：collect / 命中 / overlay 查找已是堆 DFS。换栈粒度与 GPUI **对齐**（不足才换栈）。Taffy 内部递归两边都未解决。

---

## 6. 当前终态（已冻结）

1. **子节点堆分配 + TLS slab** — ☑ `Box<ElementInner>`（对标 Iced）；卸载子节点回收。GPUI 式**每帧 bump** 禁止接到保留树上。
2. **RUI 走树迭代化** — ☑ collect / class·CSS / 命中 / overlay 查找 / Drop / Owner dispose / 组件壳 find 与每帧 subtree sync（含 Button hover、Sidebar rail / icon chrome）。Taffy 递归 + 128 帽接受。扁平 `paint_order` **不做**（§3.2）。
3. **热点 `with_ui_build_stack`** — ☑ 64 KiB / 1 MiB；用户闭包边界 + Taffy 入口。
4. **禁止**把 gallery 旧 `build.rs` `/STACK:16MB`、无条件 `grow`、或 fork taffy 拉回默认路径。

---

## 7. 相关文档

| 文档 | 关系 |
|------|------|
| [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) | ADR-10；巩固期禁区 |
| [`REFERENCE_COMPARISON.md`](../comparison/REFERENCE_COMPARISON.md) | 六参考「该学 / 不该搬」总表 |
| [`size_and_memory.md`](../ops/size_and_memory.md) | RSS / Element 节点计数（栈预算见本文） |
| [`bugs.md`](../archive/bugs.md) | 历史「计算器需 8MB」已关闭，指向本文 |
| [`reactivity_and_hot_reload.md`](../ops/reactivity_and_hot_reload.md) | 结构岛 / `isolate_reads` 与构建帧 |

---

*纠错（行号、测试命令）可改；策略变更须同步 ADR-10 与 `stack_budget`。本文已冻结：Taffy 内部递归为接受差距，其余开放项已收口或显式不做。*
