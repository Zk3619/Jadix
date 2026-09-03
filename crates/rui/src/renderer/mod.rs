//! GPU 与文本渲染：管线、实例数据、glyphon 封装及每帧文本准备。

pub mod backdrop_blur;
pub mod cpu_text;
pub mod css_filter;
pub mod effect;
pub mod effect_pass;
pub mod emoji_config;
pub mod emoji_detect;
pub mod emoji_layout;
pub mod emoji_matrix;
#[cfg(test)]
mod emoji_shape_diag;
pub mod emoji_svg;
mod extended_pictographic;
pub mod font_loader;
pub mod geometry2d;
pub mod glyph_fx;
pub mod graph;
pub mod offscreen;
pub mod pipeline;
pub mod prepare_text;
pub mod present_retain;
pub mod resources;
pub mod text;
pub mod text_hit;
pub(crate) mod text_prepare_debug;
pub mod text_xform;
pub mod textured_pipeline;
pub mod twemoji_name;
pub(crate) mod wgpu_compat;

#[cfg(feature = "scene3d")]
pub mod scene3d;

pub use backdrop_blur::BackdropBlurPipeline;
pub use css_filter::{
    queue_css_filter_wrap, take_filter_jobs, CssFilterPassGpu, FilterJob, FilterUniforms,
};
pub use effect::{
    enrich_render_graph, queue_capture, queue_effect, queue_from_attrs as queue_effect_from_attrs,
    queue_subtree_wrap, take_capture_jobs, take_effect_jobs, CaptureJob, EffectJob, EffectKind,
    EffectUniforms, ATTR_EFFECT, ATTR_EFFECT_CAPTURE_CHILDREN, ATTR_EFFECT_STRENGTH,
};
pub use effect_pass::EffectPassGpu;
pub use emoji_config::{configured_twemoji_dir, emoji_bitmap_override_active};
pub use geometry2d::{gpu2d_enabled, path_to_instances, quads_to_instances};
pub use glyph_fx::{
    format_rgba8, mix_rgba, shimmer_band_mix, shimmer_box_highlight_rgba, shimmer_box_mix_at,
    GlyphFx, GlyphFxKind, ShimmerParams, ATTR_GLYPH_FX, ATTR_SHIMMER_ANGLE_DEG, ATTR_SHIMMER_BOX,
    ATTR_SHIMMER_COLOR, ATTR_SHIMMER_COUNT, ATTR_SHIMMER_DURATION_MS, ATTR_SHIMMER_EPOCH,
    ATTR_SHIMMER_ONCE, ATTR_SHIMMER_REVERSE, ATTR_SHIMMER_SPREAD_PX, SHIMMER_INHERIT_ATTRS,
};
pub use graph::{
    env_dump_enabled as render_graph_dump_enabled, store_last_dump as store_render_graph_dump,
    PassKind, RenderGraph, RenderPassNode,
};
pub use pipeline::{Globals, InstanceInput, RuiPipeline, VertexInput};
pub use prepare_text::prepare_text_frame;
pub(crate) use prepare_text::{caret_blink_next_toggle_at, caret_blink_phase, caret_blink_visible};
pub use present_retain::PresentRetainPipeline;
pub use resources::{ResourceKind, TextureMeta, TexturePool};
pub use text::RuiTextRenderer;
pub use text_hit::{
    best_font_family, hit_test_input_is_line_side_blank, hit_test_input_text_index,
    hit_test_textarea_below_last_line, hit_test_textarea_click_caret_index,
    is_single_line_text_box, measure_prefix_width, truncate_with_ellipsis,
};
pub use text_xform::{
    bake_text_texture_key_for_3d, element_wants_sticky_xform, mat2d_uniform_scale, mat4_from_mat2d,
    mat4_identity, mat4_mul, ndc_to_pixel, project_point_mat4, resolve_glyph_raster,
    text_xform_mode, text_xform_offscreen, GlyphRasterMode, TextXformMode, BAKE_OVERSAMPLE,
    BAKE_SCALE_MAX, BAKE_SCALE_MIN,
};
pub use textured_pipeline::{TexturedInstanceInput, TexturedQuadPipeline};

