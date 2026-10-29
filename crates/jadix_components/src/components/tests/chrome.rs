use super::{button, button_chrome_for_variant};
use crate::core::tree::{Element, LayoutStatus};
use crate::style::color_mix_over_background;
use crate::style::{parse_color, parse_text_color, set_theme_mode, ThemeMode};

fn hex(c: [f32; 4]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8
    )
}

fn luminance(c: [f32; 4]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

#[test]
fn scaled_preview_visible_button_updates_hover_below_logical_viewport() {
    crate::core::reactivity::create_root(|| {
        set_theme_mode(ThemeMode::Light);
        let mut button: Element = button("scaled-preview-hover").text("Hover").into();
        button.class_set = None;
        button.style.transition.clear();
        let mut root = Element::new().w(680.0).h(720.0).child(
            Element::new()
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(390.0)
                .h(2500.0)
                .scale(0.25)
                .transform_origin("left top")
                .child(button.absolute().top(2000.0).w(120.0).h(32.0)),
        );
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 680.0, 720.0, [-1.0; 2]);
        let before = root.children[0].children[0].style.bg_color;
        crate::core::reactivity::use_signal("scaled-preview-hover_hover", || false)
            .set_silent(true);
        let mut impact = LayoutStatus::Clean;
        super::sync_button_hover_paint_subtree(
            &mut root,
            &mut layout,
            &mut impact,
            680.0,
            720.0,
            [0.0; 2],
        );
        assert_ne!(
            root.children[0].children[0].style.bg_color, before,
            "a visually visible scaled button must receive hover paint"
        );
    });
}

#[test]
fn primary_matches_docs_black_and_hover_80() {
    set_theme_mode(ThemeMode::Light);
    let (rest, _, _) = button_chrome_for_variant("default", 0.0);
    let (hover, _, _) = button_chrome_for_variant("default", 1.0);
    assert_eq!(hex(rest), "#000000");
    assert_eq!(hex(hover), "#333333");
}

#[test]
fn hover_does_not_toggle_border_presence() {
    // 0↔1px border is a Taffy box change. Hover must stay paint-only.
    set_theme_mode(ThemeMode::Light);
    for variant in [
        "default",
        "secondary",
        "outline",
        "ghost",
        "link",
        "destructive",
        "success",
        "warning",
    ] {
        let (_, _, rest) = button_chrome_for_variant(variant, 0.0);
        let (_, _, hover) = button_chrome_for_variant(variant, 1.0);
        assert_eq!(
            rest.is_some(),
            hover.is_some(),
            "{variant}: rest border={} hover border={}",
            rest.is_some(),
            hover.is_some()
        );
    }
}

#[test]
fn secondary_hover_uses_secondary_80() {
    set_theme_mode(ThemeMode::Light);
    let (rest, _, _) = button_chrome_for_variant("secondary", 0.0);
    let (hover, fg, _) = button_chrome_for_variant("secondary", 1.0);
    assert_eq!(fg, parse_text_color("secondary-foreground"));
    assert_eq!(hex(rest), "#F5F5F5");
    assert_eq!(hex(hover), hex(parse_color("secondary/80")));
}

#[test]
fn ghost_rest_keeps_hover_rgb_at_zero_alpha() {
    set_theme_mode(ThemeMode::Light);
    let (rest, rest_fg, _) = button_chrome_for_variant("ghost", 0.0);
    let (hover, hover_fg, _) = button_chrome_for_variant("ghost", 1.0);
    assert!(
        (rest[0] - hover[0]).abs() < 1e-5
            && (rest[1] - hover[1]).abs() < 1e-5
            && (rest[2] - hover[2]).abs() < 1e-5,
        "ghost rest must share hover RGB so CSS alpha-only fade stays clean"
    );
    assert!(rest[3] < 0.01);
    assert!(hover[3] > 0.9);
    assert_eq!(rest_fg, parse_text_color("foreground"));
    assert_eq!(hover_fg, rest_fg, "ghost hover keeps caption color");
}

