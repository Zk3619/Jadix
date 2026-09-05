//! 样式子系统：
//! - [`Style`]：盒模型 + 与 Taffy 对齐的字段 + 矩形绘制（背景/边框/阴影）；
//! - [`TextStyle`] / [`TextDecorationStyle`]：文本排版与装饰线（见 [`text_style`] 模块）；
//! - 语义色解析；
//! - CSS [`keyframes`] 曲线 + [`animation`] 运行时（transition / `@keyframes` tick）。
//!
//! Element 侧 **transition 配方**（`MotionStyle` / `prepare_motion_sync`）在
//! [`crate::utils::motion`]；分层决策见仓库 `docs/engine/animation_and_motion.md`。
//!
//! 布局遍历见 [`crate::layout::layout_tree`]。

mod animatable;
mod animation;
mod clip_path;
mod color;
mod color_space;
mod cursor;
pub(crate) mod cursor_image;
mod easing;
mod element_style;
pub mod filter;
mod focus_ring;
pub(crate) mod gradient;
pub mod keyframes;
mod keyframes_parse;
pub(crate) mod offset_path;
mod scroll_timeline;
mod semantic_tone;
mod specified;
pub mod svg_filter;
mod tailwind_leading;
mod text_style;
mod theme;
pub mod timeline;
mod timing;
pub mod transform;
mod used_layout;
pub(crate) mod utility;
mod variant_override;
mod waapi;
mod writing_mode;

