use super::*;
use crate::layout::layout_tree::LayoutTree;

#[test]
fn badge_variants_set_slot_and_paint_key() {
    for (name, _) in [
        ("default", BadgeVariant::Default),
        ("secondary", BadgeVariant::Secondary),
        ("success", BadgeVariant::Success),
        ("warning", BadgeVariant::Warning),
        ("destructive", BadgeVariant::Destructive),
        ("outline", BadgeVariant::Outline),
        ("ghost", BadgeVariant::Ghost),
        ("link", BadgeVariant::Link),
    ] {
        let el: Element = badge().variant(name).text(name).into();
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_BADGE)
        );
        assert_eq!(el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()), Some(name));
        assert!(
            el.children.iter().any(|c| c.text.as_deref() == Some(name)),
            "caption leaf missing for {name}"
        );
    }
}

#[test]
fn badge_warn_and_error_aliases_normalize() {
    let warn: Element = badge().variant("warn").text("w").into();
    assert_eq!(
        warn.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
        Some("warning")
    );
    let err: Element = badge().variant("error").text("e").into();
    assert_eq!(
        err.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
        Some("destructive")
    );
    assert_eq!(
        "warn".parse::<BadgeVariant>().unwrap(),
        BadgeVariant::Warning
    );
    assert_eq!(
        "error".parse::<BadgeVariant>().unwrap(),
        BadgeVariant::Destructive
    );
}

#[test]
fn badge_empty_id_does_not_write_empty_template() {
    let el: Element = badge().text("x").into();
    assert!(el.template_id.is_none() || el.template_id.as_deref() != Some(""));
}

#[test]
fn badge_default_height_has_vertical_breathing_room() {
    let mut el: Element = badge().text("Hi").into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 400.0, 80.0, [-1.0, -1.0]);
    let h = el.layout_bounds[3];
    let text_h = el.children[0].layout_bounds[3];
    // text-xs default leading (~16px) + py (0.1875rem × 2) ≈ 22px; must exceed tight 1.0 leading.
    assert!(
        h >= 20.0,
        "badge layout height should be at least 20px, got {h:.2} (text leaf {text_h:.2})"
    );
    assert!(
        text_h >= 14.0,
        "badge text leaf should use Tailwind text-xs leading, got {text_h:.2}"
    );
}

#[cfg(feature = "theme-fluent")]
#[test]
fn fluent_badge_uses_compact_caption_metrics() {
    use crate::style::{set_theme_preset, ThemePreset};

    let previous = crate::style::get_theme();
    set_theme_preset(ThemePreset::Fluent);
    let el: Element = badge().text("Default").into();
    let caption = el
        .children
        .iter()
        .find(|child| child.text.is_some())
        .unwrap();
    assert_eq!(caption.text_style.size, Some(12.0));
    assert_eq!(caption.text_style.line_height_px, Some(16.0));
    crate::style::set_theme(previous);
}

#[test]
fn badge_content_sized_shrinks_with_shorter_label() {
    use crate::primitives::r#box::hstack;
    let mut long: Element = hstack()
        .items_center()
        .child(badge().text("Blocked"))
        .into();
    let mut short: Element = hstack().items_center().child(badge().text("Ready")).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut long, 400.0, 80.0, [-1.0, -1.0]);
    let _ = tree.build_and_layout(&mut short, 400.0, 80.0, [-1.0, -1.0]);
    let lw = long.children[0].layout_bounds[2];
    let sw = short.children[0].layout_bounds[2];
    assert!(
        lw > sw + 4.0,
        "Blocked ({lw:.1}) should be wider than Ready ({sw:.1})"
    );
}

#[test]
fn badge_unpacks_macro_vstack_children_and_paints_variant_color() {
    use crate::primitives::r#box::vstack;
    use crate::utils::text_node::build_icon_text_node;

    // Simulate ui! multi-child pack: Icon + caption wrapped in a VStack.
    let packed = Element::from(vstack().children(vec![
        build_icon_text_node("✓").text_size(12.0),
        Element::new().text("Verified"),
    ]));
    let el: Element = Badge(BadgeProps {
        variant: Some(BadgeVariant::Secondary),
        children: Some(packed),
        ..Default::default()
    });
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
        Some("secondary")
    );
    assert_eq!(
        el.children.len(),
        2,
        "icon+caption must be hoisted into the badge row, not left as a nested VStack"
    );
    let want = parse_text_color("secondary-foreground");
    for (i, c) in el.children.iter().enumerate() {
        assert!(
            c.text.as_ref().is_some_and(|t| !t.is_empty()),
            "child {i} must be a text/icon leaf"
        );
        assert_eq!(
            c.text_style.color_resolved(),
            want,
            "child {i} must use secondary-foreground, not default white"
        );
    }
    assert!(
        el.children
            .iter()
            .any(|c| c.text.as_deref() == Some("Verified")),
        "caption must survive unpack"
    );
}

#[test]
fn apply_bound_badge_variant_updates_chrome() {
    let mut el: Element = badge().variant("default").text("Hi").into();
    let status = apply_bound_badge_variant(&mut el, "outline");
    assert_ne!(status, LayoutStatus::Clean);
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
        Some("outline")
    );
    assert!(el.style.border_width > 0.5);
    let again = apply_bound_badge_variant(&mut el, "outline");
    assert_eq!(again, LayoutStatus::Clean);
}

#[test]
fn semantic_badge_soft_tint_matches_button_rest() {
    use crate::style::{set_theme_mode, ThemeMode};
    set_theme_mode(ThemeMode::Light);
    for (name, accent_token, text_token) in [
        ("success", "success", "success"),
        ("warning", "warning", "warning"),
        ("destructive", "destructive", "destructive"),
    ] {
        let el: Element = badge().variant(name).text(name).into();
        let accent = parse_color(accent_token);
        let bg = parse_color("background");
        let want_bg = [
            accent[0] * 0.1 + bg[0] * 0.9,
            accent[1] * 0.1 + bg[1] * 0.9,
            accent[2] * 0.1 + bg[2] * 0.9,
            1.0,
        ];
        assert!(
            (0..4).all(|i| (el.style.bg_color[i] - want_bg[i]).abs() < 1e-4),
            "{name} bg should match button soft rest"
        );
        let want_fg = parse_text_color(text_token);
        let leaf = el
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some(name))
            .expect("caption");
        assert_eq!(leaf.text_style.color_resolved(), want_fg);
    }
}

#[test]
fn builder_style() {
    let el: Element = badge().style(|b| b.text_size(20.0)).text("New").into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "badge .style(|b| b.text_size(20)) must land on the badge"
    );
}