#[test]
fn explicit_text_color_overrides_ghost_chrome() {
    use super::button;
    use crate::core::reactivity::create_root;
    use crate::core::tree::Element;
    let muted = parse_text_color("muted-foreground");
    let (_h, el) = create_root(|| -> Element {
        button("t-btn-muted")
            .variant("ghost")
            .text("26")
            .text_color(muted)
            .into()
    });
    let fg = el
        .children
        .iter()
        .find_map(|c| c.text.as_ref().map(|_| c.text_style.color_resolved()));
    assert_eq!(fg, Some(muted));
    assert_eq!(
        el.authoring_state
            .get::<crate::components::button::ButtonHoverPaint>()
            .and_then(|p| p.text_color),
        Some(muted)
    );
}

#[test]
fn destructive_soft_tint_with_opaque_destructive_text() {
    set_theme_mode(ThemeMode::Light);
    let destructive = parse_color("destructive");
    let (rest, fg, _) = button_chrome_for_variant("destructive", 0.0);
    let (hover, hover_fg, _) = button_chrome_for_variant("destructive", 1.0);
    assert_eq!(fg, parse_text_color("destructive"));
    assert_eq!(hover_fg, parse_text_color("destructive"));
    assert_eq!(fg[3], 255);
    assert_eq!(hex(rest), hex(color_mix_over_background(destructive, 0.1)));
    assert_eq!(hex(hover), hex(color_mix_over_background(destructive, 0.2)));
    assert!(luminance(hover) < luminance(rest));
}

#[test]
fn success_and_warning_soft_tint_match_toast_accents() {
    set_theme_mode(ThemeMode::Light);
    let green = parse_color("success");
    let orange = parse_color("warning");
    let (s_rest, s_fg, _) = button_chrome_for_variant("success", 0.0);
    let (s_hover, _, _) = button_chrome_for_variant("success", 1.0);
    assert_eq!(s_fg, parse_text_color("success"));
    assert_eq!(s_fg, parse_text_color("green-600"));
    assert_eq!(hex(s_rest), hex(color_mix_over_background(green, 0.1)));
    assert_eq!(hex(s_hover), hex(color_mix_over_background(green, 0.2)));
    let (w_rest, w_fg, _) = button_chrome_for_variant("warning", 0.0);
    let (w_hover, _, _) = button_chrome_for_variant("warning", 1.0);
    assert_eq!(w_fg, parse_text_color("warning"));
    assert_eq!(w_fg, parse_text_color("orange-600"));
    assert_eq!(hex(w_rest), hex(color_mix_over_background(orange, 0.1)));
    assert_eq!(hex(w_hover), hex(color_mix_over_background(orange, 0.2)));
    assert_eq!(
        "warn".parse::<super::ButtonVariant>().unwrap(),
        super::ButtonVariant::Warning
    );
    assert_eq!(
        "error".parse::<super::ButtonVariant>().unwrap(),
        super::ButtonVariant::Destructive
    );
}

#[test]
fn inline_size_has_no_padding() {
    use super::button;
    use crate::core::tree::Element;
    use crate::layout::lp_length_px;
    let el: Element = button("t-inline")
        .variant("link")
        .size("inline")
        .text("Home")
        .into();
    assert_eq!(lp_length_px(el.style.padding.left), Some(0.0));
    assert_eq!(lp_length_px(el.style.padding.right), Some(0.0));
}

#[test]
fn link_default_underline_offset_is_4px() {
    let el: Element = button("link-offset").variant("link").text("Go").into();
    assert_eq!(
        el.text_style.underline_offset,
        Some(4.0),
        "shadcn link uses underline-offset-[4px] (not rem-scale 4 = 16px)"
    );
    let caption = el
        .children
        .iter()
        .find(|c| c.text.is_some())
        .expect("caption");
    assert!(
        caption.text_style.underline.is_none(),
        "caption inherits hover:underline from the Button root"
    );
}

