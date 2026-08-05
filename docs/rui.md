# RUI 开发笔记

## Grid 与 `class` / `tw` 常用 utility

布局链式 API 见 `Element::display_grid`、`grid_template_*`、`grid_cols_equal` / `grid_rows_equal`、`col_span` 等；RSX 中可用 `<Grid cols="3" rows="2">` 与子项 `col_span`。

`class` / `tw` / `uno` 会经 `rui_macros::utility_token_to_call` 映射，当前子集包括：

| 类名 | 映射 |
|------|------|
| `grid` | `display_grid()` |
| `grid-cols-N` | `display_grid()` + `grid_template_columns(layout::cols_equal_fr(N))` |
| `grid-rows-N` | `display_grid()` + `grid_template_rows(layout::cols_equal_fr(N))` |
| `gap-x-*` / `gap-y-*` | `gap_x` / `gap_y`（值为 Tailwind 式间距，如 `gap-x-2` → `8px`） |
| `self-center` / `self-start` / `self-end` / `self-stretch` / `self-baseline` / `self-auto` | `align_self_kw(...)` |

等分列辅助：`rui::layout::cols_equal_fr`（与 `grid-cols-N` 使用同一套轨函数）。

列表拖拽：`drag_reorder_handle(list_id, index)` + 信号 `drag_commit_signal_key` / `drag_hover_signal_key`，提交后用 `apply_reorder_vec` 更新 `Vec`。完整示例见 `examples/hello_world` 中的 Grid + 排序演示。

`for_keyed!` 的渲染闭包若含 `{ ... }` 花括号，不要直接塞进 `ui!` 里单层 `{ ... }` 子表达式（RSX 花括号扫描会与闭包冲突）；可改用 `reactive!(id, move || { ... for_keyed! ... })` 或提取为普通 Rust 函数。
