//! Component-library private chrome (not product API, not framework).
//!
//! Shared implementation for product components. Future crate split: move with
//! `components/` into the component-library crate.
//!
//! Dialog 模态壳已并入 [`crate::components::dialog`]；此处保留 Input/Textarea 共享 chrome
//! 与菜单 Checkbox/Radio 控制面。

pub(crate) mod menu_checkable;
pub(crate) mod text_edit;
