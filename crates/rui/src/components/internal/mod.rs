//! Component-library private chrome (not product API, not framework).
//!
//! Shared implementation for product components. Future crate split: move with
//! `components/` into the component-library crate.
//!
//! Dialog 模态壳已并入 [`crate::components::dialog`]；此处保留 Input/Textarea 共享 chrome、
//! 菜单 Checkbox/Radio 控制面、嵌套菜单子树，以及 Toast / Carousel 交互引擎（WP-SI）。

pub(crate) mod carousel_autoplay;
pub(crate) mod carousel_loop;
pub(crate) mod carousel_snap;
pub(crate) mod carousel_track;
pub(crate) mod menu_checkable;
pub(crate) mod menu_item_chrome;
pub(crate) mod menu_subtree;
pub(crate) mod text_edit;
pub(crate) mod toast_queue;
pub(crate) mod toast_swipe;
pub(crate) mod toast_timer;
