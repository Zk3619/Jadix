use super::{scale_textured_draws, uv_for_subrect, TexturedDraw};

#[test]
fn rounded_texture_clips_scale_with_display_density() {
    let mut draw = TexturedDraw::clipped([8.0, 8.0, 88.0, 88.0], None, "photo", 1.0).unwrap();
    draw.rounded_clips = [[8.0, 8.0, 88.0, 88.0], [44.0; 4], [0.0; 4], [0.0; 4]];
    let scaled = scale_textured_draws(&[draw], 1.5);
    let instance = scaled[0].to_instance();
    assert_eq!(instance.rounded_clips[0], [12.0, 12.0, 132.0, 132.0]);
    assert_eq!(instance.rounded_clips[1], [66.0; 4]);
    assert_eq!(instance.rounded_clips[2], [0.0; 4]);
}

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
fn scale_instances_preserves_normalized_shimmer_phase() {
    use super::{scale_instances, InstanceInput};
    use bytemuck::Zeroable;
    let mut shimmer = InstanceInput::zeroed();
    shimmer.glass = [-1.0, 0.5, 0.22, 0.02];
    let mut glass = InstanceInput::zeroed();
    glass.glass = [1.46, 8.0, 0.1, 0.6];
    for dpi in [0.75, 1.0, 1.25, 1.5, 2.0] {
        let scaled = scale_instances(&[shimmer, glass], dpi);
        assert_eq!(
            scaled[0].glass, shimmer.glass,
            "sweep phase must not scale with DPI"
        );
        assert_eq!(
            scaled[1].glass[1],
            8.0 * dpi,
            "glass thickness still uses physical pixels"
        );
    }
}

#[test]
fn scale_instances_scales_transform_translation() {
    use super::{scale_instances_in_place, InstanceInput};
    use bytemuck::Zeroable;
    let mut v = vec![InstanceInput::zeroed()];
    let c = 45.0f32.to_radians().cos();
    let s = 45.0f32.to_radians().sin();
    v[0].transform = [c, s, -s, c, 10.0, -4.0];
    scale_instances_in_place(&mut v, 2.0);
    assert!(
        (v[0].transform[4] - 20.0).abs() < 1e-4 && (v[0].transform[5] + 8.0).abs() < 1e-4,
        "origin txty must scale with DPI, got {:?}",
        v[0].transform
    );
    assert!(
        (v[0].transform[0] - c).abs() < 1e-5,
        "linear part must stay, got {:?}",
        v[0].transform
    );
}