pub use animatable::{
    AnimatableValue, LayoutImpact, PropertyId, PropertySet, parse_background_position,
    parse_fr_tracks,
};
pub use animation::{
    AnimationBaseline, AnimationLock, AnimationRunState, AnimationSnapshot, AnimationTick,
    CubicTween, CubicTweenState, TimelineProgress, TransitionSnapshot,
    advance_css_transition_timeline_ms, animation_run_active, any_ui_animation_active,
    assign_transition_specified, copy_transition_used_values, css_animation_needs_frame,
    css_transition_needs_frame, cubic_bezier_y_at_x, keyframes_anim_entry, prefers_reduced_motion,
    set_css_transition_synthetic_clock, set_css_transition_timeline_ms,
    set_css_transition_virtual_timeline, set_prefers_reduced_motion,
    snap_element_transitions_to_current, tick_animations, tick_element_animations,
    tick_element_animations_ex, tick_element_animations_ex_named,
    tick_element_animations_ex_progress, tick_element_transitions, tick_element_transitions_locked,
    tick_element_transitions_with_text, tick_element_transitions_with_text_used,
    use_ui_transition_cubic,
};
pub use clip_path::{
    ClipCoord, ClipFillRule, ClipPath, GPU_CLIP_MAX_PTS, GpuClipPoly, begin_named_clip_pass,
    clamp_border_radii, lookup_clip_path, parse_clip_path, radii_affect_clip, register_clip_path,
    rounded_rect_contains, tessellate_rounded_rect,
};
pub use color::{
    color_mix_over_background, parse_color, parse_text_color, try_parse_color, try_parse_text_color,
};
pub use color_space::{
    PaintChrome, color_mix_over_surface, display_srgba_to_linear_srgba,
    encode_linear_srgb_component, encode_linear_srgb_to_display_srgb, lerp_display_srgb_linear,
    lerp_paint_chrome, lerp_premultiplied_display_srgba, lerp_rgba_u8, lerp_text_rgba,
    linearize_display_srgb, linearize_display_srgb_component, mix_display_srgb_in_oklab,
    oklch_to_display_srgba,
};
pub use cursor::{CursorKind, CursorUrl, CursorValue};
pub(crate) use cursor_image::CustomCursorCache;
pub use cursor_image::register_cursor_rgba8;
pub use easing::{split_comma_list, split_ws_tokens};
pub use element_style::{
    AnimationComposition, AnimationDirection, AnimationEntry, AnimationFillMode,
    AnimationIterationCount, AnimationPlayState, AnimationTimeline, BorderImageRepeat, BoxShadow,
    CornerShape, InterpolateSize, LineStyle, StepJumpTerm, Style, TimingFunction, TransitionEntry,
    TransitionProperty, UnitSlot, WillChangeHint, parse_box_shadow, parse_box_shadows,
};
pub use filter::{
    FilterFunction, FilterList, filter_list_paint_outset, filter_list_to_css,
    interpolate_filter_lists, parse_filter,
};
pub use focus_ring::{
    FOCUS_RING_BLUR, FOCUS_RING_HARD_SPREAD, FOCUS_RING_SOFT_BLUR, apply_focus_ring_shadow,
    apply_focus_ring_shadow_color, control_focus_ring_active, focus_ring_border_color,
    focus_ring_color, focus_ring_destructive_color, focus_ring_params, focus_ring_shadow_matches,
    focus_ring_shadow_matches_color, sync_focus_ring_subtree,
};
pub use gradient::{
    ColorHint, ColorStop, ConicGradient, Gradient, GradientPosition, LinearGradient, RadialExtent,
    RadialGradient, RadialShape, RadialSize, ensure_gradient_texture, flush_gradient_textures,
    gradient_raster_dpi, gradient_texture_radii, gradient_texture_size, parse_gradient,
    rasterize_gradient, sample_gradient, sample_gradient_in_box, set_gradient_raster_dpi,
};
pub use keyframes::{
    KeyframeBase, KeyframeCoverage, KeyframeSample, KeyframeStop, keyframe_coverage,
    keyframes_exist, lookup_keyframes, register_keyframes, sample_keyframes,
    sample_keyframes_eased,
};
pub use keyframes_parse::parse_and_register_keyframes;
pub use offset_path::{OffsetPath, parse_offset_path};
pub use scroll_timeline::{
    ScrollTimelineAxis, lookup_named, map_animation_range, normalize_timeline_name,
    parse_range_progress, push_named_timelines, scroller_progress_axis, view_timeline_progress,
};
pub use semantic_tone::{
    MODAL_OVERLAY_OPACITY_NEUTRAL, SEMANTIC_MEDIA_WASH_ALPHA, SemanticBorder, SemanticChrome,
    SemanticChromeDecor, SemanticChromeOpts, SemanticPalette, SemanticShadow, SemanticSurface,
    SemanticTone, SoftSemanticMix, apply_semantic_border, apply_semantic_shadow,
    button_soft_semantic_chrome, dim_semantic_shadow, muted_from_accent, semantic_chrome,
    semantic_media_wash, semantic_modal_overlay_scrim, semantic_palette, semantic_shadow,
    soft_semantic_fill, soft_semantic_fill_hover, soft_semantic_mix_alphas,
};
pub use specified::SpecifiedPresent;
pub use svg_filter::{
    FilterGraph, FilterParseError, lookup_svg_filter, register_svg_filter,
    register_svg_filter_graph, resolve_filter_list_to_graph, resolve_svg_filter_url,
};
pub use tailwind_leading::{
    TAILWIND_TEXT_2XL_LEADING_SCALE, TAILWIND_TEXT_3XL_LEADING_SCALE,
    TAILWIND_TEXT_4XL_LEADING_SCALE, TAILWIND_TEXT_BASE_LEADING_SCALE,
    TAILWIND_TEXT_LG_LEADING_SCALE, TAILWIND_TEXT_SM_LEADING_SCALE, TAILWIND_TEXT_XL_LEADING_SCALE,
    TAILWIND_TEXT_XS_LEADING_SCALE, tailwind_default_leading_scale,
};
pub use text_style::{
    InputCaretDraw, TextAlign, TextAlignDeclaration, TextDecorationStyle, TextDirection,
    TextOverflow, TextSpan, TextSpanKind, TextStyle, WhiteSpace, parse_text_shadow,
};
pub use theme::{
    DesignTokens, FocusRingStyle, Theme, ThemeMode, focus_ring_style, get_theme, is_dark_theme,
    set_focus_ring_style, set_theme, set_theme_mode, theme_mode, toggle_theme_mode, with_theme,
};
pub use timeline::{
    Repeat, Timeline, TimelineInner, any_timeline_playing, cubic_from_css, global_time_s, keyframe,
    lerp, tick_timelines,
};
pub use timing::{Timing, TimingPhase, TimingProgress, spring_step};
pub use transform::{
    MAT2D_IDENTITY, Mat2d, TransformBox, TransformFunction, TransformList, TransformOrigin,
    TransformOriginX, TransformOriginY, TransformStyle, compute_combined_transform, mat2d_inverse,
    mat2d_is_axis_aligned, mat2d_is_identity, mat2d_linear, mat2d_multiply, mat2d_rotate,
    mat2d_transform_point, mat2d_transform_vec, origin_frac, parse_transform, parse_transform_box,
    parse_transform_origin, parse_transform_style, resolve_transform_matrix, set_origin_frac,
};
pub use used_layout::{UsedLayout, ensure_used_layout_from_bounds, stamp_element_used_layout};
pub use utility::{
    ClassPart, ClassSet, VariantContext, apply_class_list, apply_utility_classes,
    class_variant_hover_needs_frame, classes, push_class_part, reapply_class_color_overrides,
    resolve_class_variants, sync_class_variants_subtree, tick_active_css_transitions_subtree,
    with_class_list,
};
pub use variant_override::{
    ATTR_OVERRIDE_BG, ATTR_OVERRIDE_BORDER, ATTR_OVERRIDE_FG, ATTR_OVERRIDE_LEADING,
    ATTR_OVERRIDE_PAD, ATTR_OVERRIDE_RADIUS, ATTR_OVERRIDE_SHADOW, ATTR_OVERRIDE_SIZE,
    ATTR_OVERRIDE_WEIGHT, ATTR_VARIANT_CHROME, VariantStyleField,
    apply_semantic_chrome_respecting_overrides, apply_user_bg, apply_user_border_color,
    apply_user_fg, is_variant_style_overridden, mark_variant_style_override,
    set_variant_chrome_enabled, variant_chrome_enabled,
};
pub use waapi::{Animation, ComputedTiming, Keyframe, KeyframeEffect, get_animations};
pub use writing_mode::{PhysicalSide, WritingMode};
