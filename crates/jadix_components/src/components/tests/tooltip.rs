use super::*;

#[test]
fn tooltip_builds_tree() {
    let _el: Element = tooltip("tip1", "Hello").into();
}

#[test]
fn tooltip_side_maps() {
    assert_eq!(tooltip_side_to_placement("top"), PopupPlacement::TopCenter);
    assert_eq!(
        tooltip_side_to_placement("bottom"),
        PopupPlacement::BottomCenter
    );
    assert_eq!(
        tooltip_side_to_placement("bottom-start"),
        PopupPlacement::BottomStart
    );
}

#[test]
fn tooltip_content_anchor_is_zero_sized() {
    use crate::core::reactivity::create_root;
    let (_owner, el) = create_root(|| {
        tooltip_scope("tip-anchor", None, || {
            tooltip_content(TooltipContentOpts::default(), || Element::new().text("hi"))
        })
    });
    fn find_zero(el: &Element) -> bool {
        let is_view = el.template_id.as_deref() == Some("tip-anchor__view");
        let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
            && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
        (is_view && dim0) || el.children.iter().any(find_zero)
    }
    assert!(
        find_zero(&el),
        "TooltipContent must use a 0×0 portal anchor"
    );
}
