use super::*;

#[test]
fn parse_simple_blur_filter() {
    let markup = r##"
        <filter id="blurMe">
          <feGaussianBlur stdDeviation="5"/>
        </filter>
        "##;
    let graphs = parse_svg_filters_document(markup).unwrap();
    assert_eq!(graphs.len(), 1);
    assert_eq!(graphs[0].id, "blurMe");
    assert!(matches!(
        &graphs[0].primitives[0],
        FilterPrimitive::GaussianBlur { std_deviation: (a, b), .. }
            if (*a - 5.0).abs() < 1e-6 && (*b - 5.0).abs() < 1e-6
    ));
}
