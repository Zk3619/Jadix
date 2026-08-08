//! Input / Textarea 共享 chrome（**私有**，不进 prelude / 公开 API）。
//!
//! 产品面仍是 [`super::input`] 与 [`super::textarea`]；编辑能力在 [`crate::utils::text_field`]。

mod build;
mod paint;

pub(crate) use build::{build_text_edit_element, TextEditBuildParams};
pub(crate) use paint::sync_text_edit_paint_subtree;

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
    }
}
