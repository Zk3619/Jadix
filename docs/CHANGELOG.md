# Changelog（草案）

> **地位**：SemVer / 实验面草稿（WP-2.5 任务③）。正式发布前以 [`AUTHORITY.md`](AUTHORITY.md) WP-4.1 为准重写。  
> **日期**：2026-08-13

## Unreleased

### Experimental（未进 SemVer 承诺面）

| API | 路径 | 说明 |
|-----|------|------|
| `Store` / `create_store` / `store_select` / `Store::produce` | `core/store.rs` | `produce` ≡ `update`；无 proxy 路径触发器（R-WP-3） |
| `create_resource` / `Resource` / `suspense` | `app/resource.rs` | 桌面语义；无 SSR |
| `error_boundary` / `throw_error` / `throw_resource_error` | `app/error_boundary.rs` | Resource Err→EB 须**显式** `throw_resource_error`（巩固期不自动桥） |

### Added

- WP-2.5：`throw_resource_error`；Resource `from_signal` / `set_error`；`produce` 切片契约测；Resource Err→EB e2e 测。
- WP-2.9：Builder `.border_box()`；utility `box-border` / `box-content`；命中/裁剪库测；测宽叶审计见 `docs/box_model.md` 附录 A。
- 盒模型单位：布局期真 `vw`/`vh`/`dvh`、局部 `em`、`calc(A ± B)`（含 `calc(100% - Npx)` 相对父 content box）；逻辑 `*-inline`/`*-block` 双值；`row-gap`/`column-gap`。
- B15 分边 border：`border_left` / `border_top_width` / `borderWidth="4px 8px"`（1–4 值）；Taffy 分边；均匀 SDF、非均匀四边条绘制。
- 窗口缩放 fast 布局会重烘焙 `vw`/`vh`/`em`/`calc`，视口单位随窗口实时变化。

### Notes

- 稳定面控件 API 仍见 inventory / COMPONENT_LIBRARY；本文件只标反应性异步与 Store 实验边界。
- 盒模型默认仍为 BorderBox；禁止静默改默认。见 [`box_model.md`](box_model.md)。
