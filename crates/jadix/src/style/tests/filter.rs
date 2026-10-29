use super::*;

#[test]
fn parse_none() {
    assert!(parse_filter("none").is_none());
    assert!(parse_filter("").is_none());
}

#[test]
fn parse_blur() {
    let list = parse_filter("blur(5px)").unwrap();
    assert_eq!(list.len(), 1);
    assert!(
        matches!(&list[0], FilterFunction::Blur(Some(CssDim::Px(v))) if (*v - 5.0).abs() < 1e-6)
    );
    assert!((filter_list_paint_outset(&list, 80.0, 80.0) - 15.0).abs() < 1e-6);
}

#[test]
fn parse_brightness_percent_and_number() {
    let a = parse_filter("brightness(0.4)").unwrap();
    assert!(matches!(&a[0], FilterFunction::Brightness(Some(v)) if (*v - 0.4).abs() < 1e-6));
    let b = parse_filter("brightness(200%)").unwrap();
    assert!(matches!(&b[0], FilterFunction::Brightness(Some(v)) if (*v - 2.0).abs() < 1e-6));
}

#[test]
fn parse_multi() {
    let list = parse_filter("contrast(175%) brightness(1.03)").unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn parse_drop_shadow() {
    let list = parse_filter("drop-shadow(16px 16px 20px red)").unwrap();
    assert!(matches!(
        &list[0],
        FilterFunction::DropShadow {
            color: Some(_),
            offset_x: CssDim::Px(ox),
            offset_y: CssDim::Px(oy),
            blur: Some(CssDim::Px(b)),
        } if (*ox - 16.0).abs() < 1e-6 && (*oy - 16.0).abs() < 1e-6 && (*b - 20.0).abs() < 1e-6
    ));
}

#[test]
fn parse_url() {
    let list = parse_filter("url(\"filters.svg#filter-id\")").unwrap();
    assert!(matches!(&list[0], FilterFunction::Url(u) if u == "filters.svg#filter-id"));
    let list2 = parse_filter("url(#blurMe)").unwrap();
    assert!(matches!(&list2[0], FilterFunction::Url(u) if u == "#blurMe"));
}

#[test]
fn invalid_arg_yields_none() {
    assert!(parse_filter("blur(50%)").is_none());
    assert!(parse_filter("nope(1)").is_none());
}

#[test]
fn hue_rotate_and_grayscale() {
    let list = parse_filter("hue-rotate(90deg) grayscale(80%)").unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        matches!(&list[0], FilterFunction::HueRotate(Some(a)) if (a.to_deg() - 90.0).abs() < 1e-6)
    );
    assert!(matches!(&list[1], FilterFunction::Grayscale(Some(v)) if (*v - 0.8).abs() < 1e-6));
}
