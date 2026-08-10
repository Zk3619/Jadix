# 子树布局与上溢升父（WP-2.3）

对应 [ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-2.3**。

## 契约

| API | 作用 |
|-----|------|
| [`mark_element_layout_dirty`](../crates/rui/src/layout/layout_tree.rs) / `mark_dirty_nodes_in_tree` | 推 style/measure + Taffy `mark_dirty`（含祖先缓存失效） |
| [`pick_subtree_compute_root`](../crates/rui/src/layout/layout_tree.rs) | 脏节点同父 → 该父；否则 `None`（app root） |
| [`promote_compute_root_for_size_overflow`](../crates/rui/src/layout/layout_tree.rs) | 从候选根沿祖先上溢：内容尺寸（`size` 为 `Auto`）则升父 |
| [`resolve_layout_compute_root`](../crates/rui/src/layout/layout_tree.rs) | `pick` + `promote` |

**上溢**：子尺寸变化可能导致祖先**外尺寸**变化（宽或高为 `Auto`）。此时浅层 `compute_layout` 不足以带动外层兄弟重排，必须升到更高祖先或 app root。

## 实装路径（live）

`AppState` LayoutChanged：**始终** `mark_dirty` 后从 **app root** `compute_layout`（`subtree_root: None`）。

原因：Taffy 把传入的 compute 节点当作布局根，会将其 `location` 置零，导致绝对坐标塌缩（见 `showcase_collapse` / gallery Show）。根 compute 仍靠脏缓存跳过干净兄弟。

`resolve_layout_compute_root` 固化升父规则并供库测 / 未来安全子树 compute 使用；**当前 live 不传入非 `None` 的 subtree root**。

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
