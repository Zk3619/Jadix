use super::*;
use crate::layout::layout_tree::OverlayBandRange;
use bytemuck::Zeroable;

fn band_instances(lo: usize, hi: usize) -> OverlayBandRange {
    OverlayBandRange {
        instance: (lo, hi),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        backdrop_blur: 0.0,
        underlay_opacity: 1.0,
        stamp_key: 0,
    }
}

#[test]
fn foreground_damage_does_not_invalidate_local_backdrop() {
    use crate::app::damage::{PaintDirty, PaintDirtyKind};
    let band = band_instances(10, 20);
    let node = crate::layout::layout_tree::NodePaint {
        key: 42,
        bounds: [100.0, 200.0, 100.0, 100.0],
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let dirty = PaintDirty {
        key: 42,
        bounds: node.bounds,
        kind: PaintDirtyKind::Node,
    };
    assert!(local_backdrop_source_damage(&dirty, &[node], &band));
    assert!(!local_backdrop_source_damage(
        &dirty,
        &[crate::layout::layout_tree::NodePaint {
            overlay: true,
            ..node
        }],
        &band
    ));
    assert!(!local_backdrop_source_damage(
        &dirty,
        &[crate::layout::layout_tree::NodePaint {
            instance: (20, 21),
            ..node
        }],
        &band
    ));
    for kind in [PaintDirtyKind::Overlay, PaintDirtyKind::OverlayRestore] {
        assert!(!local_backdrop_source_damage(
            &PaintDirty { kind, ..dirty },
            &[],
            &band
        ));
    }
}

#[test]
fn local_blur_radius_is_zero_when_instance_buffer_was_taken() {
    // Regression: `mem::take(render_result.instances)` left an empty vec; looking
    // there for backdrop_blur skipped capture and the scrim sampled the dummy.
    let band = band_instances(0, 4);
    assert_eq!(band_instance_backdrop_blur(&[], &band), 0.0);
}

#[test]
fn local_blur_radius_reads_scaled_instance_buffer() {
    let mut inst = InstanceInput::zeroed();
    inst.backdrop_blur = 16.0;
    inst.rect_size = [100.0, 80.0];
    inst.clip_size = InstanceInput::NO_CLIP_SIZE;
    let band = band_instances(0, 1);
    assert!((band_instance_backdrop_blur(&[inst], &band) - 16.0).abs() < 0.01);
}

#[test]
fn local_frost_still_captures_when_modal_underlay_will_blur() {
    assert!(
        should_capture_page_stack_local_backdrops(true, true),
        "page-stack frost must run even if a ViewportModal underlay will also blur"
    );
    assert!(!should_capture_page_stack_local_backdrops(false, true));
    assert!(!should_capture_page_stack_local_backdrops(true, false));
}

#[test]
fn host_scissor_uses_physical_clip_without_rescaling() {
    let mut inst = InstanceInput::zeroed();
    inst.backdrop_blur = 16.0;
    inst.clip_pos = [100.0, 200.0];
    inst.clip_size = [220.0, 140.0];
    let band = band_instances(0, 1);
    let sc = local_backdrop_host_scissor_physical(&[inst], &band, 800, 600).expect("scissor");
    assert_eq!(sc, (100, 200, 220, 140));
}

#[test]
fn optical_capture_is_local_and_includes_refracted_samples() {
    let mut instance = InstanceInput::zeroed();
    instance.backdrop_blur = 2.0;
    instance.glass = [1.46, 10.0, 0.1, 0.6];
    instance.rect_pos = [100.0, 120.0];
    instance.rect_size = [200.0, 80.0];
    instance.clip_size = [800.0, 600.0];
    let scissor =
        local_backdrop_host_scissor_physical(&[instance], &band_instances(0, 1), 800, 600);
    assert_eq!(scissor, Some((78, 98, 244, 124)));
}

#[test]
fn kernel_pad_covers_the_full_gaussian_support() {
    assert_eq!(local_backdrop_kernel_pad_px(2.0), 10);
    assert_eq!(local_backdrop_kernel_pad_px(12.0), 62);
}

#[test]
fn expand_scissor_clamps_to_surface() {
    let expanded = expand_physical_scissor((10, 10, 20, 20), 8, 800, 600);
    assert_eq!(expanded, (2, 2, 36, 36));
    let edge = expand_physical_scissor((0, 0, 10, 10), 8, 800, 600);
    assert_eq!(edge, (0, 0, 18, 18));
}

#[test]
fn skip_recapture_when_partial_damage_misses_host() {
    let host = Some((100, 200, 220, 140));
    assert!(
        !local_backdrop_must_recapture(true, true, false, host, &[[10.0, 10.0, 40.0, 20.0]], 1.0,),
        "hover elsewhere must keep last frost"
    );
    assert!(
        local_backdrop_must_recapture(true, true, false, host, &[[120.0, 220.0, 10.0, 10.0]], 1.0,),
        "damage on the host must recapture"
    );
    assert!(
        local_backdrop_must_recapture(true, true, true, host, &[], 1.0),
        "scroll blit moves pixels under the scrim"
    );
    assert!(
        local_backdrop_must_recapture(false, true, false, host, &[], 1.0),
        "first frame has no cached frost"
    );
    assert!(
        !local_backdrop_must_recapture(true, true, false, host, &[], 1.0),
        "overlay-only PartialLoad with empty dirty keeps frost"
    );
}