/// Pick swapchain alpha. Opaque windows force `Opaque` so undrawn pixels stay solid;
/// frameless glass / transparent windows prefer premultiplied compositor alpha.
pub(crate) fn pick_composite_alpha(
    modes: &[wgpu::CompositeAlphaMode],
    compositor_alpha: bool,
) -> wgpu::CompositeAlphaMode {
    if modes.is_empty() {
        return wgpu::CompositeAlphaMode::Auto;
    }
    if compositor_alpha {
        for want in [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
            wgpu::CompositeAlphaMode::Auto,
        ] {
            if modes.contains(&want) {
                return want;
            }
        }
        log::warn!(
            "compositor alpha requested but surface only offers {modes:?}; glass may not show"
        );
        return modes[0];
    }
    modes
        .iter()
        .copied()
        .find(|m| *m == wgpu::CompositeAlphaMode::Opaque)
        .unwrap_or(modes[0])
}

pub(crate) fn surface_clear_color(compositor_alpha: bool) -> wgpu::Color {
    if compositor_alpha {
        wgpu::Color::TRANSPARENT
    } else {
        wgpu::Color {
            r: 0.95,
            g: 0.95,
            b: 0.95,
            a: 1.0,
        }
    }
}

#[inline]
pub(crate) fn color_target_blend(alpha_mode: wgpu::CompositeAlphaMode) -> wgpu::BlendState {
    match alpha_mode {
        wgpu::CompositeAlphaMode::PreMultiplied => wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
        _ => wgpu::BlendState::ALPHA_BLENDING,
    }
}

/// 将逻辑空间实例缩放到物理像素（DPI）。
pub fn scale_instances(instances: &[InstanceInput], scale: f32) -> Vec<InstanceInput> {
    let mut out = instances.to_vec();
    scale_instances_in_place(&mut out, scale);
    out
}

/// Scale in place. `scale ≈ 1` is a no-op (common 96-DPI path) to avoid per-frame copies.
pub fn scale_instances_in_place(instances: &mut [InstanceInput], scale: f32) {
    if (scale - 1.0).abs() <= f32::EPSILON {
        return;
    }
    for out in instances.iter_mut() {
        out.rect_pos[0] *= scale;
        out.rect_pos[1] *= scale;
        out.rect_size[0] *= scale;
        out.rect_size[1] *= scale;
        out.radii = [
            out.radii[0] * scale,
            out.radii[1] * scale,
            out.radii[2] * scale,
            out.radii[3] * scale,
        ];
        out.border_width *= scale;
        out.shadow_offset[0] *= scale;
        out.shadow_offset[1] *= scale;
        out.shadow_blur *= scale;
        out.shadow_spread *= scale;
        if out.clip_size[0] >= 0.0 && out.clip_size[1] >= 0.0 {
            out.clip_pos[0] *= scale;
            out.clip_pos[1] *= scale;
            out.clip_size[0] *= scale;
            out.clip_size[1] *= scale;
        }
    }
}

/// 布局收集阶段产出的纹理绘制实例（逻辑像素坐标）。
#[derive(Clone, Debug)]
pub struct TexturedDraw {
    pub rect: [f32; 4],
    /// 贴图 UV 起点（与 [`uv_for_subrect`] 一致：rect 顶 → `uv.y=0`）。
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub texture_key: String,
    /// Combined ancestor × element opacity for tint alpha.
    pub opacity: f32,
    /// RGB tint (SDF text bake uses glyph color here; images stay white).
    pub tint_rgb: [f32; 3],
    /// CSS/`compute_combined_transform` 矩阵（绕矩形中心）；默认单位阵。
    pub transform: [f32; 6],
}

