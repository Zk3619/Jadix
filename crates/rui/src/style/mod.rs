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
pub mod filter;
mod focus_ring;
pub(crate) mod gradient;
pub mod keyframes;
pub mod svg_filter;
mod tailwind_leading;
mod text_style;
mod theme;
pub mod timeline;
pub mod transform;
pub(crate) mod utility;

pub use animation::{
    advance_css_transition_timeline_ms, any_ui_animation_active, css_animation_needs_frame,
    css_transition_needs_frame, cubic_bezier_y_at_x, set_css_transition_synthetic_clock,
    set_css_transition_timeline_ms, set_css_transition_virtual_timeline,
    snap_element_transitions_to_current, tick_animations, tick_element_animations,
    tick_element_transitions, tick_element_transitions_locked, use_ui_transition_cubic,
    AnimationBaseline, AnimationLock, AnimationRunState, AnimationSnapshot, AnimationTick,
    CubicTween, CubicTweenState, TransitionSnapshot,
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
    parse_box_shadow, parse_box_shadows, AnimationComposition, AnimationDirection, AnimationEntry,
    AnimationFillMode, AnimationIterationCount, AnimationPlayState, AnimationTimeline,
    BorderImageRepeat, BoxShadow, CornerShape, LineStyle, Style, TimingFunction, TransitionEntry,
    TransitionProperty, UnitSlot,
};
pub use keyframes::{
    keyframe_coverage, keyframes_exist, lookup_keyframes, register_keyframes, sample_keyframes,
    sample_keyframes_eased, KeyframeBase, KeyframeCoverage, KeyframeSample, KeyframeStop,
};
pub use filter::{
    filter_list_to_css, interpolate_filter_lists, parse_filter, FilterFunction, FilterList,
};
pub use focus_ring::{
    apply_focus_ring_shadow, apply_focus_ring_shadow_color, control_focus_ring_active,
    focus_ring_border_color, focus_ring_color, focus_ring_destructive_color, focus_ring_params,
    focus_ring_shadow_matches, focus_ring_shadow_matches_color, sync_focus_ring_subtree,
    FOCUS_RING_BLUR, FOCUS_RING_HARD_SPREAD, FOCUS_RING_SOFT_BLUR,
};
pub use gradient::{
    ensure_gradient_texture, flush_gradient_textures, gradient_texture_radii,
    gradient_texture_size, parse_gradient, rasterize_gradient, sample_gradient,
    sample_gradient_in_box, set_gradient_raster_dpi, ColorHint, ColorStop, ConicGradient,
    Gradient, GradientPosition, LinearGradient, RadialExtent, RadialGradient, RadialShape,
    RadialSize,
};
pub use svg_filter::{
    lookup_svg_filter, register_svg_filter, register_svg_filter_graph,
    resolve_filter_list_to_graph, resolve_svg_filter_url, FilterGraph, FilterParseError,
};
pub use tailwind_leading::{
    tailwind_default_leading_scale, TAILWIND_TEXT_2XL_LEADING_SCALE,
    TAILWIND_TEXT_3XL_LEADING_SCALE, TAILWIND_TEXT_4XL_LEADING_SCALE,
    TAILWIND_TEXT_BASE_LEADING_SCALE, TAILWIND_TEXT_LG_LEADING_SCALE,
    TAILWIND_TEXT_SM_LEADING_SCALE, TAILWIND_TEXT_XL_LEADING_SCALE, TAILWIND_TEXT_XS_LEADING_SCALE,
};
pub use text_style::{
    parse_text_shadow, InputCaretDraw, TextAlign, TextAlignDeclaration, TextDecorationStyle,
    TextDirection, TextOverflow, TextSpan, TextSpanKind, TextStyle, WhiteSpace,
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
    compute_combined_transform, mat2d_inverse, mat2d_is_axis_aligned, mat2d_is_identity,
    mat2d_linear, mat2d_multiply, mat2d_transform_point, mat2d_transform_vec, parse_transform,
    parse_transform_box, parse_transform_origin, parse_transform_style, resolve_transform_matrix,
    Mat2d, TransformBox, TransformFunction, TransformList, TransformOrigin, TransformOriginX,
    TransformOriginY, TransformStyle, MAT2D_IDENTITY,
};
pub use utility::{
    apply_class_list, apply_utility_classes, class_variant_hover_needs_frame,
    resolve_class_variants, sync_class_variants_subtree, tick_active_css_transitions_subtree,
    with_class_list, ClassSet, VariantContext,
};
