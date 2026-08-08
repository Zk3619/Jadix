//! Input / Textarea 共享 chrome（**crate 私有**，不在 `components/`，不进公开 API）。
//!
//! 产品面：[`crate::components::input`] / [`crate::components::textarea`]；
//! 编辑能力：[`crate::utils::text_field`]。

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
