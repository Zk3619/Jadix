use super::*;
use crate::style::parse_filter;

#[test]
fn lower_blur_grayscale() {
    let list = parse_filter("blur(5px) grayscale(0.4)").unwrap();
    let lowered = lower_css_filters(&list);
    assert_eq!(lowered.segments.len(), 1);
    if let LowerSegment::Graph(g) = &lowered.segments[0] {
        assert_eq!(g.primitives.len(), 2);
        assert!(matches!(
            &g.primitives[0],
            FilterPrimitive::GaussianBlur { .. }
        ));
        assert!(matches!(
            &g.primitives[1],
            FilterPrimitive::ColorMatrix { .. }
        ));
    } else {
        panic!("expected graph");
    }
}

#[test]
fn lower_url_splits() {
    let list = parse_filter("url(#x) brightness(1.2)").unwrap();
    let lowered = lower_css_filters(&list);
    assert!(matches!(&lowered.segments[0], LowerSegment::Url(u) if u == "#x"));
    assert!(matches!(&lowered.segments[1], LowerSegment::Graph(_)));
}
