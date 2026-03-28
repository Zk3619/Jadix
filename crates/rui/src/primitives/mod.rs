//! 原子原语层：声明式 UI 的底座。
//!
//! - **[`r#box`] / [`hstack`] / [`vstack`]**（`ui!` 中 `<Box>`、`<Rect>`、`<HStack>`、`<VStack>`）：布局与视觉容器，对应 [`BoxBuilder`]。
//! - **[`interactive`]**（`ui!` 中 `<Interactive>`）：在已有 [`Element`] 上挂载点击、悬停、键盘与焦点，对应 [`InteractiveBuilder`]。
//! - **[`text`] / [`icon`]**：纯文本与图标叶子（`ui!` 中 `<Text>`）。
//! - **[`visible_item_range`]**：虚拟列表用可见行区间（与 `ScrollArea` 滚动信号配合）。
//!
//! 复合组件（Button、Input 等）在内部应优先组合上述原语，而不是重复实现布局或输入路由。

pub mod r#box;
pub mod interactive;
pub mod popup;
pub mod tabs;
pub mod text;
pub mod toggle;
pub mod virtual_list;

pub use interactive::{interactive, InteractiveBuilder};
pub use popup::{
    panel_block_clicks, register_last_popup_for_escape, try_close_top_popup, underlay_close_on_click,
    PopupPlacement,
};
pub use r#box::{hstack, r#box, vstack, BoxBuilder};
pub use tabs::{tabs_headless, TabsHeadlessBuilder, TABSTRIP_FOCUS_PREFIX};
pub use text::{icon, text, TextBuilder};
pub use toggle::{toggle, ToggleBuilder};
pub use virtual_list::visible_item_range;
