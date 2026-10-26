use jadix::prelude::*;

#[test]
fn static_icons_and_css_hover_do_not_require_lucide_recipes() {
    let _: Option<&'static str> = jadix::utils::lucide_svg_inner("check");
    let mut icon = with_icon_animations(
        icon_named("check"),
        "spin",
        "none",
        "feature-icon-spin",
        "snappy",
    );
    assert!(icon.text.as_ref().is_some_and(|text| !text.is_empty()));
    (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
    sync_icon_hover(&mut icon);
    if cfg!(feature = "animation-base") {
        assert_eq!(icon.style.animations[0].name, "spin");
    } else {
        assert!(icon.style.animations.is_empty());
    }
    (icon.on_hover.as_ref().unwrap().borrow_mut())(false);
    sync_icon_hover(&mut icon);
    assert!(icon.style.animations.is_empty());

    register_icon(
        "feature-custom-glyph",
        IconSource::Glyph {
            family: "custom".into(),
            glyph: "X".into(),
        },
    );
    let custom = icon_named("feature-custom-glyph");
    assert_eq!(custom.text.as_deref(), Some("X"));
    assert_eq!(custom.text_style.font_family.as_deref(), Some("custom"));
    unregister_icon("feature-custom-glyph");
}

#[cfg(not(feature = "lucide-animated"))]
#[test]
fn disabled_lucide_recipes_leave_icons_static() {
    let icon = with_icon_animations(
        icon_named("bell"),
        "animated",
        "none",
        "feature-icon-static",
        "snappy",
    );
    assert!(icon.text.is_some());
    assert!(icon.on_hover.is_none());
    assert!(!icon.attrs.contains_key("jadix_animated_icon"));
    assert!(!icon.attrs.contains_key("jadix_animated_icon_state"));
}

#[cfg(feature = "lucide-animated")]
#[test]
fn enabled_lucide_recipes_support_builder_macro_and_hover() {
    assert!(animated_icon_names().count() >= 400);
    assert!(animated_icon_recipe("bell").is_some());
    let mut icon: Element = animated_icon("bell").into();
    (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
    sync_icon_hover(&mut icon);
    assert_eq!(
        icon.attrs[jadix::utils::ATTR_ANIMATED_ICON_STATE],
        "animate"
    );
    (icon.on_hover.as_ref().unwrap().borrow_mut())(false);
    sync_icon_hover(&mut icon);
    assert_eq!(icon.attrs[jadix::utils::ATTR_ANIMATED_ICON_STATE], "normal");
    let controlled = jadix::ui! { <AnimatedIcon name="bell" controlled={true} /> };
    assert_eq!(controlled.attrs[jadix::utils::ATTR_ANIMATED_ICON], "bell");
    assert!(controlled.on_hover.is_none());
}

#[cfg(all(feature = "svg", feature = "animation-ex"))]
#[test]
fn svg_morph_runs_without_lucide_recipe_playback() {
    let mut icon = with_icon_animations(
        morph_icon("M4 12L20 12").hover_animation(false).into(),
        "spin",
        "none",
        "feature-morph-hover",
        "snappy",
    );
    assert!(icon.attrs.contains_key(jadix::utils::ATTR_MORPH_ICON));
    (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
    sync_icon_hover(&mut icon);
    assert_eq!(icon.style.animations[0].name, "spin");
    morph_to(&mut icon, "M12 4L12 20");
    sync_icon_hover(&mut icon);
    assert!(icon.style.animations.is_empty());
    assert!(jadix::utils::tick_morph_icons_subtree(&mut icon, 0.016));
    assert!(jadix::utils::ensure_morph_icon_texture(&icon, 24.0, [0, 0, 0, 255], 1.0).is_some());
    assert!(jadix::utils::tick_morph_icons_subtree(&mut icon, 10.0));
    sync_icon_hover(&mut icon);
    assert_eq!(icon.style.animations[0].name, "spin");
    (icon.on_hover.as_ref().unwrap().borrow_mut())(false);
    sync_icon_hover(&mut icon);
    assert!(icon.style.animations.is_empty());
}

#[cfg(all(feature = "code", feature = "components"))]
#[test]
fn code_editor_is_independent_of_highlighting_and_language_servers() {
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let editor: Element = jadix::components::code_editor("feature-code")
            .initial("let value = 1;")
            .into();
        assert!(editor.input_paint_sync.is_some());
    });
}

#[cfg(feature = "markdown")]
#[test]
fn markdown_model_and_helpers_do_not_require_document_view() {
    let document = Document::markdown("# Title\n\n**content**\n");
    assert!(matches!(
        document.blocks()[0].kind,
        DocumentBlockKind::Heading(1)
    ));
    assert_eq!(document.source().text(), "# Title\n\n**content**\n");
    let inline = jadix::text::parse_markdown_inline("**bold**");
    assert_eq!(
        inline
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        "bold"
    );
}

#[cfg(feature = "code-lsp")]
#[test]
fn language_service_positions_do_not_require_an_editor() {
    let source = "a😀中\r\nz";
    let position = jadix::text::lsp::position(source, 5);
    assert_eq!(position["line"].as_u64(), Some(0));
    assert_eq!(position["character"].as_u64(), Some(3));
    assert_eq!(jadix::text::lsp::byte_at(source, &position), Some(5));
}

#[cfg(feature = "syntax-highlighting")]
#[test]
fn syntax_highlighting_does_not_require_an_editor_or_document_view() {
    let source = "let message = \"中文😀\";\n";
    let spans = jadix::text::syntax::highlight_code(source, "rs", false);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        source
    );
    assert!(spans.iter().any(|span| span.color.is_some()));
}

