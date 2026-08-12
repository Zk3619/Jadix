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

### Notes

- 稳定面控件 API 仍见 inventory / COMPONENT_LIBRARY；本文件只标反应性异步与 Store 实验边界。