impl TexturedDraw {
    /// `full_rect` = 叶布局矩形；`clip` = 祖先 overflow。
    /// 只提交相交像素，并用 UV 取完整贴图对应区域（禁止把整图挤进可见缝）。
    pub fn clipped(
        full_rect: [f32; 4],
        clip: Option<[f32; 4]>,
        texture_key: impl Into<String>,
        opacity: f32,
    ) -> Option<Self> {
        let draw = match clip {
            Some(c) if c[2] > 0.0 && c[3] > 0.0 => aabb_intersect(c, full_rect),
            _ => full_rect,
        };
        if draw[2] <= 0.0 || draw[3] <= 0.0 || full_rect[2] <= 0.0 || full_rect[3] <= 0.0 {
            return None;
        }
        let (uv_offset, uv_scale) = uv_for_subrect(full_rect, draw);
        Some(Self {
            rect: draw,
            uv_offset,
            uv_scale,
            texture_key: texture_key.into(),
            opacity,
            tint_rgb: [1.0, 1.0, 1.0],
            transform: crate::style::transform::MAT2D_IDENTITY,
        })
    }

    pub fn with_transform(mut self, transform: [f32; 6]) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_tint_rgb(mut self, rgb: [f32; 3]) -> Self {
        self.tint_rgb = rgb;
        self
    }
}

fn aabb_intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

/// `textured_rect` 顶点：rect 顶 → `uv.y=0`（与上传图 / 离屏 RT 行序一致）。
pub fn uv_for_subrect(full: [f32; 4], sub: [f32; 4]) -> ([f32; 2], [f32; 2]) {
    let fw = full[2].max(1e-6);
    let fh = full[3].max(1e-6);
    let u0 = ((sub[0] - full[0]) / fw).clamp(0.0, 1.0);
    let v0 = ((sub[1] - full[1]) / fh).clamp(0.0, 1.0);
    let us = (sub[2] / fw).clamp(0.0, 1.0);
    let vs = (sub[3] / fh).clamp(0.0, 1.0);
    ([u0, v0], [us, vs])
}

/// 缩放至物理像素后的纹理绘制矩形。
#[derive(Clone, Debug)]
pub struct ScaledTexturedDraw {
    pub rect: [f32; 4],
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub texture_key: String,
    pub opacity: f32,
    pub tint_rgb: [f32; 3],
    pub transform: [f32; 6],
}

impl ScaledTexturedDraw {
    pub fn to_instance(&self) -> TexturedInstanceInput {
        let flags = if self
            .texture_key
            .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
        {
            1.0
        } else {
            0.0
        };
        TexturedInstanceInput {
            rect_pos: [self.rect[0], self.rect[1]],
            rect_size: [self.rect[2], self.rect[3]],
            tint: [
                self.tint_rgb[0],
                self.tint_rgb[1],
                self.tint_rgb[2],
                self.opacity,
            ],
            uv_offset: self.uv_offset,
            uv_scale: self.uv_scale,
            transform: self.transform,
            flags,
            _pad: [0.0; 3],
        }
    }
}

pub fn scale_textured_draws(draws: &[TexturedDraw], scale: f32) -> Vec<ScaledTexturedDraw> {
    draws
        .iter()
        .map(|d| {
            // Scale translation of the affine; linear part (rotate/scale) stays.
            let mut t = d.transform;
            t[4] *= scale;
            t[5] *= scale;
            ScaledTexturedDraw {
                rect: [
                    d.rect[0] * scale,
                    d.rect[1] * scale,
                    d.rect[2] * scale,
                    d.rect[3] * scale,
                ],
                uv_offset: d.uv_offset,
                uv_scale: d.uv_scale,
                texture_key: d.texture_key.clone(),
                opacity: d.opacity,
                tint_rgb: d.tint_rgb,
                transform: t,
            }
        })
        .collect()
}

#[cfg(test)]
mod textured_uv_tests {
    use super::{scale_textured_draws, uv_for_subrect, TexturedDraw};

    #[test]
    fn scale_instances_in_place_noop_at_unity() {
        use super::{scale_instances_in_place, InstanceInput};
        use bytemuck::Zeroable;
        let mut v = vec![InstanceInput::zeroed()];
        v[0].rect_pos = [10.0, 20.0];
        v[0].rect_size = [3.0, 4.0];
        scale_instances_in_place(&mut v, 1.0);
        assert_eq!(v[0].rect_pos, [10.0, 20.0]);
        assert_eq!(v[0].rect_size, [3.0, 4.0]);
    }