#[test]
fn disabled_native_materials_do_not_request_native_blur() {
    let available = cfg!(any(
        all(windows, feature = "windows-dwm"),
        all(not(windows), feature = "window-materials")
    ));
    if !available {
        let attributes = WindowOptions::new().blur(true).to_window_attributes();
        assert!(!attributes.blur);
    }
}

#[test]
fn theme_registry_contains_only_enabled_presets() {
    let expected = [
        (ThemePreset::Shadcn, cfg!(feature = "theme-shadcn")),
        (ThemePreset::Fluent, cfg!(feature = "theme-fluent")),
        (ThemePreset::Material3, cfg!(feature = "theme-material3")),
        (
            ThemePreset::LiquidGlass,
            cfg!(feature = "theme-liquid-glass"),
        ),
    ];
    for (preset, enabled) in expected {
        assert_eq!(preset.is_enabled(), enabled);
        assert_eq!(ThemePreset::ALL.contains(&preset), enabled);
        if enabled {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let theme = Theme::preset(preset, mode);
                assert_eq!(theme.preset, preset);
                assert_eq!(theme.mode, mode);
                assert!(theme.colors.background.into_iter().all(f32::is_finite));
                assert!(theme.focus_ring_width.is_finite());
            }
        }
    }
    assert_eq!(Theme::default().preset, ThemePreset::default());
    assert_eq!(get_theme().preset, ThemePreset::default());
}

#[test]
fn disabled_themes_are_rejected_without_mutating_current_theme() {
    let before = get_theme();
    for preset in [
        ThemePreset::Fluent,
        ThemePreset::Material3,
        ThemePreset::LiquidGlass,
    ] {
        if !preset.is_enabled() {
            let source = format!("preset = {:?}", preset.name());
            assert!(parse_theme_overrides_str(&source).is_err());
            let overrides = ThemeOverrides {
                preset,
                ..ThemeOverrides::default()
            };
            assert!(install_theme_overrides(overrides).is_err());
            assert_eq!(get_theme(), before);
            assert!(std::panic::catch_unwind(|| set_theme_preset(preset)).is_err());
            assert_eq!(get_theme(), before);
        }
    }
}

#[cfg(all(feature = "document", feature = "components"))]
#[test]
fn document_view_is_available_without_editing_or_science() {
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let document = Document::markdown("# Title\n\ncontent\n");
        assert!(matches!(
            document.blocks()[0].kind,
            DocumentBlockKind::Heading(1)
        ));
        let view: Element = document_view("feature-document").document(document).into();
        assert_eq!(view.attrs["data-slot"], "document-view");
    });
}

#[cfg(feature = "animation-ex")]
#[test]
fn interaction_motion_is_available_without_path_animation() {
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let state = use_motion("feature-motion");
        let element = Element::new().motion(&state, Motion::new(MotionTarget::default()));
        assert!(element.children.is_empty());
    });
}

#[cfg(not(feature = "file-dialog"))]
#[test]
fn file_dialog_fallback_does_not_open_a_native_dialog() {
    assert!(jadix::utils::file_dialog::pick_file("*").is_none());
    assert!(jadix::utils::file_dialog::pick_files("*").is_empty());
    assert!(jadix::utils::file_dialog::pick_folder().is_none());
}
