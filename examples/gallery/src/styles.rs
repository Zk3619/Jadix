//! Rust 模块模式：无样式表加载器时也用 const class（与 `styles.toml` 不冲突）。

/// 气泡内容默认 utility。加载器路径写同一串到 `[data-slot=bubble-content]`。
pub const BUBBLE_CONTENT: &str = "rounded-2xl px-2 sm:px-3 py-2 text-sm dark:ring-1";
