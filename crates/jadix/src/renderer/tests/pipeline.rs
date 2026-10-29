use super::InstanceInput;

#[test]
fn opaque_center_requires_full_coverage_and_untransformed_painter_geometry() {
    let mut inst = InstanceInput {
        rect_pos: [0.0; 2],
        rect_size: [640.0; 2],
        color: [1.0; 4],
        border_color: [0.0; 4],
        radii: [12.0; 4],
        border_width: 3.0,
        shadow_color: [0.0, 0.0, 0.0, 0.2],
        shadow_offset: [4.0, 6.0],
        shadow_blur: 12.0,
        shadow_spread: 0.0,
        shadow_inset: 0.0,
        backdrop_fade: 0.0,
        transform: crate::style::transform::MAT2D_IDENTITY,
        clip_pos: [0.0; 2],
        clip_size: InstanceInput::NO_CLIP_SIZE,
        border_extra: 1.0,
        backdrop_blur: 0.0,
        glass: [0.0; 4],
    };
    assert!(super::opaque_box_eligible(&inst));
    inst.shadow_inset = 1.0;
    assert!(
        !super::opaque_box_eligible(&inst),
        "inset shadow colors the center"
    );
    inst.shadow_inset = 0.0;
    inst.color[3] = 0.9;
    assert!(
        !super::opaque_box_eligible(&inst),
        "alpha order must remain unchanged"
    );
    inst.color[3] = 1.0;
    inst.border_extra = 2.0;
    assert!(
        !super::opaque_box_eligible(&inst),
        "decorative borders keep general coverage"
    );
    inst.border_extra = 1.0;
    inst.transform[0] = 1.01;
    assert!(
        !super::opaque_box_eligible(&inst),
        "transformed SDF uses its original AABB"
    );
    inst.transform[0] = 1.0;
    inst.backdrop_blur = 4.0;
    assert!(!super::opaque_box_eligible(&inst));
    inst.backdrop_blur = 0.0;
    inst.radii = [320.0; 4];
    assert!(
        !super::opaque_box_eligible(&inst),
        "round body has no opaque rectangular center"
    );
    inst.radii = [8.0; 4];
    inst.shadow_color = [0.0; 4];
    assert!(super::solid_fill_eligible(&inst));
    inst.glass = [-1.0, 0.5, 0.22, 0.0];
    assert!(
        !super::solid_fill_eligible(&inst),
        "a sweep is not a constant fill"
    );
    assert!(
        !super::opaque_box_eligible(&inst),
        "a sweep has transparent edges"
    );
}

#[test]
fn contained_large_quad_skips_only_redundant_convex_clips() {
    use super::{instance_clip_needed, InstanceInput};
    use crate::style::GpuClipPoly;
    let mut inst = InstanceInput {
        rect_pos: [64.0, 64.0],
        rect_size: [256.0, 256.0],
        color: [1.0; 4],
        border_color: [0.0; 4],
        radii: [12.0; 4],
        border_width: 3.0,
        shadow_color: [0.0; 4],
        shadow_offset: [0.0; 2],
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_inset: 0.0,
        backdrop_fade: 0.0,
        transform: crate::style::transform::MAT2D_IDENTITY,
        clip_pos: [0.0; 2],
        clip_size: InstanceInput::NO_CLIP_SIZE,
        border_extra: 1.0,
        backdrop_blur: 0.0,
        glass: [0.0; 4],
    };
    let clip = GpuClipPoly::from_rounded_rect([0.0, 0.0, 384.0, 384.0], [24.0; 4]);
    assert!(!instance_clip_needed(&inst, &clip));
    inst.rect_pos = [12.0, 12.0];
    assert!(
        instance_clip_needed(&inst, &clip),
        "AA/border may touch rounded corner"
    );
    inst.rect_pos = [64.0, 64.0];
    inst.shadow_blur = 40.0;
    assert!(
        instance_clip_needed(&inst, &clip),
        "shadow halo leaves clip"
    );
    inst.shadow_blur = 0.0;
    inst.transform = [0.707, 0.707, -0.707, 0.707, 0.0, 0.0];
    assert!(
        instance_clip_needed(&inst, &clip),
        "rotated AABB leaves rounded corner"
    );
    assert!(!instance_clip_needed(&inst, &GpuClipPoly::NONE));
    let poly = GpuClipPoly::from_world_points(
        &[[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [0.0, 1000.0]],
        crate::style::ClipFillRule::NonZero,
    );
    assert!(
        instance_clip_needed(&inst, &poly),
        "polygon paths retain fragment tests"
    );
    inst.transform = crate::style::transform::MAT2D_IDENTITY;
    let mut second = clip;
    second.round_rect2 = [100.0, 100.0, 100.0, 100.0];
    assert!(instance_clip_needed(&inst, &second));
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
}

#[test]
fn pack_unpack_border_extra_roundtrip() {
    let cases = [
        (1.0, 0.0, 0.0), // solid + round
        (2.0, 0.0, 0.0), // dotted + round
        (3.0, 0.0, 0.0), // dashed
        (1.0, 1.0, 0.0), // solid + angle
        (1.0, 2.0, 4.0), // superellipse(4)
        (1.0, 2.0, 8.0), // superellipse(8)
        (1.0, 3.0, 0.0), // notch
        (1.0, 4.0, 0.0), // scoop
    ];
    for (style, shape, param) in cases {
        let packed = InstanceInput::pack_border_extra(style, shape, param);
        let (s, sh, p) = InstanceInput::unpack_border_extra(packed);
        approx(s, style);
        approx(sh, shape);
        approx(p, param);
    }
}
