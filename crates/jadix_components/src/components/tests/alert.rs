use super::*;
use crate::style::parse_color;

#[test]
fn alert_root_slot_role_and_variant() {
    let el = Alert(AlertProps {
        id: String::new(),
        variant: Some(AlertVariant::Warning),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("warning")
    );
    assert_eq!(
        el.attrs.get("accesskit_role").map(String::as_str),
        Some("alert")
    );
}

#[test]
fn alert_destructive_unfilled_uses_card_surface() {
    let el = build_alert_root(AlertVariant::Destructive, false, true, vec![]);
    assert_eq!(el.style.bg_color, parse_color("card"));
    let border = el.style.border_color;
    assert_eq!(border, parse_color("destructive/50"));
    assert_eq!(
        el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
        Some("destructive")
    );
}

#[test]
fn alert_info_filled_soft_tint() {
    let el = build_alert_root(AlertVariant::Info, true, true, vec![]);
    assert_ne!(el.style.bg_color, parse_color("card"));
}

#[test]
fn alert_info_unfilled_uses_semantic_border() {
    let el = build_alert_root(AlertVariant::Info, false, true, vec![]);
    assert_eq!(el.style.bg_color, parse_color("card"));
    assert_eq!(el.style.border_color, parse_color("info/50"));
    assert!(el.style.border_width > 0.5);
}

#[test]
fn alert_title_and_description_slots() {
    let title = AlertTitle(AlertTitleProps {
        children: Some(Element::new().text("T")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let desc = AlertDescription(AlertDescriptionProps {
        children: Some(Element::new().text("D")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        title.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_TITLE)
    );
    assert_eq!(
        desc.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_DESCRIPTION)
    );
}

#[test]
fn apply_bound_alert_variant_updates_root() {
    let mut el = build_alert_root(AlertVariant::Default, false, true, vec![]);
    assert_eq!(
        apply_bound_alert_variant(&mut el, "success"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
        Some("success")
    );
    assert_eq!(
        apply_bound_alert_variant(&mut el, "success"),
        LayoutStatus::Clean
    );
}

#[test]
fn apply_alert_variant_preserves_user_bg_override() {
    use crate::style::{mark_variant_style_override, parse_color, VariantStyleField};

    let mut el = build_alert_root(AlertVariant::Info, false, true, vec![]);
    let custom = parse_color("red-500");
    el.style.bg_color = custom;
    mark_variant_style_override(&mut el, VariantStyleField::Bg);
    apply_alert_variant_paint(&mut el, "warning");
    assert_eq!(el.style.bg_color, custom);
    assert_eq!(
        el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
        Some("warning")
    );
}

#[test]
fn chrome_false_skips_repaint_on_variant_change() {
    let mut el = build_alert_root(AlertVariant::Info, false, false, vec![]);
    let bg = el.style.bg_color;
    let border = el.style.border_color;
    apply_alert_variant_paint(&mut el, "destructive");
    assert_eq!(el.style.bg_color, bg);
    assert_eq!(el.style.border_color, border);
    assert_eq!(
        el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
        Some("destructive")
    );
}

#[test]
fn alert_title_slot_text_uses_variant_fg() {
    use crate::style::parse_text_color;

    let title = AlertTitle(AlertTitleProps {
        children: Some(Element::new().text("Error")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let el = build_alert_root(AlertVariant::Destructive, false, true, vec![title]);
    let want = alert_chrome(AlertVariant::Destructive, false).fg;
    let mut found = false;
    fn walk(el: &Element, want: [u8; 4], found: &mut bool) {
        if el.text.as_deref() == Some("Error") {
            assert_eq!(el.text_style.color_resolved(), want);
            *found = true;
        }
        for child in &el.children {
            walk(child, want, found);
        }
    }
    walk(&el, want, &mut found);
    assert!(found, "title text node missing");
    assert_eq!(want, parse_text_color("destructive"));
}

#[test]
fn alert_title_text_survives_layout_cascade() {
    use crate::layout::layout_tree::cascade_inheritable_text_style_for_test;
    use crate::style::parse_text_color;

    let title = AlertTitle(AlertTitleProps {
        children: Some(Element::new().text("Heads up!")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let desc = AlertDescription(AlertDescriptionProps {
        children: Some(Element::new().text("Details")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let mut el = build_alert_root(AlertVariant::Info, false, true, vec![title, desc]);
    let title_fg = alert_chrome(AlertVariant::Info, false).fg;
    let desc_fg = alert_chrome(AlertVariant::Info, false).muted_fg;

    cascade_inheritable_text_style_for_test(&mut el);

    let mut title_ok = false;
    let mut desc_ok = false;
    fn walk(
        el: &Element,
        title_fg: [u8; 4],
        desc_fg: [u8; 4],
        title_ok: &mut bool,
        desc_ok: &mut bool,
    ) {
        match el.text.as_deref() {
            Some("Heads up!") => {
                assert_eq!(el.text_style.color_resolved(), title_fg);
                assert_ne!(el.text_style.color_resolved(), [255, 255, 255, 255]);
                *title_ok = true;
            }
            Some("Details") => {
                assert_eq!(el.text_style.color_resolved(), desc_fg);
                *desc_ok = true;
            }
            _ => {}
        }
        for child in &el.children {
            walk(child, title_fg, desc_fg, title_ok, desc_ok);
        }
    }
    walk(&el, title_fg, desc_fg, &mut title_ok, &mut desc_ok);
    assert!(title_ok, "title text missing after cascade");
    assert!(desc_ok, "description text missing after cascade");
    assert_ne!(title_fg, parse_text_color("background"));
}

#[test]
fn alert_title_fg_override_preserved_on_variant_change() {
    use crate::style::{mark_variant_style_override, parse_text_color, VariantStyleField};

    let custom = parse_text_color("blue-500");
    let mut title = AlertTitle(AlertTitleProps {
        children: Some(Element::new().text("Custom")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    mark_variant_style_override(&mut title, VariantStyleField::Fg);
    fn paint_text(el: &mut Element, c: [u8; 4]) {
        if el.text.is_some() {
            el.text_style.color = Some(c);
            return;
        }
        for child in &mut el.children {
            paint_text(child, c);
        }
    }
    paint_text(&mut title, custom);

    let mut el = build_alert_root(AlertVariant::Info, false, true, vec![title]);
    apply_alert_variant_paint(&mut el, "destructive");

    let mut found = false;
    fn walk(el: &Element, want: [u8; 4], found: &mut bool) {
        if el.text.as_deref() == Some("Custom") {
            assert_eq!(el.text_style.color_resolved(), want);
            *found = true;
        }
        for child in &el.children {
            walk(child, want, found);
        }
    }
    walk(&el, custom, &mut found);
    assert!(found);
}

#[test]
fn alert_class_bg_preserved_on_variant_change() {
    use crate::style::{
        apply_class_list, is_variant_style_overridden, parse_color, VariantStyleField,
    };

    let mut el = build_alert_root(AlertVariant::Info, false, true, vec![]);
    let custom = parse_color("red-500");
    apply_class_list(&mut el, "bg-red-500");
    assert_eq!(el.style.bg_color, custom);
    assert!(is_variant_style_overridden(
        &el.attrs,
        VariantStyleField::Bg
    ));
    apply_alert_variant_paint(&mut el, "warning");
    assert_eq!(el.style.bg_color, custom);
}

#[test]
fn builder_assembles_icon_title_description() {
    let mut icon = Element::new().text("!");
    icon.text_style.font_family = Some("lucide".into());
    let el: Element = alert()
        .variant("info")
        .icon(icon)
        .title("Heads up")
        .description("Details here.")
        .into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    let body = &el.children[0];
    assert!(body.children.len() >= 2);
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = alert().variant("warning").title("X").into();
    let el = Alert(AlertProps {
        id: String::new(),
        variant: Some(AlertVariant::Default),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let _: Element = el;
}
