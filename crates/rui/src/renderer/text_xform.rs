//! Text under CSS transform: three raster modes.
//!
//! - [`GlyphRasterMode::Coverage`]: axis-aligned UI — glyphon swash + Nearest (sharp small sizes).
//! - [`GlyphRasterMode::Bake`]: sticky supersampled RGBA + Linear(+mip) textured quad (follow anim).
//!   Emoji leaves composite **Twemoji** into the Bake RT (never font outline / MSDF).
//! - [`GlyphRasterMode::Msdf`]: distance-field bake for free scale / 3D (after Bake band).
//!   **Not used** when the leaf contains emoji (color would collapse to a wireframe).
//!
//! Shader path for Msdf: `median(RGB)` + screen-space `pxRange` in `textured_rect.wgsl`
//! (single-channel SDF packed as R=G=B today; true multi-channel MSDF later).
//! 2D draw uses mat2 about rect center; 3D uses [`project_point_mat4`] then the same VS.
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
/// 3D milestone: same supersampled (or SDF) atlas as 2D follow-anim; apply perspective
/// via [`project_point_mat4`] on CPU (billboard corners → screen AABB), then the
/// textured `mat2` VS. Does not modify glyphon `GlyphToRender` (Coverage stays atlas-quad).
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
    let px = mvp[0] * x + mvp[4] * y + mvp[8] * z + mvp[12];
    let py = mvp[1] * x + mvp[5] * y + mvp[9] * z + mvp[13];
    let pz = mvp[2] * x + mvp[6] * y + mvp[10] * z + mvp[14];
    let pw = mvp[3] * x + mvp[7] * y + mvp[11] * z + mvp[15];
    if pw.abs() < 1e-8 {
        return None;
    }
    Some([px / pw, py / pw, pz / pw])
}

/// NDC → pixel top-left origin (matches textured / glyphon Y-down).
pub fn ndc_to_pixel(ndc: [f32; 2], width: f32, height: f32) -> [f32; 2] {
    [
        (ndc[0] * 0.5 + 0.5) * width,
        (1.0 - (ndc[1] * 0.5 + 0.5)) * height,
    ]
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
    if s.animations.iter().any(|a| {
        let n = a.name.to_ascii_lowercase();
        n.contains("spin") || n.contains("rotate")
    }) {
        return true;
    }
    // `transition: transform` alone is not sticky. Toast / Dialog zoom only tween
    // independent `translate`/`scale` (already baked into the rect + `paint_scale`).
    // Treating the transition list as sticky forced Bake on every descendant label,
    // so stacked overflow-clipped cards painted as empty white shells.
    false
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
    fn uniform_scale_is_not_sticky() {
        let mut el = Element::new();
        el.style.scale = [0.95, 0.95];
        assert!(
            !element_wants_sticky_xform(&el),
            "scale-only motion must keep Coverage + paint_scale, not Bake"
        );
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
}