#[test]
fn link_author_underline_offset_class_wins() {
    let el: Element = button("link-offset-author")
        .variant("link")
        .class("underline-offset-[8px]")
        .text("Go")
        .into();
    assert_eq!(el.text_style.underline_offset, Some(8.0));
}

#[test]
fn underline_offset_bare_token_is_rem_scale() {
    let el: Element = button("link-offset-rem")
        .variant("link")
        .class("underline-offset-4")
        .text("Go")
        .into();
    let root = crate::layout::dimension::css_root_font_size_px();
    assert!(
        (el.text_style.underline_offset.unwrap_or(0.0) - root).abs() < 0.01,
        "underline-offset-4 = 1rem = {root}px, got {:?}",
        el.text_style.underline_offset
    );
}

#[test]
fn link_hover_underline_is_text_style() {
    use crate::core::reactivity::create_root;
    use crate::style::{resolve_class_variants, VariantContext};

    let (_h, mut el) =
        create_root(|| -> Element { button("t-link-ul").variant("link").text("Go").into() });
    let attrs = el.attrs.clone();
    let ctx = VariantContext {
        hovered: true,
        attrs: Some(&attrs),
        ..VariantContext::default()
    };
    let _ = resolve_class_variants(&mut el, &ctx);
    assert_eq!(el.text_style.underline, Some(true));
    let caption = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Go"))
        .expect("caption");
    assert_eq!(caption.text_style.underline, Some(true));
    assert_eq!(caption.text_style.underline_offset, Some(4.0));
}

#[test]
fn link_explicit_muted_hovers_to_foreground() {
    use super::link_caption_color;
    let muted = parse_text_color("muted-foreground");
    let fg = parse_text_color("foreground");
    assert_eq!(
        link_caption_color(Some(muted), parse_text_color("primary"), false),
        muted
    );
    assert_eq!(
        link_caption_color(Some(muted), parse_text_color("primary"), true),
        fg
    );
}

#[test]
fn unknown_variant_and_size_fall_back_to_default() {
    assert_eq!(
        "not-a-variant".parse::<super::ButtonVariant>().unwrap(),
        super::ButtonVariant::Default
    );
    assert_eq!(
        "xxl".parse::<super::ButtonSize>().unwrap(),
        super::ButtonSize::Default
    );
    let (unknown, ..) = button_chrome_for_variant("nope", 0.0);
    let (default, ..) = button_chrome_for_variant("default", 0.0);
    assert_eq!(hex(unknown), hex(default));
}

#[test]
fn outline_button_unpacks_ui_vstack_and_paints_caption_foreground() {
    use super::{Button, ButtonProps, ButtonVariant};
    use crate::components::kbd::kbd_group;
    use crate::primitives::r#box::vstack;

    // Mimic ui! multi-child pack: anonymous VStack wrapping caption + KbdGroup.
    let packed = Element::from(vstack().children(vec![
        Element::new().text("Search"),
        kbd_group().kbd("⌘").kbd("K").into(),
    ]));
    let el = Button(ButtonProps {
        variant: Some(ButtonVariant::Outline),
        children: Some(packed),
        ..Default::default()
    });
    let want_fg = parse_text_color("foreground");
    let caption = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Search"))
        .expect("Search caption must be hoisted to the button row");
    assert_eq!(
        caption.text_style.color_resolved(),
        want_fg,
        "outline caption must use foreground, not default white"
    );
    let group = el
        .children
        .iter()
        .find(|c| c.attrs.get("data-slot").map(|s| s.as_str()) == Some("kbd-group"))
        .expect("KbdGroup shell must stay intact");
    assert!(
        group.children.iter().all(|k| {
            k.attrs.get("data-slot").map(|s| s.as_str()) == Some("kbd")
                && k.children.iter().any(|leaf| {
                    leaf.text.is_some()
                        && leaf.text_style.color == Some(parse_text_color("muted-foreground"))
                })
        }),
        "Kbd captions must keep muted-foreground"
    );
}

