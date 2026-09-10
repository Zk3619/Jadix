//! Text under CSS transform: three raster modes.
//!
//! - [`GlyphRasterMode::Coverage`]: axis-aligned UI — glyphon swash + Nearest (sharp small sizes).
//! - [`GlyphRasterMode::Bake`]: sticky supersampled RGBA + Linear(+mip) textured quad (follow anim).
//!   Emoji leaves composite **Twemoji** into the Bake RT (never font outline / MSDF).
//! - [`GlyphRasterMode::Msdf`]: distance-field bake for free scale / 3D (after Bake band).
//!   **Not used** when the leaf contains emoji (color would collapse to a wireframe).
//!
//! Shader path for Msdf: `median(RGB)` + screen-space gradient AA in `textured_rect.wgsl`
//! (true 3-channel MSDF from edge-pair coloring; R=G=B still decodes as a single SDF).
//! 2D draw uses mat2 about rect center; 3D labels use [`project_billboard_warp`]
//! (4 corners + clip `w`) so the textured VS can do perspective-correct UV.
//!
//! Vello-class GPU vector text is a later spike (`utils/text_vector_spike.rs` / `docs/drawing.md`).

use crate::core::tree::Element;
use crate::style::transform::{mat2d_is_axis_aligned, Mat2d};

/// How a text leaf reaches the GPU when ancestors may rotate / scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphRasterMode {
    Coverage,
    Bake,
    Msdf,
}

/// Preferred supersample for Bake (2–3×). Kept out of cache key angles.
/// Higher helps rotate AA once mips are off (mip + minify was washing out 2.5×).
pub const BAKE_OVERSAMPLE: f32 = 3.0;

/// Uniform scale band where Bake stays sharp enough; outside → [`GlyphRasterMode::Msdf`].
pub const BAKE_SCALE_MIN: f32 = 0.5;
pub const BAKE_SCALE_MAX: f32 = 2.0;

/// Bake / SDF texture key usable as a scene3d / textured material source.
///
/// 3D milestone: same MSDF atlas as 2D free-scale; perspective via
/// [`project_billboard_warp`] (not an AABB). Coverage glyphon is unchanged.
pub fn bake_text_texture_key_for_3d(
    text: &str,
    font_size_px: f32,
    font_weight: u16,
    color: [u8; 4],
    box_w: f32,
    box_h: f32,
) -> Option<String> {
    let (bw, bh, pad) = crate::utils::text_texture::text_xform_bake_box(box_w, box_h, font_size_px);
    crate::utils::text_texture::ensure_text_xform_texture(
        text,
        font_size_px,
        font_weight,
        color,
        box_w,
        box_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Center,
        3.0,
        true, // SDF for free perspective scale
        0.0,
    )
    .map(|(k, _)| k)
}

/// Column-major 4×4 identity.
pub fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// Embed a 2×2 linear + translation into a column-major mat4 (z=0 plane).
pub fn mat4_from_mat2d(m: &crate::style::transform::Mat2d) -> [f32; 16] {
    let mut out = mat4_identity();
    out[0] = m[0];
    out[1] = m[1];
    out[4] = m[2];
    out[5] = m[3];
    out[12] = m[4];
    out[13] = m[5];
    out
}

/// `out = a * b` (column-major).
pub fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            r[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    r
}

/// Perspective divide: returns `(ndc_x, ndc_y, ndc_z)` or `None` if `w≈0`.
pub fn project_point_mat4(mvp: &[f32; 16], x: f32, y: f32, z: f32) -> Option<[f32; 3]> {
    let clip = project_point_mat4_clip(mvp, x, y, z)?;
    let w = clip[3];
    Some([clip[0] / w, clip[1] / w, clip[2] / w])
}

/// Clip-space `xyzw` **before** perspective divide.
pub fn project_point_mat4_clip(mvp: &[f32; 16], x: f32, y: f32, z: f32) -> Option<[f32; 4]> {
    let px = mvp[0] * x + mvp[4] * y + mvp[8] * z + mvp[12];
    let py = mvp[1] * x + mvp[5] * y + mvp[9] * z + mvp[13];
    let pz = mvp[2] * x + mvp[6] * y + mvp[10] * z + mvp[14];
    let pw = mvp[3] * x + mvp[7] * y + mvp[11] * z + mvp[15];
    if pw.abs() < 1e-8 {
        return None;
    }
    Some([px, py, pz, pw])
}

