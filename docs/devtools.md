# RUI DevTools / Inspector（B1）

> **文档地位**：运行时 Inspector 的产品形态、API 面、分阶段验收与禁区。  
> **执行源**：[`AUTHORITY.md`](AUTHORITY.md) §7 **B1**（Beta 后 backlog，**不挡 Beta**）。  
> **交叉**：[`element_identity.md`](element_identity.md) · [`box_model.md`](box_model.md) · [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md) · [`REACTIVITY.md`](REACTIVITY.md)（WP-R.8 数据钩子）

---

## 1. 定位

类浏览器 **Elements + Layout + Reactivity** 开发者工具，面向 **retained Element + taffy + Signal** 栈——不是 DOM，也不是 Solid 浏览器扩展的翻版。

| 面板 | 能力 | 阶段 |
|------|------|------|
| **Elements** | UI 树、点选高亮、盒模型只读、样式字段展示 | **P0** ☑ |
| **Elements（编辑）** | 白名单内 `Style` 字段 live 写回 | **P1** ☑ |
| **Reactivity** | `reactivity_debug_snapshot` 可视化 + 与选中节点 cross-link | **P1** ☑ |
| **Layout** | display / flex-direction / overflow / position / z-index / gap / size / `scroll_offset` | **P2** ☑ |
| **Hot reload 联动** | 选中节点展示 `template_id` 并解析 `file:line:col`；Inspector **不**重复编译链 | **P2** ☑ |
| **CSSOM 文本编辑** | 声明列表 textarea（`property: value;`）Apply 写回 retained `Style` | **P2** ☑ |

**明确不做**：选择器级联 / 样式表 / `calc()` 面板；Network / Performance / Memory；无 feature 默认打进 release。CSSOM 编辑器是声明列表，不是浏览器 CSSOM。

---

## 2. 启用方式

```toml
# Cargo.toml
rui = { version = "0.1", features = ["devtools"] }
```

```rust
use rui::app::{run, WindowOptions};

run(
    WindowOptions::new().devtools(true),
    || my_app(),
);
```

| 快捷键 | 行为 |
|--------|------|
| **F12** | 开关 Inspector |
| **Ctrl+Shift+C** | 开关点选模式（Pick） |

Gallery：`cargo run -p gallery --features devtools`

---

## 3. 公开 introspection API（`rui::app::devtools`）

只读快照，与 UI 解耦，可供测试 / 外部工具消费：

| 类型 / 函数 | 说明 |
|-------------|------|
| `InspectorNodeSnapshot` | 单节点：`path`、`focus_id`、`template_id`、`layout_bounds`、`scroll_offset`、`box_model`、`style` 摘要 |
| `BoxModelSnapshot` | content / padding / border / margin 数值（逻辑 px） |
| `StyleSnapshot` | 常用 `Style` 字段的 JSON 友好摘要 |
| `snapshot_tree(root: &Element) -> Vec<InspectorNodeSnapshot>` | 深度优先扁平列表（跳过 `__rui_devtools*` chrome） |
| `snapshot_subtree(root: &Element, path: &[usize]) -> Option<InspectorNodeSnapshot>` | 按子索引路径 |
| `pick_element_path(root, x, y) -> Option<Vec<usize>>` | 命中测路径（与 `tree_hit` 同空间） |
| `source_location_from_template_id` | 从热重载 `template_id` 解析 `file:line:col` |
| `cssom_text_from_style` / `apply_cssom_text` | 声明列表往返 |
| `reactivity_debug_snapshot()` | 复用 WP-R.8（crate 根 re-export） |

**身份键**：优先 `focus_id`；无则 `template_id`；展示用 `path` 作稳定选取键（见 [`element_identity.md`](element_identity.md)）。

---

## 4. 架构

```text
app_builder() → Element
       ↓
devtools::wrap_app(app)     ← 行布局：app_host | panel_slot（非绝对浮层）
       ↓
AppState::render → sync_overlay（世代变化时注入 Dock）
                 → layout
                 → sync_highlight_after_layout（盒模型高亮跟随）
```

- Overlay 改为 **行布局停靠**：`app_host`（flex-grow）+ `panel_slot`（打开时占用宽度）。高亮框在 app_host 内、布局后按 `layout_bounds` 跟随。  
- Chrome 状态用 `Cell`/`RefCell`，**不用** Signal。

- **P0**：高亮读 `layout_bounds`；面板只读展示盒模型与 `Style` 摘要。  
- **P1**：编辑走 `AppState::patch_devtools_element_at_path` 写 retained `Style`；Reactivity Tab 读 WP-R.8 边表。  
- **P2**：Layout Tab；`template_id` 源码定位；声明列表 Apply。热重载编译链仍由 `hot_reload_cli` / debug 路径负责。

---

## 5. 样式 live 编辑

### P1 白名单字段

- `padding` / `margin`（uniform）
- `width` / `height`
- `bg`（设计令牌名或 `#rrggbb`）
- `opacity`
- `border_width`
- `flex_grow` / `flex_shrink`

### P2 声明列表（CSSOM textarea）

另支持：`min-width` / `min-height`、`gap`、`display`、`flex-direction`、`overflow` / `overflow-x` / `overflow-y`、`position`、`z-index`、`border-color`、`class`（走 `apply_class_list`）。

**禁止**在 Inspector 内改：`template_id`、handler 回调、完整 `grid-template` 语法、选择器级联。

---

## 6. 验收（B1 DoD）

### P0

- [x] `cargo test -p rui --lib --features devtools devtools` 绿（快照 + pick 路径）
- [x] Gallery + `devtools` feature：F12 打开面板；Pick 点选后高亮与树选中一致
- [x] 盒模型数值与 `layout_bounds` + padding/border 一致（对照 [`box_model.md`](box_model.md)）

### P1

- [x] 白名单字段修改后下一帧视觉更新，无整树 `app_builder` 重跑
- [x] Reactivity Tab 展示 signal→effect / signal→memo 边
- [x] 文档：本节 + CHANGELOG + AUTHORITY B1

### P2

- [x] Layout Tab：display / flex / overflow / position / z-index / gap / size / scroll_offset
- [x] 选中节点展示 `template_id` 与解析后的 `file:line:col`
- [x] CSSOM textarea Apply 写回（声明列表，非级联）
- [x] F12 / Pick 不再 `RefCell already borrowed`

---

## 7. 禁区（与纲领一致）

- 不进默认 release / SemVer 承诺面（`devtools` feature）
- 不实现浏览器 CSSOM（级联、选择器、stylesheet）
- 不挡 Beta