#[test]
fn outline_button_alone_kbd_group_is_not_dismantled() {
    use super::{Button, ButtonProps, ButtonVariant};
    use crate::components::kbd::kbd_group;

    let el = Button(ButtonProps {
        variant: Some(ButtonVariant::Outline),
        children: Some(kbd_group().kbd("⌘").kbd("K").into()),
        ..Default::default()
    });
    assert_eq!(
        el.children.len(),
        1,
        "sole KbdGroup must not be unpacked into raw keycaps"
    );
    assert_eq!(
        el.children[0].attrs.get("data-slot").map(|s| s.as_str()),
        Some("kbd-group")
    );
}

#[test]
fn button_class_hover_applies_primary_80() {
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::LayoutTree;
    use crate::style::sync_class_variants_subtree;
    let (_h, mut el) = create_root(|| -> Element {
        button("t-btn-class-hover")
            .variant("default")
            .text("Go")
            .into()
    });
    el.layout_bounds = [0.0, 0.0, 80.0, 32.0];
    for c in &mut el.children {
        if c.text.is_some() {
            c.layout_bounds = [8.0, 6.0, 64.0, 20.0];
        }
    }
    assert_eq!(hex(el.style.bg_color), hex(parse_color("primary")));
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [40.0, 16.0],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        hex(el.style.bg_color),
        hex(parse_color("primary/80")),
        "Button hover color must resolve via class hover:bg-primary/80"
    );
}

#[test]
fn ghost_hover_keeps_foreground() {
    use crate::core::reactivity::create_root;
    use crate::style::{resolve_class_variants, VariantContext};
    let (_h, mut el) =
        create_root(|| -> Element { button("t-btn-ghost-fg").variant("ghost").text("Go").into() });
    let rest = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Go"))
        .expect("caption")
        .text_style
        .color;
    assert_eq!(rest, Some(parse_text_color("foreground")));
    let attrs = el.attrs.clone();
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    let hover = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Go"))
        .expect("caption")
        .text_style
        .color;
    assert_eq!(
        hover, rest,
        "observed ShadCN ghost Button: hover changes bg only, caption stays foreground"
    );
}

#[test]
fn ghost_classes_declare_rest_foreground() {
    let classes = super::button_variant_color_classes("ghost");
    assert!(
        classes.split_whitespace().any(|t| t == "text-foreground"),
        "ghost rest must set text-foreground so Lucide chrome (min/max/close) \
             is not washed to primary-foreground / white on frameless secondaries: {classes}"
    );
}

