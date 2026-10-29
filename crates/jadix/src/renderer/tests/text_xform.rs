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
    // hover:scale-125 settles at 1.25, outside this band — sticky collect
    // rasters at 1.0 instead (see layout_tree Bake raster_scale).
    assert!((bake_source_scale(1.25) - 1.25).abs() < 1e-6);
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
    el.attrs.insert("jadix_sticky_xform".into(), "1".into());
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