#[test]
fn zoom_capture_blit_sets_sparse_sprite_flag() {
    // flags=2: discard a=0 (live inner SDF) and un-premultiply capture samples.
    use super::ScaledTexturedDraw;
    let mk = |key: &str| ScaledTexturedDraw {
        rect: [0.0, 0.0, 10.0, 10.0],
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_key: key.into(),
        opacity: 1.0,
        tint_rgb: [1.0, 1.0, 1.0],
        transform: crate::style::transform::MAT2D_IDENTITY,
        clip_pos: [0.0, 0.0],
        clip_size: TexturedDraw::NO_CLIP_SIZE,
        rounded_clips: [[0.0; 4]; 4],
    };
    assert!((mk("jadix_zoom_cap:ab").to_instance().flags - 2.0).abs() < 1e-5);
    assert!((mk("jadix_filter_cap:cd").to_instance().flags - 2.0).abs() < 1e-5);
    assert!(mk("photo.png").to_instance().flags.abs() < 1e-5);
    assert!(
        (mk(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            .to_instance()
            .flags
            - 1.0)
            .abs()
            < 1e-5
    );
}

#[test]
fn sparse_capture_unpremul_recovers_variant_color() {
    // Transparent RT stores color * coverage. ALPHA_BLENDING that sample
    // without un-premultiply yields color * coverage² (washed gray).
    let color = [34.0f32 / 255.0, 197.0 / 255.0, 94.0 / 255.0];
    let cov = 0.78f32;
    let stored = [color[0] * cov, color[1] * cov, color[2] * cov];
    let recovered = [stored[0] / cov, stored[1] / cov, stored[2] / cov];
    for i in 0..3 {
        assert!((recovered[i] - color[i]).abs() < 1e-5);
    }
    let washed = [stored[0] * cov, stored[1] * cov, stored[2] * cov];
    assert!(
        (washed[1] - color[1]).abs() > 0.05,
        "double coverage must be a visible wash"
    );
}

#[test]
fn packed_instances_keep_per_draw_rect_not_last_write() {
    let draws = [
        TexturedDraw {
            rect: [10.0, 20.0, 100.0, 80.0],
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            texture_key: "jadix_path:line".into(),
            opacity: 1.0,
            tint_rgb: [1.0, 1.0, 1.0],
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0, 0.0],
            clip_size: TexturedDraw::NO_CLIP_SIZE,
            rounded_clips: [[0.0; 4]; 4],
        },
        TexturedDraw {
            rect: [10.0, 200.0, 180.0, 180.0],
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            texture_key: "jadix_path:pie".into(),
            opacity: 1.0,
            tint_rgb: [1.0, 1.0, 1.0],
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0, 0.0],
            clip_size: TexturedDraw::NO_CLIP_SIZE,
            rounded_clips: [[0.0; 4]; 4],
        },
    ];
    let scaled = scale_textured_draws(&draws, 1.0);
    let insts: Vec<_> = scaled.iter().map(|d| d.to_instance()).collect();
    assert_eq!(insts.len(), 2);
    assert!(
        (insts[0].rect_pos[1] - 20.0).abs() < 1e-3 && (insts[0].rect_size[1] - 80.0).abs() < 1e-3,
        "line instance must keep its own rect, got {:?}",
        insts[0]
    );
    assert!(
        (insts[1].rect_pos[1] - 200.0).abs() < 1e-3 && (insts[1].rect_size[1] - 180.0).abs() < 1e-3,
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
        "jadix_path:test",
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

#[test]
fn rotated_overflow_keeps_full_quad_and_gpu_clips() {
    // vertical-rl bake: swap 40×120 → 120×40, then rotate 90° about center.
    let full = [0.0, 40.0, 120.0, 40.0];
    let parent_rect = [40.0, 0.0, 40.0, 120.0];
    let parent = Some(parent_rect);
    let squeezed =
        TexturedDraw::clipped(full, parent, "jadix_text_xform_v3:cpu", 1.0).expect("cpu intersect");
    assert!(
        squeezed.rect[2] < 50.0,
        "pre-rotation AABB clip is the bug: width {}",
        squeezed.rect[2]
    );

    let keep = TexturedDraw::clipped(full, None, "jadix_text_xform_v3:gpu", 1.0)
        .expect("full")
        .with_overflow_clip(parent);
    assert!(
        (keep.rect[2] - 120.0).abs() < 0.1 && (keep.uv_scale[0] - 1.0).abs() < 1e-4,
        "rotated bake must keep the swapped quad, got rect={:?} uv={:?}",
        keep.rect,
        keep.uv_scale
    );
    assert!(
        (keep.clip_size[0] - 40.0).abs() < 0.1 && (keep.clip_size[1] - 120.0).abs() < 0.1,
        "GPU clip must be the overflow AABB, got {:?}",
        keep.clip_size
    );

    let xform =
        crate::style::mat2d_linear(&crate::style::mat2d_rotate(std::f32::consts::FRAC_PI_2));
    let vis = super::aabb_of_centered_transform(full, &xform);
    assert!(
        (vis[2] - 40.0).abs() < 0.5 && (vis[3] - 120.0).abs() < 0.5,
        "90° of swapped bake must recover the layout AABB, got {vis:?}"
    );
    assert!(!super::aabbs_disjoint(vis, parent_rect));

    let keep_rot = keep.clone().with_transform(xform);
    let vis_c = [vis[0] + vis[2] * 0.5, vis[1] + vis[3] * 0.5];
    assert!(
        super::overflow_clip_contains_world(keep_rot.clip_pos, keep_rot.clip_size, vis_c,),
        "rotated bake clips in world; visual center must stay, vis={vis:?}"
    );
    assert!(
        !super::overflow_clip_contains_world(keep_rot.clip_pos, keep_rot.clip_size, [0.0, 40.0],),
        "unrotated bake-quad corner must miss the world overflow AABB"
    );
}

#[test]
fn scale_textured_draws_scales_overflow_clip() {
    let d = TexturedDraw::clipped([0.0, 0.0, 10.0, 10.0], None, "k", 1.0)
        .unwrap()
        .with_overflow_clip(Some([2.0, 4.0, 8.0, 16.0]));
    let scaled = scale_textured_draws(&[d], 2.0);
    assert!((scaled[0].clip_pos[0] - 4.0).abs() < 1e-4);
    assert!((scaled[0].clip_size[1] - 32.0).abs() < 1e-4);
}

#[test]
fn textured_overflow_matches_integer_text_and_scroll_clip() {
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        for clip in [[8.0, 341.0, 239.0, 10.0], [8.0, 351.1, 239.0, 0.1]] {
            let draw = TexturedDraw::clipped([16.0, 349.0, 16.0, 16.0], None, "icon", 1.0)
                .unwrap()
                .with_overflow_clip(Some(clip));
            let scaled = scale_textured_draws(&[draw], scale);
            let expected = super::prepare_text::integerize_clip_rect(clip.map(|v| v * scale))
                .unwrap_or([0.0; 4]);
            assert_eq!(scaled[0].clip_pos, [expected[0], expected[1]]);
            assert_eq!(scaled[0].clip_size, [expected[2], expected[3]]);
        }
    }
}

