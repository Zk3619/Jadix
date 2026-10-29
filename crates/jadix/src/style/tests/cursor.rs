use super::*;

#[test]
fn parse_keywords_and_url_fallback() {
    assert_eq!(CursorKind::parse_css("pointer"), Some(CursorKind::Pointer));
    assert_eq!(
        CursorKind::parse_css("url(hand.cur), pointer"),
        Some(CursorKind::Pointer)
    );
    assert_eq!(
        CursorKind::parse_css("url(a.png) 4 12, auto"),
        Some(CursorKind::Auto)
    );
    assert_eq!(CursorKind::parse_css("inherit"), None);
    assert_eq!(CursorKind::parse_css("initial"), Some(CursorKind::Auto));
    assert_eq!(
        CursorKind::parse_keyword("nwse_resize"),
        Some(CursorKind::NwseResize)
    );
    for kw in CursorKind::ALL_KEYWORDS {
        assert!(
            CursorKind::parse_keyword(kw).is_some(),
            "missing keyword {kw}"
        );
    }
    assert_eq!(CursorKind::None.to_winit(), None);
    assert_eq!(CursorKind::Auto.to_winit(), Some(CursorIcon::Default));
    assert_eq!(CursorKind::Pointer.to_winit(), Some(CursorIcon::Pointer));
}

#[test]
fn parse_url_list_hotspot_and_data_comma() {
    let v = CursorValue::parse_css("url(hand.cur), pointer").unwrap();
    assert_eq!(v.images.len(), 1);
    assert_eq!(v.images[0].src, "hand.cur");
    assert_eq!(v.images[0].hotspot, None);
    assert_eq!(v.kind, CursorKind::Pointer);

    let v = CursorValue::parse_css("url(a.png) 4 12, auto").unwrap();
    assert_eq!(v.images[0].hotspot, Some((4, 12)));
    assert_eq!(v.kind, CursorKind::Auto);

    let v = CursorValue::parse_css(r#"url("a.png") 4px 12px, url(b.ico) 1 2, wait"#).unwrap();
    assert_eq!(v.images.len(), 2);
    assert_eq!(v.images[0].hotspot, Some((4, 12)));
    assert_eq!(v.images[1].src, "b.ico");
    assert_eq!(v.kind, CursorKind::Wait);

    let data = "url(data:image/png;base64,iVBOR,w0KGgo), crosshair";
    let v = CursorValue::parse_css(data).unwrap();
    assert_eq!(v.images[0].src, "data:image/png;base64,iVBOR,w0KGgo");
    assert_eq!(v.kind, CursorKind::Crosshair);

    assert!(CursorValue::parse_css("inherit").is_none());
    assert_eq!(
        CursorValue::parse_css("initial").unwrap().kind,
        CursorKind::Auto
    );
}
