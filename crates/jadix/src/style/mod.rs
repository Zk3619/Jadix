//! 样式子系统：
//! - [`Style`]：盒模型 + 与 Taffy 对齐的字段 + 矩形绘制（背景/边框/阴影）；
//! - [`TextStyle`] / [`TextDecorationStyle`]：文本排版与装饰线（见 [`text_style`] 模块）；
//! - 语义色解析；
//! - CSS [`keyframes`] 曲线 + [`animation`] 运行时（transition / `@keyframes` tick）；
//! - [`generator`]：闭式 spring / inertia / 标量 [`animate`]（对照 Motion 生成器内核）。
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
mod generator;
mod glass;
pub(crate) mod liquid_flow;
pub(crate) mod gradient;
mod into_value;
pub mod keyframes;
mod keyframes_parse;
pub(crate) mod offset_path;
mod scroll_timeline;
mod semantic_tone;
mod specified;
pub mod stylesheet;
pub mod svg_filter;
mod tailwind_leading;
mod text_style;
mod theme;
mod theme_override;
mod theme_style;
pub mod timeline;
mod timing;
pub mod transform;
mod used_layout;
pub(crate) mod utility;
mod variant_override;
mod waapi;
mod writing_mode;

pub use animatable::{
    parse_background_position, parse_fr_tracks, AnimatableValue, LayoutImpact, PropertyId,
    PropertySet,
};
#[cfg(feature = "animation-ex")]
pub(crate) use animatable::{
    read_value as read_animatable_value, values_close as animatable_values_close,
    write_value as write_animatable_value,
};
pub(crate) use animation::css_timeline_now_ms;
pub(crate) use animation::note_page_css_needs_frame;
pub use animation::{
    advance_css_transition_timeline_ms, animation_run_active, any_page_css_needs_frame,
    any_ui_animation_active, assign_transition_specified,
    commit_style_transitions_after_specified_write, copy_transition_used_values,
    css_animation_needs_frame, css_transition_needs_frame, cubic_bezier_y_at_x,
    keyframes_anim_entry, prefers_reduced_motion, set_css_transition_max_step_ms,
    set_css_transition_synthetic_clock, set_css_transition_timeline_ms,
    set_css_transition_virtual_timeline, set_prefers_reduced_motion,
    snap_element_transitions_to_current, tick_animations, tick_element_animations,
    tick_element_animations_ex, tick_element_animations_ex_named,
    tick_element_animations_ex_progress, tick_element_transitions, tick_element_transitions_locked,
    tick_element_transitions_with_text, tick_element_transitions_with_text_used,
    use_ui_transition_cubic, AnimationBaseline, AnimationLock, AnimationRunState,
    AnimationSnapshot, AnimationTick, CubicTween, CubicTweenState, TimelineProgress,
    TransitionSnapshot,
};
pub use clip_path::{
    begin_named_clip_pass, clamp_border_radii, lookup_clip_path, parse_clip_path,
    radii_affect_clip, register_clip_path, rounded_rect_contains, tessellate_rounded_rect,
    ClipCoord, ClipFillRule, ClipPath, GpuClipPoly, GPU_CLIP_MAX_PTS,
};
pub use color::{
    color_mix_over_background, parse_color, parse_text_color, try_parse_color,
    try_parse_text_color, TRANSPARENT,
};
pub use color_space::{
    color_mix_over_surface, display_srgba_to_linear_srgba, encode_linear_srgb_component,
    encode_linear_srgb_to_display_srgb, lerp_display_srgb_linear, lerp_paint_chrome,
    lerp_premultiplied_display_srgba, lerp_rgba_u8, lerp_text_rgba, linearize_display_srgb,
    linearize_display_srgb_component, mix_display_srgb_in_oklab, oklch_to_display_srgba,
    PaintChrome,
};
pub use cursor::{CursorKind, CursorUrl, CursorValue};
pub use cursor_image::register_cursor_rgba8;
pub(crate) use cursor_image::CustomCursorCache;
pub use easing::{split_comma_list, split_ws_tokens};
pub use element_style::{
    parse_box_shadow, parse_box_shadows, AnimationComposition, AnimationDirection, AnimationEntry,
    AnimationFillMode, AnimationIterationCount, AnimationPlayState, AnimationTimeline,
    BorderImageRepeat, BoxShadow, CornerShape, InterpolateSize, LineStyle, StepJumpTerm, Style,
    TimingFunction, TransitionEntry, TransitionProperty, UnitSlot, WillChangeHint,
};
pub use filter::{
    filter_list_paint_outset, filter_list_to_css, interpolate_filter_lists, parse_filter,
    FilterFunction, FilterList,
};
pub use focus_ring::{
    apply_focus_ring_shadow, apply_focus_ring_shadow_color, control_focus_ring_active,
    focus_ring_border_color, focus_ring_color, focus_ring_destructive_color, focus_ring_params,
    focus_ring_params_for, focus_ring_params_for_style, focus_ring_resolved_color,
    focus_ring_shadow_matches, focus_ring_shadow_matches_color, sync_focus_ring_subtree,
    FOCUS_RING_BLUR, FOCUS_RING_HARD_SPREAD, FOCUS_RING_SOFT_BLUR,
};
pub use generator::{
    animate, any_value_generator_active, find_spring, follow, stagger_ms, tick_value_generators,
    use_animated, AnimatedNumber, GeneratorSample, Inertia, InertiaOptions, RepeatMode, Spring,
    SpringOptions, SpringPhysics, StaggerFrom, TweenOptions, ValueDriver,
};
pub use gradient::{
    ensure_gradient_texture, flush_gradient_textures, gradient_raster_dpi, gradient_texture_radii,
    gradient_texture_size, parse_gradient, rasterize_gradient, sample_gradient,
    sample_gradient_in_box, set_gradient_raster_dpi, ColorHint, ColorStop, ConicGradient, Gradient,
    GradientPosition, LinearGradient, RadialExtent, RadialGradient, RadialShape, RadialSize,
};
pub use into_value::{
    overflow_as_str, parse_align_items, parse_flex_wrap, parse_justify_content,
    parse_overflow_keyword, IntoAlignItems, IntoCursor, IntoFlexWrap, IntoJustifyContent,
    IntoOverflow, IntoPaintColor, IntoTextColor,
};
pub use keyframes::{
    keyframe_coverage, keyframes_exist, lookup_keyframes, register_keyframes, sample_keyframes,
    sample_keyframes_eased, KeyframeBase, KeyframeCoverage, KeyframeSample, KeyframeStop,
};
pub use keyframes_parse::parse_and_register_keyframes;
pub use offset_path::{parse_offset_path, OffsetPath};
pub use scroll_timeline::{
    lookup_named, map_animation_range, normalize_timeline_name, parse_range_progress,
    push_named_timelines, scroller_progress_axis, view_timeline_progress, ScrollTimelineAxis,
};
pub use semantic_tone::{
    apply_semantic_border, apply_semantic_shadow, button_soft_semantic_chrome, dim_semantic_shadow,
    muted_from_accent, semantic_chrome, semantic_media_wash, semantic_modal_overlay_scrim,
    semantic_palette, semantic_shadow, soft_semantic_fill, soft_semantic_fill_hover,
    soft_semantic_mix_alphas, SemanticBorder, SemanticChrome, SemanticChromeDecor,
    SemanticChromeOpts, SemanticPalette, SemanticShadow, SemanticSurface, SemanticTone,
    SoftSemanticMix, MODAL_OVERLAY_OPACITY_NEUTRAL, SEMANTIC_MEDIA_WASH_ALPHA,
};
pub use specified::SpecifiedPresent;
pub use stylesheet::{
    apply_style_file_batch, apply_stylesheet_rules, apply_stylesheet_rules_tree, clear_stylesheets,
    collect_matching_rules, load_stylesheet, parse_stylesheet, register_stylesheet,
    reload_stylesheets, watch_stylesheets, MatchedStyleRule, Selector, StyleRule, Stylesheet,
    StylesheetError,
};
pub(crate) use stylesheet::{flush_style_rebuild, track_style_changes};
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
pub(crate) use theme::{take_system_theme_refresh, theme_generation, update_system_theme};
pub use glass::GlassMaterial;
pub use theme_style::{set_theme_preset, theme_preset, ThemePreset, ThemeStyles};
pub use theme::{
    focus_ring_style, focus_ring_width, get_theme, is_dark_theme, set_focus_ring_style,
    set_focus_ring_width, set_theme, set_theme_mode, set_theme_preference, theme_mode,
    theme_preference, toggle_theme_mode, with_theme,
    DesignTokens, FocusRingStyle, Theme, ThemeMode, ThemePreference, ThemeTransitionConfig,
    DEFAULT_FOCUS_RING_WIDTH,
};
pub use theme_override::{
    apply_theme_patch, clear_theme_overrides, compose_theme, install_theme_overrides,
    load_and_install_theme_overrides, load_theme_overrides, parse_theme_overrides_str,
    set_theme_transition_config, theme_transition_config, theme_overrides, DesignTokensPatch, ThemeOverrideError,
    ThemeOverrides,
};
pub(crate) use theme_override::{restore_theme_overrides, snapshot_theme_overrides};
#[cfg(all(test, target_os = "windows"))]
pub(crate) use timeline::set_global_time_for_test;
pub use timeline::{
    any_timeline_playing, cubic_from_css, global_time_s, keyframe, lerp, tick_timelines, Repeat,
    Timeline, TimelineInner,
};
pub use timing::{spring_step, Timing, TimingPhase, TimingProgress};
pub use transform::{
    aabb_of_centered_transform, compute_combined_transform, mat2d_center_pivot_origin_tx_ty,
    mat2d_inverse, mat2d_is_axis_aligned, mat2d_is_identity, mat2d_linear, mat2d_multiply,
    mat2d_rotate, mat2d_set_translation, mat2d_transform_point, mat2d_transform_vec,
    mat2d_world_center_pivot, mat2d_world_center_pivot_with_shift, origin_frac, origin_px,
    parse_transform, parse_transform_box, parse_transform_origin, parse_transform_style,
    quad_of_centered_transform, resolve_transform_matrix, rotate_and_list_mat2d,
    scale_box_about_origin, set_origin_frac, Mat2d, TransformBox, TransformFunction, TransformList,
    TransformOrigin, TransformOriginX, TransformOriginY, TransformStyle, MAT2D_IDENTITY,
};
pub use used_layout::{ensure_used_layout_from_bounds, stamp_element_used_layout, UsedLayout};
pub use utility::{
    apply_class_list, apply_utility_classes, class_variant_color_overlay,
    class_variant_hover_needs_frame, classes, ensure_class_list, extend_class_list,
    patch_class_set_base_display, patch_class_set_base_motion, patch_class_set_base_paint,
    patch_class_set_base_radii, patch_class_set_base_shadow, push_class_part,
    reapply_class_color_overrides, resolve_class_variants, sync_class_variants_subtree,
    tick_active_css_transitions_subtree, with_class_list, ClassPart, ClassSet, VariantContext,
    ATTR_SYNC_LABEL_FG,
};
pub use variant_override::{
    apply_semantic_chrome_respecting_overrides, apply_user_bg, apply_user_border_color,
    apply_user_fg, is_variant_style_overridden, mark_variant_style_override,
    set_variant_chrome_enabled, variant_chrome_enabled, VariantStyleField, ATTR_OVERRIDE_BG,
    ATTR_OVERRIDE_BORDER, ATTR_OVERRIDE_FG, ATTR_OVERRIDE_LEADING, ATTR_OVERRIDE_PAD,
    ATTR_OVERRIDE_RADIUS, ATTR_OVERRIDE_SHADOW, ATTR_OVERRIDE_SIZE, ATTR_OVERRIDE_WEIGHT,
    ATTR_VARIANT_CHROME,
};
pub use waapi::{get_animations, Animation, ComputedTiming, Keyframe, KeyframeEffect};
pub use writing_mode::{PhysicalSide, WritingMode};

pub(crate) mod button_chrome;
