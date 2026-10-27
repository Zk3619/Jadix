//! Host operations used by independently distributed widget implementations.

pub mod hit {
    pub use crate::app::tree_hit::{
        find_paint_aabb_by_focus_id, invalidate_hover_handlers, ATTR_HOVER_REGION,
    };
}
pub mod classes {
    pub use crate::style::utility::{
        extend_class_list, patch_class_set_base_layout, with_default_class_list,
    };
}
pub mod glass {
    pub use crate::app::glass_params::parse_glass_tint;
}
pub mod collect {
    pub use crate::layout::collect_cache::invalidate_collect_paint_hash_spines;
}
pub mod scroll {
    pub use crate::app::scroll_support::{
        request_scroll_descendant_into_view_when, request_scroll_item_into_view,
        request_scroll_item_into_view_when, reset_scroll_item_into_view, ScrollAlignWhen,
        ScrollItemAlign,
    };
}
pub mod shadows {
    pub use crate::app::draw_cull::sdf_outset_shadow_pad;
}
pub mod text_edit {
    pub use crate::utils::text_edit::field;
    pub use crate::utils::text_edit::{build_text_edit_element, TextEditBuildParams};
}

pub fn caret_blink_visible() -> bool {
    crate::renderer::caret_blink_visible()
}

pub fn restart_caret_blink() {
    crate::renderer::restart_caret_blink();
}

pub fn navigation_chevron(previous: bool) -> &'static str {
    crate::chrome_nav_chevron(previous)
}
