use super::BackdropBlurPipeline;

#[test]
fn identity_when_src_matches_dst() {
    assert_eq!(
        BackdropBlurPipeline::map_scissor((100, 80, 220, 140), 800, 600, 800, 600),
        Some((100, 80, 220, 140))
    );
}

#[test]
fn halves_when_work_rt_is_half_res() {
    assert_eq!(
        BackdropBlurPipeline::map_scissor((100, 80, 220, 140), 800, 600, 400, 300),
        Some((50, 40, 110, 70))
    );
}

#[test]
fn clamps_to_dst_bounds() {
    assert_eq!(
        BackdropBlurPipeline::map_scissor((700, 500, 200, 200), 800, 600, 800, 600),
        Some((700, 500, 100, 100))
    );
}

#[test]
fn local_capture_pad_covers_each_gaussian_pass() {
    assert_eq!(BackdropBlurPipeline::local_capture_pad_px(2.0), 10);
    assert_eq!(BackdropBlurPipeline::local_capture_pad_px(12.0), 62);
    assert_eq!(BackdropBlurPipeline::local_capture_pad_px(20.0), 106);
}
