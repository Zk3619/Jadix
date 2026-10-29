use super::*;

#[test]
fn split_hides_unrevealed_clusters() {
    let fx = GlyphFx::new(GlyphFxKind::Split, 0.4);
    assert!(fx.opacity(0, 10) > 0.9);
    assert!(fx.opacity(9, 10) < 0.1);
}

#[test]
fn parse_aliases() {
    assert_eq!(GlyphFxKind::parse("BlurText"), GlyphFxKind::Blur);
    assert_eq!(GlyphFxKind::parse("split"), GlyphFxKind::Split);
    assert_eq!(GlyphFxKind::parse("shimmer"), GlyphFxKind::Shimmer);
}

#[test]
fn shimmer_band_peaks_under_the_sweep() {
    let mid = shimmer_band_mix(0.5, 0.5, 0.25, false, 20.0);
    let edge = shimmer_band_mix(0.0, 0.5, 0.25, false, 20.0);
    assert!(mid > 0.7, "mid={mid}");
    assert!(edge < mid);
    let rev = shimmer_band_mix(0.1, 0.1, 0.2, true, 0.0);
    let fwd = shimmer_band_mix(0.1, 0.1, 0.2, false, 0.0);
    assert!(rev < fwd);
}

#[test]
fn shimmer_once_clamps_progress() {
    let p = ShimmerParams {
        duration_ms: 1000.0,
        count: Some(1.0),
        epoch_s: 0.0,
        ..Default::default()
    };
    assert!((p.progress(0.5) - 0.5).abs() < 1e-4);
    assert_eq!(p.progress(2.0), 1.0);
    assert!(p.finished(1.0));
}

#[test]
fn shimmer_count_two_cycles_then_settles() {
    let p = ShimmerParams {
        duration_ms: 1000.0,
        count: Some(2.0),
        epoch_s: 0.0,
        ..Default::default()
    };
    assert!((p.progress(0.25) - 0.25).abs() < 1e-4);
    assert!((p.progress(1.25) - 0.25).abs() < 1e-4);
    assert!(!p.finished(1.5));
    assert!(p.finished(2.0));
    assert_eq!(p.progress(2.5), 1.0);
}

#[test]
fn shimmer_box_mix_matches_text_falloff() {
    let params = ShimmerParams {
        duration_ms: 1000.0,
        epoch_s: 0.0,
        angle_deg: 0.0,
        ..Default::default()
    };
    let mid = shimmer_box_mix_at(&params, 0.5, 0.5, 200.0);
    let edge = shimmer_box_mix_at(&params, 0.5, 0.0, 200.0);
    let far = shimmer_box_mix_at(&params, 0.5, 1.0, 200.0);
    assert!(mid > 0.7, "mid-cycle center should peak, got {mid}");
    assert!(
        edge < 0.15,
        "edges must fade like glyph shimmer, got {edge}"
    );
    assert!(far < 0.15, "far edge must fade, got {far}");
    let left = shimmer_box_mix_at(&params, 0.5, 0.35, 200.0);
    let right = shimmer_box_mix_at(&params, 0.5, 0.65, 200.0);
    assert!(
        (left - right).abs() < 0.2,
        "falloff should be a soft bell, left={left} right={right}"
    );
}

#[test]
fn shimmer_box_highlight_is_a_white_sheen() {
    let params = ShimmerParams::default();
    let sheen = shimmer_box_highlight_rgba(&params, crate::style::parse_color("secondary"));
    assert!(
        sheen[0] + sheen[1] + sheen[2] > 2.5,
        "box sheen must lighten like text shimmer, got {sheen:?}"
    );
    assert!(sheen[3] >= 0.2 && sheen[3] <= 0.45, "peak alpha {sheen:?}");
}