/// NDC → pixel top-left origin (matches textured / glyphon Y-down).
pub fn ndc_to_pixel(ndc: [f32; 2], width: f32, height: f32) -> [f32; 2] {
    [
        (ndc[0] * 0.5 + 0.5) * width,
        (1.0 - (ndc[1] * 0.5 + 0.5)) * height,
    ]
}

/// 3D label plate → 4 texture-space corners (TL/TR/BR/BL) + clip `w` for warp VS.
#[derive(Clone, Copy, Debug)]
pub struct BillboardWarp {
    pub corners_px: [[f32; 2]; 4],
    pub clip_w: [f32; 4],
}

impl BillboardWarp {
    pub fn warp_attrs(&self) -> ([f32; 4], [f32; 4], [f32; 4]) {
        let c = self.corners_px;
        (
            [c[0][0], c[0][1], c[1][0], c[1][1]],
            [c[2][0], c[2][1], c[3][0], c[3][1]],
            self.clip_w,
        )
    }

    pub fn top_edge_len(&self) -> f32 {
        let a = self.corners_px[0];
        let b = self.corners_px[1];
        (a[0] - b[0]).hypot(a[1] - b[1])
    }

    pub fn bottom_edge_len(&self) -> f32 {
        let a = self.corners_px[3];
        let b = self.corners_px[2];
        (a[0] - b[0]).hypot(a[1] - b[1])
    }
}

/// Project a Y-up world plate (`bl, br, tr, tl`) to Y-down texture corners.
pub fn project_billboard_warp(
    mvp: &[f32; 16],
    world_bl_br_tr_tl: [[f32; 3]; 4],
    viewport_w: f32,
    viewport_h: f32,
) -> Option<BillboardWarp> {
    let mut clip = [[0.0f32; 4]; 4];
    for (i, c) in world_bl_br_tr_tl.iter().enumerate() {
        clip[i] = project_point_mat4_clip(mvp, c[0], c[1], c[2])?;
    }
    // Texture TL/TR/BR/BL = world TL/TR/BR/BL = indices 3, 2, 1, 0.
    let order = [3usize, 2, 1, 0];
    let mut corners_px = [[0.0f32; 2]; 4];
    let mut clip_w = [0.0f32; 4];
    for (out, &i) in order.iter().enumerate() {
        let w = clip[i][3];
        let ndc = [clip[i][0] / w, clip[i][1] / w];
        corners_px[out] = ndc_to_pixel(ndc, viewport_w, viewport_h);
        clip_w[out] = w;
    }
    Some(BillboardWarp {
        corners_px,
        clip_w,
    })
}

/// Legacy enum — Pipeline = Coverage/glyphon; Offscreen = Bake path enabled in layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextXformMode {
    Pipeline,
    Offscreen,
}

/// True when this element's style/attrs request sticky xform text (even at 0°).
pub fn element_wants_sticky_xform(el: &Element) -> bool {
    let s = &el.style;
    if s.rotate_deg.abs() > 1e-6 {
        return true;
    }
    if s.transform.as_ref().is_some_and(|t| !t.is_empty()) {
        return true;
    }
    // Uniform CSS `scale` is already baked into the instance rect + glyphon
    // `paint_scale`. Treating it as sticky forced Bake/Msdf at the animated font
    // size (Dialog `zoom-in-95`: every label re-rasterizes at 0.95 then snaps back
    // to Coverage at 1.0 — the "缩小态渲染异常" artifact, plus exit stutter).
    if el.attrs.get("rui_animate").map(|v| v.as_str()) == Some("spin") {
        return true;
    }
    if el.attrs.get("rui_sticky_xform").map(|v| v.as_str()) == Some("1") {
        return true;
    }
    if s.animations.iter().any(|a| {
        let n = a.name.to_ascii_lowercase();
        n.contains("spin") || n.contains("rotate")
    }) {
        return true;
    }
    // Hover linear xform (scale/rotate/skew) must sticky-bake at rest so the
    // first pointer-enter does not pop Coverage→Bake (Twemoji reaction chips).
    // Toast / Dialog zoom uses inline `transition: transform` + programmatic
    // scale — no hover:scale class — and must stay Coverage.
    if el
        .class_set
        .as_ref()
        .is_some_and(|cs| cs.has_hover_linear_xform())
    {
        return true;
    }
    // `transition: transform` alone is not sticky. Toast / Dialog zoom only tween
    // independent `translate`/`scale` (already baked into the rect + `paint_scale`).
    // Treating the transition list as sticky forced Bake on every descendant label,
    // so stacked overflow-clipped cards painted as empty white shells.
    false
}

