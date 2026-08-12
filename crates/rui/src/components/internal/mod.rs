//! Component-library private chrome (not product API, not framework).
//!
//! Shared implementation for product components. Future crate split: move with
//! `components/` into the component-library crate.
//!
//! Dialog 模态壳已并入 [`crate::components::dialog`]；此处仅保留 Input/Textarea 共享 chrome。

pub(crate) mod text_edit;
