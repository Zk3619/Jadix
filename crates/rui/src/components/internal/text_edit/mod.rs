//! Input / Textarea 共享 chrome（[`crate::components::internal`]，不进公开 API / prelude）。
//!
//! 产品面：[`crate::components::input`] / [`crate::components::textarea`]；
//! Editing capabilities: [`crate::utils::text_field`].

mod build;
mod paint;
mod strings;

pub(crate) use build::{build_text_edit_element, TextEditBuildParams};
pub(crate) use paint::sync_text_edit_paint_subtree_in_viewport;
#[cfg(test)]
pub(crate) use paint::{sync_text_edit_paint_subtree, take_input_scroll_prepare_calls};