/// CSS `scale` used for **raster** (Bake font size / glyphon `TextArea.scale`).
///
/// Near-identity zooms (`zoom-in-90` is 0.90→1.0) must bake at rest and let the
/// draw quad carry the scale. Multiplying font size by 0.90 every frame forces a
/// new glyph atlas — Dialog stutter vs Sheet `translate` (paint_scale stays 1).
pub fn bake_source_scale(paint_scale: f32) -> f32 {
    if paint_scale >= 0.85 && paint_scale <= 1.15 {
        1.0
    } else {
        paint_scale.max(0.01)
    }
}

/// Average absolute scale from a 2×2 linear matrix.
pub fn mat2d_uniform_scale(m: &Mat2d) -> f32 {
    let sx = (m[0] * m[0] + m[1] * m[1]).sqrt();
    let sy = (m[2] * m[2] + m[3] * m[3]).sqrt();
    ((sx + sy) * 0.5).max(1e-6)
}

/// Resolve raster mode for a text leaf.
///
/// `paint_scale` is the CSS/`ancestor_scale` factor already baked into the draw rect
/// (not present in `world_linear` — see layout scale¹ rule). Large paint scale → Msdf.
pub fn resolve_glyph_raster(
    sticky_or_ancestor: bool,
    world_linear: &Mat2d,
    paint_scale: f32,
) -> GlyphRasterMode {
    let needs = sticky_or_ancestor || !mat2d_is_axis_aligned(world_linear);
    if !needs {
        return GlyphRasterMode::Coverage;
    }
    let s = mat2d_uniform_scale(world_linear).max(paint_scale.max(1e-6));
    if s >= BAKE_SCALE_MIN && s <= BAKE_SCALE_MAX {
        GlyphRasterMode::Bake
    } else {
        GlyphRasterMode::Msdf
    }
}

/// `true` when layout should use Bake/Msdf textured path (not glyphon Coverage).
pub fn text_xform_offscreen() -> bool {
    true
}

