//! 样式子系统：
//! - [`Style`]：盒模型 + 与 Taffy 对齐的字段 + 矩形绘制（背景/边框/阴影）；
//! - [`TextStyle`] / [`TextDecorationStyle`]：文本排版与装饰线（见 [`text_style`] 模块）；
//! - 语义色解析。
//!
//! 布局遍历见 [`crate::layout::layout_tree`]。

mod animation;
mod color;
mod color_space;
mod cursor;
pub(crate) mod cursor_image;
mod element_style;
mod focus_ring;
mod tailwind_leading;
mod text_style;
mod theme;
pub mod timeline;
pub mod transform;
pub(crate) mod utility;

pub use animation::{
    any_ui_animation_active, cubic_bezier_y_at_x, tick_animations, use_scrollbar_visibility_spring,
    use_spring, use_ui_transition_cubic, use_ui_transition_spring, AnimationTick, CubicTween,
    CubicTweenState, Spring, SpringState, TransitionSnapshot,
};
pub use color::{parse_color, parse_text_color, try_parse_color, try_parse_text_color};
pub use color_space::{
    display_srgba_to_linear_srgba, encode_linear_srgb_component,
    encode_linear_srgb_to_display_srgb, lerp_display_srgb_linear, lerp_premultiplied_display_srgba,
    linearize_display_srgb, linearize_display_srgb_component, mix_display_srgb_in_oklab,
    oklch_to_display_srgba,
};
pub use cursor::{CursorKind, CursorUrl, CursorValue};
pub use cursor_image::register_cursor_rgba8;
pub(crate) use cursor_image::CustomCursorCache;
pub use element_style::{
    parse_box_shadow, parse_box_shadows, BorderImageRepeat, BoxShadow, CornerShape, LineStyle,
    LinearGradient, Style, TimingFunction, TransitionEntry, TransitionProperty, UnitSlot,
};
pub use focus_ring::{
    apply_focus_ring_shadow, apply_focus_ring_shadow_color, control_focus_ring_active,
    focus_ring_border_color, focus_ring_color, focus_ring_destructive_color, focus_ring_params,
    focus_ring_shadow_matches, focus_ring_shadow_matches_color, sync_focus_ring_subtree,
    FOCUS_RING_BLUR, FOCUS_RING_HARD_SPREAD, FOCUS_RING_SOFT_BLUR,
};
pub use tailwind_leading::{
    tailwind_default_leading_scale, TAILWIND_TEXT_2XL_LEADING_SCALE,
    TAILWIND_TEXT_3XL_LEADING_SCALE, TAILWIND_TEXT_4XL_LEADING_SCALE,
    TAILWIND_TEXT_BASE_LEADING_SCALE, TAILWIND_TEXT_LG_LEADING_SCALE,
    TAILWIND_TEXT_SM_LEADING_SCALE, TAILWIND_TEXT_XL_LEADING_SCALE, TAILWIND_TEXT_XS_LEADING_SCALE,
};
pub use text_style::{
    parse_text_shadow, InputCaretDraw, TextDecorationStyle, TextDirection, TextOverflow, TextSpan,
    TextSpanKind, TextStyle, WhiteSpace,
};
pub use theme::{
    focus_ring_style, get_theme, is_dark_theme, set_focus_ring_style, set_theme, set_theme_mode,
    theme_mode, toggle_theme_mode, with_theme, DesignTokens, FocusRingStyle, Theme, ThemeMode,
};
pub use timeline::{
    any_timeline_playing, cubic_from_css, global_time_s, keyframe, lerp, tick_timelines, Repeat,
    Timeline, TimelineInner,
};
pub use transform::{
    compute_combined_transform, mat2d_inverse, mat2d_is_identity, mat2d_transform_point,
    parse_transform, parse_transform_box, parse_transform_origin, parse_transform_style,
    resolve_transform_matrix, Mat2d, TransformBox, TransformFunction, TransformList,
    TransformOrigin, TransformOriginX, TransformOriginY, TransformStyle, MAT2D_IDENTITY,
};
pub use utility::{
    apply_class_list, apply_utility_classes, resolve_class_variants, sync_class_variants_subtree,
    with_class_list, ClassSet, VariantContext,
};
