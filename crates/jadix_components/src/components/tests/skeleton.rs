use super::*;

#[test]
fn skeleton_sets_slot_accent_and_pulse() {
    let el: Element = skeleton().into();
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_SKELETON)
    );
    assert_eq!(el.style.bg_color, parse_color("accent"));
    assert!(
        el.attrs
            .get("jadix_animate")
            .map(|s| s.as_str())
            .is_some_and(|v| v == "pulse")
            || el
                .attrs
                .get("jadix_utility_class")
                .map(|s| s.contains("animate-pulse"))
                .unwrap_or(false)
    );
}

#[test]
fn skeleton_empty_id_does_not_write_empty_template() {
    let el: Element = skeleton().into();
    assert!(el.template_id.is_none() || el.template_id.as_deref() != Some(""));
}

#[test]
fn builder_applies_size_and_class() {
    let el: Element = skeleton().w("250px").h("1rem").class("rounded-full").into();
    let bare: Element = skeleton().into();
    assert_ne!(el.style.size.width, bare.style.size.width);
    assert_ne!(el.style.size.height, bare.style.size.height);
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = skeleton().w("100px").h("20px").into();
    let el = Skeleton(SkeletonProps {
        id: String::new(),
        base: None,
        ..Default::default()
    });
    let _: Element = el;
}

#[test]
fn builder_style() {
    let el: Element = skeleton().style(|b| b.text_size(20.0)).into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "skeleton .style(|b| b.text_size(20)) must land on the placeholder"
    );
}
