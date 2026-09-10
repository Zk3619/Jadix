# 子树布局与上溢升父（WP-2.3）

> **文档地位**：布局脏节点、`mark_dirty` 与尺寸上溢升父的**事实契约**（Taffy 子树 compute 与 live app-root compute 边界）。  
> **冻结状态**：**已冻结**（2026-09-10；库测 `subtree_promote::` / `layout_changed_mark_dirty` / `showcase_collapse` 绿 + Gallery **布局 → WP-2.3** 视觉验收）。纠错（行号、测试命令、Gallery id）可改；**策略变更**（升父规则、live `subtree_root` 策略、Taffy compute 根语义）须改本文并跑 §自动化 测，禁止静默改布局剪枝。  
> **执行源**：WP-2.3 已退出（[`AUTHORITY.md`](../AUTHORITY.md)，2026-08-10 功能签署）。细则见本文；历史 WP 展开见 [`archive/ENGINEERING_IMPLEMENTATION_PLAN.md`](../archive/ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-2.3**。  
> **实现锚点**：[`layout/layout_tree.rs`](../../crates/rui/src/layout/layout_tree.rs)（`pick_subtree_compute_root` / `promote_compute_root_for_size_overflow` / `resolve_layout_compute_root` / `mark_dirty_nodes_in_tree`）· [`app/state.rs`](../../crates/rui/src/app/state.rs)（LayoutChanged：`subtree_root: None`）· [`crate_tests/layout/subtree_promote.rs`](../../crates/rui/src/crate_tests/layout/subtree_promote.rs) · [`crate_tests/layout/showcase_collapse.rs`](../../crates/rui/src/crate_tests/layout/showcase_collapse.rs) · Gallery [`layout.rs`](../../examples/gallery/src/pages/layout.rs)（`LayoutSubtreeShowcase`）。  
> **日期**：2026-09-10（治理层冻结签署；功能已于 2026-08-10 Win 开发机签署）。

交叉：[`damage.md`](damage.md) · [`box_model.md`](box_model.md)

---

## 契约

| API | 作用 |
|-----|------|
| [`mark_element_layout_dirty`](../../crates/rui/src/layout/layout_tree.rs) / `mark_dirty_nodes_in_tree` | 推 style/measure + Taffy `mark_dirty`（含祖先缓存失效） |
| [`pick_subtree_compute_root`](../../crates/rui/src/layout/layout_tree.rs) | 脏节点同父 → 该父；否则 `None`（app root） |
| [`promote_compute_root_for_size_overflow`](../../crates/rui/src/layout/layout_tree.rs) | 从候选根沿祖先上溢：内容尺寸（`size` 为 `Auto`）则升父 |
| [`resolve_layout_compute_root`](../../crates/rui/src/layout/layout_tree.rs) | `pick` + `promote` |

**上溢**：子尺寸变化可能导致祖先**外尺寸**变化（宽或高为 `Auto`）。此时浅层 `compute_layout` 不足以带动外层兄弟重排，必须升到更高祖先或 app root。

## 实装路径（live）

`AppState` LayoutChanged：**始终** `mark_dirty` 后从 **app root** `compute_layout`（`subtree_root: None`）。

原因：Taffy 把传入的 compute 节点当作布局根，会将其 `location` 置零，导致绝对坐标塌缩（见 `showcase_collapse` / gallery Show）。根 compute 仍靠脏缓存跳过干净兄弟。

`resolve_layout_compute_root` 固化升父规则并供库测 / 未来安全子树 compute 使用；**当前 live 不传入非 `None` 的 subtree root**。

## 与 damage / collect 的边界

布局剪枝（本 WP）**不等于**绘制剪枝：

| | 布局（WP-2.3） | Collect / present（WP-2.2 + B8） |
|--|----------------|----------------------------------|
| 跳过干净兄弟 | Taffy 脏缓存跳过重算 | 须跨帧 display list 或 layer 才「不重画仍看得见」 |
| 升父 | 尺寸上溢 → 更高 compute 根 | 几何脏区 / FullClear；重叠与 portal 另算 |
| 细则 | 本文 | [damage.md](damage.md) 四层模型与 ADR |

## 自动化

```powershell
cargo test -p rui --lib subtree_promote::
cargo test -p rui --lib layout_changed_mark_dirty
```

## Gallery 视觉验收

```powershell
cargo run -p gallery
```

侧栏 **布局** → 卡片 **WP-2.3) 子树布局上溢升父**：

1. 点「加宽 chip」：中间 wrap 行增高，下方 footer **下移**（非被裁切叠住）。
2. 点「复位」：恢复单行高度与 footer 位置。

**签署**：☑ 开发机 Win（2026-08-10，Kingdom / Win 10.0.26200）已按上表确认。