#[test]
fn ghost_icon_child_keeps_foreground_after_class_resolve() {
    use super::button;
    use crate::core::reactivity::create_root;
    use crate::style::{resolve_class_variants, VariantContext};
    use crate::utils::text_node::build_lucide_icon_text_node;
    let fg = parse_text_color("foreground");
    let lucide_fg = |el: &Element| {
        el.children
            .iter()
            .find(|c| c.text_style.font_family.as_deref() == Some("lucide"))
            .map(|c| c.text_style.color_resolved())
            .expect("lucide child")
    };
    let (_h, mut el) = create_root(|| -> Element {
        button("t-ghost-icon")
            .variant("ghost")
            .size("icon-xs")
            .child(build_lucide_icon_text_node("minus"))
            .into()
    });
    assert_eq!(lucide_fg(&el), fg);
    assert_ne!(lucide_fg(&el), [255, 255, 255, 255]);
    let attrs = el.attrs.clone();
    let _ = resolve_class_variants(
        &mut el,
        &VariantContext {
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(
        lucide_fg(&el),
        fg,
        "class restore must not paint ghost icons white"
    );
    assert_eq!(el.text_style.color, Some(fg));
}

#[test]
fn ghost_lucide_title_chrome_emits_icon_paint() {
    use super::button;
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::LayoutTree;
    use crate::utils::text_node::build_lucide_icon_text_node;
    let (_h, mut el) = create_root(|| -> Element {
        button("t-ghost-caption-x")
            .variant("ghost")
            .size("icon-xs")
            .child(build_lucide_icon_text_node("x"))
            .into()
    });
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 48.0, 32.0, [0.0, 0.0]);
    assert!(
        result
            .textured_draws
            .iter()
            .any(|d| d.texture_key.starts_with("jadix_lucide")),
        "frameless secondary chrome icons are Lucide textures; \
             secondary windows must upload/draw textured_draws. keys={:?}",
        result
            .textured_draws
            .iter()
            .map(|d| d.texture_key.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn ghost_icon_hover_applies_accent_background() {
    use super::button;
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::LayoutTree;
    use crate::style::sync_class_variants_subtree;
    use crate::utils::text_node::build_lucide_icon_text_node;
    set_theme_mode(ThemeMode::Light);
    let (_h, mut el) = create_root(|| -> Element {
        button("t-ghost-caption-hover")
            .variant("ghost")
            .size("icon-xs")
            .child(build_lucide_icon_text_node("x"))
            .into()
    });
    el.layout_bounds = [0.0, 0.0, 28.0, 28.0];
    assert!(
        el.style.bg_color[3] < 0.01,
        "ghost rest background is transparent"
    );
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [14.0, 14.0],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        hex(el.style.bg_color),
        hex(parse_color("accent")),
        "frameless caption ghost buttons resolve hover:bg-accent; \
             secondary windows must sync_class_variants with the pointer"
    );
    sync_class_variants_subtree(
        &mut el,
        [-1.0, -1.0],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        el.class_set.as_ref().and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "pointer leave must drop :hover so transition-colors can fade ghost rest"
    );
}

#[test]
fn author_hover_bg_overrides_button_variant_hover() {
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::LayoutTree;
    use crate::style::sync_class_variants_subtree;
    let (_h, mut el) = create_root(|| -> Element {
        button("t-btn-author-hover")
            .variant("default")
            .class("hover:bg-muted")
            .text("Go")
            .into()
    });
    el.layout_bounds = [0.0, 0.0, 80.0, 32.0];
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut el,
        [40.0, 16.0],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        hex(el.style.bg_color),
        hex(parse_color("muted")),
        "author hover:bg must win over variant hover:bg-primary/80"
    );
}

#[test]
fn button_defaults_to_keyboard_focus_ring() {
    use crate::core::reactivity::create_root;
    let (_h, el) = create_root(|| -> Element { button("t-btn-focus-ring").text("Go").into() });
    assert!(el.focusable);
    assert!(el.focus_ring);
}

#[test]
fn disabled_button_is_not_focusable() {
    use crate::core::reactivity::create_root;
    let (_h, el) = create_root(|| -> Element {
        button("t-btn-focus-ring-off")
            .disabled(true)
            .text("Go")
            .into()
    });
    assert!(!el.focusable);
    assert!(!el.focus_ring);
}

#[test]
fn button_class_ring_width_overrides_theme() {
    use crate::core::reactivity::create_root;
    use crate::style::apply_focus_ring_shadow;
    let (_h, mut el) =
        create_root(|| -> Element { button("t-btn-ring-2").class("ring-2").text("Go").into() });
    assert_eq!(el.style.focus_ring_width, Some(2.0));
    apply_focus_ring_shadow(&mut el, true, true);
    assert!(
        (el.style.shadow_spread - 2.0).abs() < f32::EPSILON,
        "Button ring must honor authored ring-2 like Input"
    );
}

#[test]
fn builder_style() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        button("btn-chrome")
            .style(|b| b.text_size(20.0))
            .child(Element::new().text("OK"))
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "button .style(|b| b.text_size(20)) must land on the button"
    );
}