pub fn text_xform_mode() -> TextXformMode {
    TextXformMode::Offscreen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::dimension::CssAngle;
    use crate::style::transform::{TransformFunction, MAT2D_IDENTITY};
    use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};

    #[test]
    fn coverage_when_idle_identity() {
        assert_eq!(
            resolve_glyph_raster(false, &MAT2D_IDENTITY, 1.0),
            GlyphRasterMode::Coverage
        );
    }

    #[test]
    fn bake_when_sticky_even_identity() {
        assert_eq!(
            resolve_glyph_raster(true, &MAT2D_IDENTITY, 1.0),
            GlyphRasterMode::Bake
        );
    }

    #[test]
    fn bake_when_rotated() {
        let c = 45f32.to_radians().cos();
        let s = 45f32.to_radians().sin();
        let m = [c, s, -s, c, 0.0, 0.0];
        assert_eq!(resolve_glyph_raster(false, &m, 1.0), GlyphRasterMode::Bake);
    }

    #[test]
    fn msdf_when_scale_large() {
        let m = [3.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        assert_eq!(resolve_glyph_raster(true, &m, 1.0), GlyphRasterMode::Msdf);
    }

    #[test]
    fn msdf_when_paint_scale_large_even_if_matrix_identity() {
        // CSS `scale` is baked into the draw rect; world_linear stays identity.
        assert_eq!(
            resolve_glyph_raster(true, &MAT2D_IDENTITY, 3.0),
            GlyphRasterMode::Msdf
        );
    }

    #[test]
    fn zoom_in_90_animation_is_not_sticky() {
        use crate::style::{keyframes_anim_entry, AnimationFillMode};
        let mut el = Element::new();
        el.style.animations.push(keyframes_anim_entry(
            "zoom-in-90",
            200.0,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        ));
        assert!(
            !element_wants_sticky_xform(&el),
            "Dialog zoom must not sticky-bake: Coverage+Bake double-paint is the enter ghost"
        );
    }

    #[test]
    fn bake_source_scale_identity_band() {
        assert!((bake_source_scale(0.90) - 1.0).abs() < 1e-6);
        assert!((bake_source_scale(1.0) - 1.0).abs() < 1e-6);
        assert!((bake_source_scale(1.10) - 1.0).abs() < 1e-6);
        assert!((bake_source_scale(1.5) - 1.5).abs() < 1e-6);
        assert!((bake_source_scale(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn transform_transition_alone_is_not_sticky() {
        let mut el = Element::new();
        el.style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 150.0,
            delay_ms: 0.0,
            timing: TimingFunction::Ease,
        });
        el.style.scale = [0.95, 0.95];
        el.style.translate = [0.0, 14.0];
        assert!(
            !element_wants_sticky_xform(&el),
            "translate/scale tweens must keep Coverage; rotate/transform-list still opt in"
        );
    }

    #[test]
    fn sticky_from_rotate_class_state() {
        let mut el = Element::new();
        el.style.rotate_deg = -15.0;
        assert!(element_wants_sticky_xform(&el));
        el.style.rotate_deg = 0.0;
        el.style.transform = Some(vec![TransformFunction::Rotate(CssAngle::Deg(12.0))]);
        assert!(element_wants_sticky_xform(&el));
    }

    #[test]
    fn hover_scale_class_is_sticky_at_rest() {
        let mut el = Element::new();
        crate::style::apply_class_list(
            &mut el,
            "cursor-pointer transition-transform duration-150 hover:scale-125",
        );
        assert!(
            element_wants_sticky_xform(&el),
            "hover:scale must sticky-bake so the first hover does not pop Coverage→Bake"
        );
    }

    #[test]
    fn hover_rotate_class_is_sticky_at_rest() {
        let mut el = Element::new();
        crate::style::apply_class_list(&mut el, "transition-transform hover:rotate-12");
        assert!(element_wants_sticky_xform(&el));
    }

    #[test]
    fn transition_transform_class_alone_is_not_sticky() {
        let mut el = Element::new();
        crate::style::apply_class_list(&mut el, "transition-transform duration-150 ease-out");
        el.style.scale = [0.95, 0.95];
        assert!(
            !element_wants_sticky_xform(&el),
            "Toast/Dialog without zoom-* keyframes: transition-transform stays Coverage"
        );
    }

    #[test]
    fn sticky_xform_attr_opts_in() {
        let mut el = Element::new();
        el.attrs.insert("rui_sticky_xform".into(), "1".into());
        assert!(element_wants_sticky_xform(&el));
    }

    #[test]
    fn mat4_from_mat2d_preserves_linear() {
        let m2 = [2.0, 0.0, 0.0, 3.0, 10.0, 20.0];
        let m4 = mat4_from_mat2d(&m2);
        assert!((m4[0] - 2.0).abs() < 1e-6);
        assert!((m4[5] - 3.0).abs() < 1e-6);
        assert!((m4[12] - 10.0).abs() < 1e-6);
        assert!((m4[13] - 20.0).abs() < 1e-6);
        let p = project_point_mat4(&m4, 1.0, 1.0, 0.0).expect("w");
        assert!((p[0] - 12.0).abs() < 1e-4);
        assert!((p[1] - 23.0).abs() < 1e-4);
    }

    #[test]
    fn billboard_warp_perspective_tapers_far_edge() {
        // clip.xy = xy, clip.w = z + 1 → farther (larger z) appears smaller.
        let mut mvp = mat4_identity();
        mvp[15] = 1.0;
        mvp[11] = 1.0; // clip.w = z + 1 (column-major: m[3+8]=m[11] * z)
        // Wait: clip.w = m[3]*x + m[7]*y + m[11]*z + m[15]
        // identity has m[15]=1, m[11]=0. Set m[11]=1 → w = z + 1.
        let world = [
            [-1.0, -0.2, 0.4], // BL near
            [1.0, -0.2, 0.4],  // BR near
            [1.0, 0.2, 1.6],   // TR far
            [-1.0, 0.2, 1.6],  // TL far
        ];
        let warp = project_billboard_warp(&mvp, world, 200.0, 100.0).expect("project");
        let near = warp.bottom_edge_len();
        let far = warp.top_edge_len();
        assert!(
            near > far * 1.15,
            "near edge {near:.1} should exceed far edge {far:.1} under perspective"
        );
        // Not an AABB: top and bottom widths differ, and x of TL != x of BL.
        assert!(
            (warp.corners_px[0][0] - warp.corners_px[3][0]).abs() > 1.0,
            "tilted plate must not stay a screen-aligned rectangle"
        );
    }

    #[test]
    fn billboard_warp_attrs_pack_tl_tr_br_bl() {
        let mut mvp = mat4_identity();
        mvp[15] = 1.0;
        let world = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        let warp = project_billboard_warp(&mvp, world, 100.0, 100.0).expect("project");
        let (c01, c23, w) = warp.warp_attrs();
        assert_eq!(c01[0], warp.corners_px[0][0]);
        assert_eq!(c01[2], warp.corners_px[1][0]);
        assert_eq!(c23[0], warp.corners_px[2][0]);
        assert_eq!(c23[2], warp.corners_px[3][0]);
        assert!(w.iter().all(|v| *v > 0.0));
    }
}
