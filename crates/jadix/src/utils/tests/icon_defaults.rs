use super::*;
use crate::core::tree::PointerEvents;

#[test]
fn icon_defaults_to_text_foreground() {
    let el = build_icon_text_node("\u{ea0e}");
    let fg = parse_text_color("foreground");
    assert_eq!(el.text_style.color_resolved(), fg);
    assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
    assert_eq!(el.pointer_events_mode(), PointerEvents::BoundingBox);
    assert!(el.pointer_events_mode().is_svg_only());
    assert!(el.pointer_events_mode().targets_self());
}

#[cfg(feature = "product-tests")]
#[test]
fn lucide_named_icon_sets_attr_from_name_only() {
    let el = build_lucide_icon_text_node("chevron-down");
    assert_eq!(
        el.get_attr(crate::utils::ATTR_ICON_NAME)
            .map(String::as_str),
        Some("chevron-down")
    );
    assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
    assert!(el.text.as_deref().is_some_and(|t| t != "▾"));
}

#[test]
fn lucide_miss_fallback_is_not_lucide_family() {
    let el = build_lucide_icon_text_node_or("__no_such_lucide_icon__", "▾");
    assert!(el.get_attr(crate::utils::ATTR_ICON_NAME).is_none());
    assert_ne!(el.text_style.font_family.as_deref(), Some("lucide"));
    assert_eq!(el.text.as_deref(), Some("▾"));
}
