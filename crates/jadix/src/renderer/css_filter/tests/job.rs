use super::*;
use crate::renderer::effect::{take_capture_jobs, take_effect_jobs};
use crate::style::parse_filter;

#[test]
fn queue_css_filter_wrap_emits_capture_and_filter() {
    let _ = take_filter_jobs();
    let _ = take_capture_jobs();
    let _ = take_effect_jobs();
    let list = parse_filter("blur(4px)").unwrap();
    let out = queue_css_filter_wrap(&list, [10.0, 20.0, 64.0, 32.0], Vec::new(), Vec::new());
    assert!(out.unwrap().starts_with("jadix_filter:"));
    let caps = take_capture_jobs();
    assert_eq!(caps.len(), 1);
    assert!(caps[0].key.starts_with("jadix_filter_cap:"));
    let jobs = take_filter_jobs();
    assert_eq!(jobs.len(), 1);
    assert!(!jobs[0].graph.primitives.is_empty());
}

#[test]
#[cfg(feature = "compute")]
fn css_blur_enriches_compute_pass() {
    let _ = take_filter_jobs();
    let _ = take_capture_jobs();
    let list = parse_filter("blur(4px)").unwrap();
    queue_css_filter_wrap(&list, [0.0, 0.0, 32.0, 16.0], Vec::new(), Vec::new());
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_filter_graph(&mut graph);
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.kind == crate::renderer::PassKind::Compute),
        "CSS blur must dump PassKind::Compute"
    );
    let _ = take_filter_jobs();
    let _ = take_capture_jobs();
}

#[test]
fn image_only_filters_share_source_capture_but_keep_distinct_outputs() {
    let _ = take_filter_jobs();
    let _ = take_capture_jobs();
    let bounds = [10.0, 20.0, 64.0, 32.0];
    let image = TexturedDraw::clipped(bounds, None, "logo", 1.0).unwrap();
    for css in ["grayscale(1)", "brightness(0.5)"] {
        queue_css_filter_wrap(
            &parse_filter(css).unwrap(),
            bounds,
            vec![],
            vec![image.clone()],
        );
    }
    let captures = take_capture_jobs();
    let filters = take_filter_jobs();
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].key, captures[1].key);
    assert_eq!(filters[0].input_key, filters[1].input_key);
    assert_ne!(filters[0].output_key, filters[1].output_key);

    let empty = FilterList::new();
    let original = filter_cache_id(&empty, bounds, &[], std::slice::from_ref(&image));
    let mut changed = image.clone();
    changed.tint_rgb[0] = 0.5;
    assert_ne!(original, filter_cache_id(&empty, bounds, &[], &[changed]));
    let mut changed = image.clone();
    changed.uv_scale[0] = 0.5;
    assert_ne!(original, filter_cache_id(&empty, bounds, &[], &[changed]));
    let mut changed = image;
    changed.transform[0] = 0.5;
    assert_ne!(original, filter_cache_id(&empty, bounds, &[], &[changed]));
    assert_ne!(
        original,
        filter_cache_id(
            &empty,
            [10.0, 20.0, 65.0, 32.0],
            &[],
            &captures[0].textured_draws
        )
    );
}

#[test]
fn cache_id_ignores_screen_origin() {
    let list = parse_filter("grayscale(1)").unwrap();
    let a = filter_cache_id(&list, [10.0, 20.0, 64.0, 32.0], &[], &[]);
    let b = filter_cache_id(&list, [100.0, 200.0, 64.0, 32.0], &[], &[]);
    assert_eq!(a, b, "scroll must not change filter cache id");
    let c = filter_cache_id(&list, [10.0, 20.0, 65.0, 32.0], &[], &[]);
    assert_ne!(a, c, "size change must bust cache");
}

#[test]
fn cache_id_survives_fractional_scroll_with_painted_content() {
    let list = parse_filter("blur(4px)").unwrap();
    let mut instance = <InstanceInput as bytemuck::Zeroable>::zeroed();
    instance.rect_pos = [17.2, 32.2];
    instance.rect_size = [48.0, 16.0];
    let draw = TexturedDraw::clipped([17.2, 32.2, 48.0, 16.0], None, "stable-svg", 1.0).unwrap();
    let bounds = [10.0, 20.0, 64.0, 32.0];
    let original = filter_cache_id(&list, bounds, &[instance], std::slice::from_ref(&draw));
    for scroll in [0.1, 240.3, 4096.7, 12000.3] {
        let mut shifted = instance;
        shifted.rect_pos[1] -= scroll;
        let mut image = draw.clone();
        image.rect[1] -= scroll;
        let mut region = bounds;
        region[1] -= scroll;
        assert_eq!(
            original,
            filter_cache_id(&list, region, &[shifted], &[image]),
            "unchanged subtree must reuse filter pixels after scroll={scroll}"
        );
    }
    instance.rect_pos[1] += 0.25;
    assert_ne!(
        original,
        filter_cache_id(&list, bounds, &[instance], &[draw])
    );
}