    #[test]
    fn packed_instances_keep_per_draw_rect_not_last_write() {
        let draws = [
            TexturedDraw {
                rect: [10.0, 20.0, 100.0, 80.0],
                uv_offset: [0.0, 0.0],
                uv_scale: [1.0, 1.0],
                texture_key: "rui_path:line".into(),
                opacity: 1.0,
                tint_rgb: [1.0, 1.0, 1.0],
                transform: crate::style::transform::MAT2D_IDENTITY,
            },
            TexturedDraw {
                rect: [10.0, 200.0, 180.0, 180.0],
                uv_offset: [0.0, 0.0],
                uv_scale: [1.0, 1.0],
                texture_key: "rui_path:pie".into(),
                opacity: 1.0,
                tint_rgb: [1.0, 1.0, 1.0],
                transform: crate::style::transform::MAT2D_IDENTITY,
            },
        ];
        let scaled = scale_textured_draws(&draws, 1.0);
        let insts: Vec<_> = scaled.iter().map(|d| d.to_instance()).collect();
        assert_eq!(insts.len(), 2);
        assert!(
            (insts[0].rect_pos[1] - 20.0).abs() < 1e-3
                && (insts[0].rect_size[1] - 80.0).abs() < 1e-3,
            "line instance must keep its own rect, got {:?}",
            insts[0]
        );
        assert!(
            (insts[1].rect_pos[1] - 200.0).abs() < 1e-3
                && (insts[1].rect_size[1] - 180.0).abs() < 1e-3,
            "pie instance must keep its own rect, got {:?}",
            insts[1]
        );
    }

    #[test]
    fn full_rect_keeps_identity_uv() {
        let (off, scale) = uv_for_subrect([0.0, 0.0, 100.0, 80.0], [0.0, 0.0, 100.0, 80.0]);
        assert!((off[0]).abs() < 1e-5 && (off[1]).abs() < 1e-5);
        assert!((scale[0] - 1.0).abs() < 1e-5 && (scale[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn top_half_clip_samples_upper_texel_band() {
        let (off, scale) = uv_for_subrect([0.0, 0.0, 100.0, 80.0], [0.0, 0.0, 100.0, 40.0]);
        assert!((scale[1] - 0.5).abs() < 1e-5);
        assert!(
            off[1].abs() < 1e-5,
            "top half → uv.y starts at 0 (no Y-flip), got {}",
            off[1]
        );
    }

    #[test]
    fn clipped_draw_does_not_squash_into_sliver_without_uv() {
        let d = TexturedDraw::clipped(
            [10.0, 100.0, 180.0, 180.0],
            Some([0.0, 200.0, 400.0, 80.0]),
            "rui_path:test",
            1.0,
        )
        .expect("intersect");
        assert!(d.rect[3] < 180.0, "visible h={}", d.rect[3]);
        assert!(
            (d.uv_scale[1] - d.rect[3] / 180.0).abs() < 1e-4,
            "uv_scale.y must match visible/full height, got {} vs {}",
            d.uv_scale[1],
            d.rect[3] / 180.0
        );
    }
}

#[cfg(test)]
mod composite_alpha_tests {
    use super::{pick_composite_alpha, surface_clear_color};

    #[test]
    fn opaque_prefers_opaque_mode() {
        let modes = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Opaque,
        ];
        assert_eq!(
            pick_composite_alpha(&modes, false),
            wgpu::CompositeAlphaMode::Opaque
        );
        assert_eq!(surface_clear_color(false).a, 1.0);
    }

    #[test]
    fn compositor_prefers_premultiplied() {
        let modes = [
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::PreMultiplied,
        ];
        assert_eq!(
            pick_composite_alpha(&modes, true),
            wgpu::CompositeAlphaMode::PreMultiplied
        );
        assert_eq!(surface_clear_color(true).a, 0.0);
    }
}