#[test]
fn gpu_overflow_clip_axis_aligned_matches_world_aabb() {
    let rect = [10.0, 20.0, 80.0, 40.0];
    let clip = [10.0, 20.0, 80.0, 40.0];
    let id = crate::style::MAT2D_IDENTITY;
    assert!(super::overflow_clip_contains_after_affine_inv(
        [clip[0], clip[1]],
        [clip[2], clip[3]],
        [rect[0], rect[1]],
        [rect[2], rect[3]],
        &id,
        [50.0, 40.0],
    ));
    assert!(!super::overflow_clip_contains_after_affine_inv(
        [clip[0], clip[1]],
        [clip[2], clip[3]],
        [rect[0], rect[1]],
        [rect[2], rect[3]],
        &id,
        [9.0, 40.0],
    ));
}

#[test]
fn gpu_overflow_clip_rotated_textured_uses_world_aabb() {
    let clip = [0.0, 0.0, 80.0, 80.0];
    let rot = crate::style::mat2d_linear(&crate::style::mat2d_rotate(std::f32::consts::FRAC_PI_4));
    let c = [40.0, 40.0];
    let d = [4.0 - c[0], 4.0 - c[1]];
    let (rx, ry) = crate::style::mat2d_transform_vec(&rot, d[0], d[1]);
    let tip = [c[0] + rx, c[1] + ry];
    assert!(
        super::overflow_clip_contains_world([clip[0], clip[1]], [clip[2], clip[3]], c),
        "world AABB must keep the box center"
    );
    assert!(
        !super::overflow_clip_contains_world([clip[0], clip[1]], [clip[2], clip[3]], tip),
        "diamond vertex outside the unrotated restrict box must discard, tip={tip:?}"
    );
    assert!(
        !super::overflow_clip_contains_world([clip[0], clip[1]], [clip[2], clip[3]], [80.0, 0.0],),
        "unrotated AABB corner must discard"
    );
    assert!(
        tip[1] < 0.0 || tip[0] < 0.0,
        "vacuous unless the vertex leaves the layout AABB, tip={tip:?}"
    );
}
