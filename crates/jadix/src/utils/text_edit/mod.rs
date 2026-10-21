pub(crate) mod field;
mod build;
mod paint;
mod strings;

pub(crate) use build::{build_text_edit_element, TextEditBuildParams};
pub(crate) use paint::push_focused_text_edit_caret_damage;
pub(crate) use paint::sync_text_edit_paint_subtree_in_viewport;
#[cfg(test)]
pub(crate) use paint::{sync_text_edit_paint_subtree, take_input_scroll_prepare_calls};
