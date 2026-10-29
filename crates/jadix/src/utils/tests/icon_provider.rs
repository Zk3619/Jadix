use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn icon_hover_animation_uses_css_and_restores_idle() {
    let mut el = with_icon_animations(
        build_icon_named("settings"),
        "spin",
        "none",
        "icon-spin-test",
        "snappy",
    )
    .class("text-foreground");
    assert!(el.style.animations.is_empty());
    (el.on_hover.as_ref().unwrap().borrow_mut())(true);
    sync_icon_hover(&mut el);
    assert_eq!(el.style.animations[0].name, "spin");
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    el.layout_bounds = [0.0, 0.0, 24.0, 24.0];
    let mut impact = LayoutStatus::Clean;
    crate::style::utility::sync_class_variants_subtree(
        &mut el,
        [10.0, 10.0],
        "",
        false,
        200.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    assert_ne!(
        impact,
        LayoutStatus::Clean,
        "CSS hover must reach the frame engine"
    );
    assert_eq!(
        el.class_set
            .as_ref()
            .unwrap()
            .base_style
            .as_ref()
            .unwrap()
            .animations[0]
            .name,
        "spin"
    );
    (el.on_hover.as_ref().unwrap().borrow_mut())(false);
    sync_icon_hover(&mut el);
    assert!(el.style.animations.is_empty());
    let mut custom = with_icon_animations(
        build_icon_named("heart"),
        "pulse 700ms ease-in-out infinite",
        "none",
        "icon-custom-test",
        "snappy",
    );
    sync_icon_hover_animation(&mut custom, true);
    assert_eq!(custom.style.animations[0].name, "pulse");
    let plain = with_icon_animations(
        build_icon_named("sun"),
        "none",
        "none",
        "icon-none-test",
        "snappy",
    );
    assert!(plain.on_hover.is_none());
}

#[cfg(all(feature = "svg", feature = "gallery-tests", feature = "gallery-tests"))]
#[test]
#[cfg(feature = "macros")]
fn icon_motion_macro_binds_name_and_arbitrates_hover_and_change() {
    let mut el = crate::ui! {
        <Icon name="sun" size="28px" hover_animation="animated" change_animation="morph" spring="smooth" />
    };
    assert_eq!(el.text_style.size, Some(28.0));
    assert_eq!(
        el.get_attr(crate::utils::morph_icon::ATTR_MORPH_SPRING)
            .map(String::as_str),
        Some("smooth")
    );
    assert_eq!(
        el.get_attr(crate::utils::ATTR_ANIMATED_ICON)
            .map(String::as_str),
        Some("sun")
    );
    (el.on_hover.as_ref().unwrap().borrow_mut())(true);
    crate::utils::sync_animated_icon_hover(&mut el);
    assert_eq!(
        apply_bound_icon_name(&mut el, "moon"),
        LayoutStatus::StyleChanged
    );
    let recipe_tick = crate::utils::lucide_animated::tick_animated_icon(&mut el, 0.016);
    if cfg!(feature = "animation-ex") {
        assert!(!recipe_tick);
    }
    assert_eq!(
        crate::utils::morph_icon::tick_morph_icon(&mut el, 0.016),
        cfg!(feature = "animation-ex")
    );
    assert_eq!(
        el.get_attr(crate::utils::ATTR_ANIMATED_ICON)
            .map(String::as_str),
        Some("moon")
    );

    let mut spin = crate::ui! {
        <Icon name="sun" size="28px" hover_animation="spin" change_animation="morph" />
    };
    (spin.on_hover.as_ref().unwrap().borrow_mut())(true);
    crate::utils::sync_animated_icon_hover(&mut spin);
    assert!(!spin.style.animations.is_empty());
    apply_bound_icon_name(&mut spin, "moon");
    crate::utils::sync_animated_icon_hover(&mut spin);
    assert_eq!(
        spin.style.animations.is_empty(),
        cfg!(feature = "animation-ex"),
        "only an active morph owns the geometry during a change"
    );
    assert_eq!(
        crate::utils::morph_icon::tick_morph_icon(&mut spin, 10.0),
        cfg!(feature = "animation-ex")
    );
    crate::utils::sync_animated_icon_hover(&mut spin);
    assert_eq!(spin.style.animations[0].name, "spin");
}

#[cfg(all(feature = "svg", feature = "gallery-tests"))]
#[test]
fn icon_motion_morphs_svg_sources_and_falls_back_for_raster_and_custom_glyphs() {
    let a = r#"<svg viewBox="0 0 24 24" fill="none"><path d="M4 12h16"/></svg>"#;
    let b = r#"<svg viewBox="0 0 24 24" fill="none"><path d="M12 4v16"/></svg>"#;
    let mut el = with_icon_animations(
        build_icon_from_src(svg_markup_to_data_url(a)),
        "spin",
        "morph",
        "icon-svg-test",
        "snappy",
    );
    assert!(el.image_texture.is_none());
    apply_bound_icon_src(&mut el, svg_markup_to_data_url(b));
    assert_eq!(
        crate::utils::subtree_needs_morph_icon_frame(&el),
        cfg!(feature = "animation-ex")
    );
    assert!(el.image_texture.is_none());
    apply_bound_icon_src(&mut el, RED_PNG);
    assert!(el.image_texture.is_some());
    assert!(el.get_attr(crate::utils::ATTR_MORPH_ICON).is_none());
    sync_icon_hover_animation(&mut el, true);
    if cfg!(feature = "animation-base") {
        assert_eq!(
            el.style.animations[0].name, "spin",
            "a stale SVG flight must not block raster hover"
        );
    } else {
        assert!(el.style.animations.is_empty());
    }
    apply_bound_icon_name(&mut el, "sun");
    assert!(el.image_texture.is_none());
    assert!(el.get_attr(crate::utils::ATTR_MORPH_ICON).is_some());
    assert!(
        !crate::utils::subtree_needs_morph_icon_frame(&el),
        "raster to SVG snaps to the first morphable icon"
    );

    let mut custom = with_icon_animations(
        build_icon_named("sun"),
        "animated",
        "morph",
        "icon-glyph-test",
        "snappy",
    );
    apply_resolved(
        &mut custom,
        Some("sun"),
        Some(IconSource::Glyph {
            family: "custom".into(),
            glyph: "X".into(),
        }),
    );
    assert_eq!(custom.text.as_deref(), Some("X"));
    #[cfg(feature = "gallery-tests")]
    assert!(custom.get_attr(crate::utils::ATTR_ANIMATED_ICON).is_none());
    assert!(custom.get_attr(crate::utils::ATTR_MORPH_ICON).is_none());
}

/// 1×1 red PNG.
const RED_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

struct MapProvider(HashMap<String, IconSource>);

impl IconProvider for MapProvider {
    fn resolve(&self, name: &str) -> Option<IconSource> {
        self.0.get(name).cloned()
    }
}

#[test]
fn lucide_name_resolves_to_glyph() {
    with_icon_test(|| {
        let src = resolve_icon("check").expect("lucide check");
        match src {
            IconSource::Glyph { family, glyph } => {
                assert_eq!(family, "lucide");
                assert_eq!(glyph, lucide_builtin_glyph("check").unwrap());
            }
            IconSource::Src(_) => panic!("expected glyph"),
        }
        let el = build_icon_named("check");
        assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
        assert_eq!(
            el.get_attr(ATTR_ICON_NAME).map(String::as_str),
            Some("check")
        );
        assert!(el.image_texture.is_none());
    });
}

#[test]
fn register_icon_overlays_lucide() {
    with_icon_test(|| {
        register_icon("check", RED_PNG);
        match resolve_icon("check").unwrap() {
            IconSource::Src(s) => assert_eq!(s, RED_PNG),
            IconSource::Glyph { .. } => panic!("overlay should win"),
        }
        let el = build_icon_named("check");
        assert!(el.image_texture.is_some());
        assert_eq!(
            el.get_attr(ATTR_ICON_SRC).map(String::as_str),
            Some(RED_PNG)
        );
        unregister_icon("check");
        match resolve_icon("check").unwrap() {
            IconSource::Glyph { .. } => {}
            IconSource::Src(_) => panic!("lucide should return"),
        }
    });
}

#[test]
fn custom_provider_shadows_lucide_but_not_overlay() {
    with_icon_test(|| {
        let mut map = HashMap::new();
        map.insert("check".into(), IconSource::path(RED_PNG));
        map.insert("brand".into(), IconSource::path("assets/brand.png"));
        set_icon_provider(Arc::new(MapProvider(map)));
        match resolve_icon("check").unwrap() {
            IconSource::Src(s) => assert_eq!(s, RED_PNG),
            IconSource::Glyph { .. } => panic!("provider should win over lucide"),
        }
        match resolve_icon("brand").unwrap() {
            IconSource::Src(s) => assert_eq!(s, "assets/brand.png"),
            IconSource::Glyph { .. } => panic!("expected src"),
        }
        register_icon("brand", "assets/overlay.png");
        match resolve_icon("brand").unwrap() {
            IconSource::Src(s) => assert_eq!(s, "assets/overlay.png"),
            IconSource::Glyph { .. } => panic!("overlay should win"),
        }
    });
}

#[test]
fn from_str_svg_becomes_data_url() {
    let src = IconSource::from("<svg xmlns='http://www.w3.org/2000/svg'></svg>");
    match src {
        IconSource::Src(s) => assert!(s.starts_with("data:image/svg+xml,"), "{s}"),
        IconSource::Glyph { .. } => panic!("expected src"),
    }
    let path = IconSource::from("assets/logo.png");
    match path {
        IconSource::Src(s) => assert_eq!(s, "assets/logo.png"),
        IconSource::Glyph { .. } => panic!("expected src"),
    }
}

#[test]
fn build_icon_from_src_queues_texture() {
    with_icon_test(|| {
        let el = build_icon_from_src(RED_PNG);
        assert!(el.image_texture.as_deref().unwrap().contains("jadix-src:"));
        assert_eq!(
            el.get_attr(ATTR_ICON_SRC).map(String::as_str),
            Some(RED_PNG)
        );
        assert!(el.text.is_none());
    });
}

#[test]
fn apply_bound_icon_name_switches_glyph_to_src() {
    with_icon_test(|| {
        let mut el = build_icon_named("check");
        assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
        register_icon("brand", RED_PNG);
        assert_eq!(
            apply_bound_icon_name(&mut el, "brand"),
            LayoutStatus::LayoutChanged
        );
        assert!(el.text.is_none());
        assert!(el.image_texture.is_some());
        assert_eq!(apply_bound_icon_name(&mut el, "brand"), LayoutStatus::Clean);
        assert_eq!(
            apply_bound_icon_name(&mut el, "check"),
            LayoutStatus::LayoutChanged
        );
        assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
        assert!(el.image_texture.is_none());
    });
}

#[test]
fn with_icon_size_boxes_src_only() {
    with_icon_test(|| {
        let glyph = with_icon_size(build_icon_named("check"), 20.0);
        assert!((glyph.text_style.size_resolved() - 20.0).abs() < f32::EPSILON);
        let src = with_icon_size(build_icon_from_src(RED_PNG), 20.0);
        assert!((src.text_style.size_resolved() - 20.0).abs() < f32::EPSILON);
    });
}
