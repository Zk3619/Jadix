use super::*;
use crate::style::*;

fn reset() {
    clear_stylesheets();
    clear_theme_overrides();
    set_theme_mode(ThemeMode::Light);
}

#[test]
fn presets_cover_both_modes_and_keep_shadcn_defaults() {
    assert_eq!(
        Theme::preset(ThemePreset::Shadcn, ThemeMode::Light),
        Theme::light()
    );
    assert_eq!(
        Theme::preset(ThemePreset::Shadcn, ThemeMode::Dark),
        Theme::dark()
    );
    for preset in ThemePreset::ALL {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = Theme::preset(preset, mode);
            assert_eq!(theme.preset, preset);
            assert_eq!(theme.mode, mode);
            theme.styles.validate().unwrap();
            assert_ne!(theme.colors.primary, theme.colors.primary_foreground);
            assert_eq!(
                theme.styles.glass.is_some(),
                preset == ThemePreset::LiquidGlass
            );
        }
    }
}

#[test]
fn winui_names_are_compatibility_aliases_for_one_fluent_preset() {
    assert_eq!(ThemePreset::ALL.len(), 4);
    for name in ["fluent", "winui3", "winui-3", "ms-fluent"] {
        let config = parse_theme_overrides_str(&format!("preset = \"{name}\"")).unwrap();
        assert_eq!(config.preset, ThemePreset::Fluent);
        assert_eq!(config.preset.name(), "fluent");
        assert!(toml::to_string(&config)
            .unwrap()
            .contains("preset = \"fluent\""));
    }
}

#[test]
fn glass_interaction_slots_tween_hover_and_selection_without_resizing_text() {
    reset();
    set_theme_preset(ThemePreset::LiquidGlass);
    set_css_transition_virtual_timeline(true);
    for slot in [
        "button",
        "sidebar-menu-button",
        "tabs-trigger",
        "toggle-group-item",
    ] {
        set_css_transition_timeline_ms(0.0);
        let mut element = Element::new().w(160.0).h(40.0).text("Stable label");
        element.set_attr("data-slot", slot);
        element.set_attr("data-state", "off");
        let element = apply_stylesheet_rules(element);
        let mut element = with_class_list(
            element,
            "bg-#202020 hover:bg-#e0e0e0 data-[state=on]:bg-#909090",
        );
        let mut layout = crate::layout::LayoutTree::new();
        layout.build_and_layout(&mut element, 320.0, 100.0, [-1.0; 2]);
        let bounds = element.layout_bounds;
        let text_size = element.text_style.size_resolved();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut element,
            [-1.0; 2],
            "",
            false,
            320.0,
            100.0,
            &mut layout,
            &mut impact,
        );
        sync_class_variants_subtree(
            &mut element,
            [20.0, 20.0],
            "",
            false,
            320.0,
            100.0,
            &mut layout,
            &mut impact,
        );
        advance_css_transition_timeline_ms(80.0);
        sync_class_variants_subtree(
            &mut element,
            [20.0, 20.0],
            "",
            false,
            320.0,
            100.0,
            &mut layout,
            &mut impact,
        );
        let middle = element.style.bg_color;
        assert!(
            middle[0] > 0.13 && middle[0] < 0.87,
            "{slot}: hover must have an intermediate sample: {middle:?}"
        );
        element.set_attr("data-state", "on");
        sync_class_variants_subtree(
            &mut element,
            [-1.0; 2],
            "",
            false,
            320.0,
            100.0,
            &mut layout,
            &mut impact,
        );
        assert_eq!(
            element.style.bg_color, middle,
            "{slot}: interrupt from live paint, not a reset pose"
        );
        advance_css_transition_timeline_ms(300.0);
        sync_class_variants_subtree(
            &mut element,
            [-1.0; 2],
            "",
            false,
            320.0,
            100.0,
            &mut layout,
            &mut impact,
        );
        assert_eq!(
            element.style.bg_color,
            parse_color("#909090"),
            "{slot}: {:?}",
            element.style.transition
        );
        assert_eq!(element.layout_bounds, bounds);
        assert_eq!(element.text_style.size_resolved(), text_size, "{slot}");
        assert_eq!(element.style.scale, [1.0; 2]);
    }
    set_css_transition_virtual_timeline(false);
    reset();
}

#[test]
fn mode_switch_keeps_preset_common_mode_and_component_overrides() {
    reset();
    let overrides = parse_theme_overrides_str(
        r##"
            preset = "md3"
            [styles]
            control-radius = 11
            [dark-styles]
            control-height = 48
            [light]
            primary = "#123456"
            [components]
            button = "rounded-[7px] hover:bg-secondary"
        "##,
    )
    .unwrap();
    install_theme_overrides(overrides).unwrap();
    assert_eq!(get_theme().colors.primary, parse_color("#123456"));
    set_theme_mode(ThemeMode::Dark);
    assert_eq!(theme_preset(), ThemePreset::Material3);
    assert_eq!(get_theme().styles.control_radius, Some(11.0));
    assert_eq!(get_theme().styles.control_height, Some(48.0));
    assert!(get_theme().components.contains_key("button"));
    set_theme_mode(ThemeMode::Light);
    assert_eq!(get_theme().styles.control_height, Some(40.0));
    set_theme_preset(ThemePreset::Fluent);
    assert_eq!(get_theme().styles.control_radius, Some(11.0));
    reset();
}

#[test]
fn invalid_style_install_is_atomic() {
    reset();
    set_theme_preset(ThemePreset::Fluent);
    let prior = get_theme();
    let stored = theme_overrides();
    for raw in [
        "[styles]\ncontrol-radius = -1",
        "[styles]\nfont-size = nan",
        "[styles]\nsurface-opacity = 2",
        "[styles.glass]\nrefraction-index = 0.5",
        "[components]\nbutton = 'not-a-utility'",
    ] {
        let overrides = parse_theme_overrides_str(raw).unwrap();
        assert!(install_theme_overrides(overrides).is_err(), "{raw}");
        assert_eq!(get_theme(), prior);
        assert_eq!(theme_overrides(), stored);
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn page_and_navigation_surfaces_follow_preset_materials() {
    reset();
    for (preset, radius) in [
        (ThemePreset::Fluent, 0.0),
        (ThemePreset::Material3, 28.0),
        (ThemePreset::LiquidGlass, 20.0),
    ] {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            set_theme_preset(preset);
            let (_, page) = crate::core::reactivity::create_root(|| {
                apply_stylesheet_rules_tree(crate::components::SidebarInset(
                    crate::components::SidebarInsetProps {
                        variant: Some("inset".into()),
                        children: None,
                        ..Default::default()
                    },
                ))
            });
            assert_eq!(page.style.radii, [radius; 4], "{preset:?} page shell");
            if preset == ThemePreset::LiquidGlass {
                assert_eq!(
                    &page.style.bg_color[..3],
                    &get_theme().colors.popover[..3],
                    "Liquid Glass page shell tint"
                );
                assert_eq!(
                    page.style.bg_color[3],
                    get_theme().styles.surface_opacity.unwrap()
                );
                assert!(page.style.glass.is_some(), "Liquid Glass page shell");
                assert!(page.style.backdrop_blur > 0.0);
            } else if preset == ThemePreset::Fluent {
                assert_eq!(
                    page.style.bg_color[3], 0.0,
                    "Fluent page shell exposes Mica"
                );
                assert!(page.style.glass.is_none());
                assert_eq!(page.style.backdrop_blur, 0.0);
            } else {
                assert_eq!(page.style.bg_color, get_theme().colors.background);
                assert!(page.style.glass.is_none());
                assert_eq!(page.style.backdrop_blur, 0.0);
            }
            assert_eq!(page.style.shadow_blur, 0.0);
            let mut sidebar = Element::new().bg(get_theme().colors.sidebar);
            sidebar.set_attr("data-slot", "sidebar");
            let sidebar = apply_stylesheet_rules(sidebar);
            if matches!(preset, ThemePreset::Fluent | ThemePreset::LiquidGlass) {
                assert_eq!(
                    sidebar.style.bg_color[3], 0.0,
                    "outer rail exposes the window material"
                );
            }
            let mut navigation = Element::new();
            navigation.set_attr("data-slot", "sidebar-inner");
            let navigation = apply_stylesheet_rules(navigation);
            let expected = if preset == ThemePreset::LiquidGlass {
                get_theme().colors.popover
            } else {
                get_theme().colors.sidebar
            };
            if preset != ThemePreset::Fluent {
                assert_eq!(&navigation.style.bg_color[..3], &expected[..3]);
            }
            assert_eq!(
                navigation.style.bg_color[3],
                match preset {
                    ThemePreset::Fluent => 0.0,
                    ThemePreset::LiquidGlass => get_theme().styles.surface_opacity.unwrap(),
                    _ => 1.0,
                }
            );
            assert_eq!(
                navigation.style.glass.is_some(),
                preset == ThemePreset::LiquidGlass,
                "{preset:?} sidebar material"
            );
            assert_ne!(
                get_theme().colors.sidebar,
                get_theme().colors.card,
                "navigation and content are distinct surfaces"
            );
        }
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn cards_use_glass_surface_only_for_liquid_glass() {
    reset();
    for preset in ThemePreset::ALL {
        set_theme_preset(preset);
        let (_, card) = crate::core::reactivity::create_root(|| {
            apply_stylesheet_rules_tree(crate::components::Card(crate::components::CardProps {
                children: None,
                ..Default::default()
            }))
        });
        assert_eq!(
            card.style.glass.is_some(),
            preset == ThemePreset::LiquidGlass,
            "{preset:?} card material"
        );
        if preset == ThemePreset::Fluent {
            assert_eq!(card.style.radii, [4.0; 4], "Fluent card corners");
        }
        if preset == ThemePreset::LiquidGlass {
            assert_eq!(
                card.style.bg_color[3],
                get_theme().styles.surface_opacity.unwrap()
            );
        } else if preset == ThemePreset::Fluent {
            assert_eq!(
                card.style.bg_color[3],
                get_theme().styles.surface_opacity.unwrap(),
                "Fluent cards tint the Mica backdrop"
            );
        }
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn fluent_scrollbars_are_thumb_only_over_the_material() {
    reset();
    set_theme_preset(ThemePreset::Fluent);
    let colors = get_theme().colors;
    assert_eq!(colors.scrollbar_track, [0.0; 4]);
    assert_eq!(colors.scrollbar_track_border, [0.0; 4]);
    assert!((colors.scrollbar_thumb[3] - 0.72).abs() < f32::EPSILON);
    assert_eq!(colors.scrollbar_thumb_border, [0.0; 4]);
    crate::core::reactivity::create_root(|| {
        for orientation in [
            crate::components::scrollbar::ScrollbarOrientation::Vertical,
            crate::components::scrollbar::ScrollbarOrientation::Horizontal,
        ] {
            let mut bar: Element = crate::components::scrollbar("fluent-bar")
                .orientation(orientation)
                .always()
                .into();
            let mut layout = crate::layout::LayoutTree::new();
            layout.build_and_layout(&mut bar, 200.0, 200.0, [-1.0; 2]);
            assert_eq!(bar.style.bg_color[3], 0.0);
            assert_eq!(bar.style.border_color[3], 0.0);
            let cross_axis = match orientation {
                crate::components::scrollbar::ScrollbarOrientation::Vertical => 2,
                crate::components::scrollbar::ScrollbarOrientation::Horizontal => 3,
            };
            assert_eq!(bar.layout_bounds[cross_axis], 8.0);
            assert_eq!(bar.children[0].layout_bounds[cross_axis], 6.0);
        }
        let mut area: Element = crate::components::scroll_area("fluent-area")
            .w(200.0)
            .h(150.0)
            .overflow("scroll")
            .scrollbar_visibility("always")
            .child(Element::new().min_w(400.0).min_h(300.0))
            .into();
        let mut layout = crate::layout::LayoutTree::new();
        layout.build_and_layout(&mut area, 200.0, 150.0, [-1.0; 2]);
        for axis in ["x", "y"] {
            let focus = format!("scrollbar-track:fluent-area:{axis}");
            let bar = area
                .children
                .iter()
                .find(|child| child.focus_id.as_deref() == Some(focus.as_str()))
                .unwrap();
            assert_eq!(bar.style.bg_color[3], 0.0);
            assert_eq!(bar.layout_bounds[if axis == "y" { 2 } else { 3 }], 8.0);
        }
        let mut explicit: Element = crate::components::scrollbar("explicit-fluent-bar")
            .thickness(14.0)
            .track_color(parse_color("#123456"))
            .always()
            .into();
        layout.build_and_layout(&mut explicit, 200.0, 200.0, [-1.0; 2]);
        assert_eq!(explicit.layout_bounds[2], 14.0);
        assert_eq!(explicit.style.bg_color, parse_color("#123456"));
    });
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn fluent_cards_blend_over_a_clear_borderless_sidebar() {
    reset();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        set_theme_mode(mode);
        set_theme_preset(ThemePreset::Fluent);
        crate::core::reactivity::create_root(|| {
            let card: Element = crate::components::card().into();
            let sidebar =
                apply_stylesheet_rules_tree(crate::components::sidebar("fluent-navigation").into());
            fn inner(element: &Element) -> Option<&Element> {
                if element.attrs.get("data-slot").map(String::as_str) == Some("sidebar-inner") {
                    return Some(element);
                }
                element.children.iter().find_map(inner)
            }
            assert!(card.style.bg_color[3] > 0.0 && card.style.bg_color[3] <= 0.35);
            assert!(card.style.backdrop_blur > 0.0);
            assert!(card.style.glass.is_none());
            let control: Element = crate::components::button("fluent-neutral-control")
                .variant("outline")
                .into();
            assert_eq!(control.style.bg_color[3], 0.0);
            let navigation = inner(&sidebar).unwrap();
            assert_eq!(navigation.style.bg_color[3], 0.0);
            assert_eq!(navigation.style.backdrop_blur, 0.0);
            assert_eq!(navigation.style.resolved_border_widths(), [0.0; 4]);
            assert_eq!(sidebar.style.bg_color[3], 0.0);
            assert_eq!(sidebar.style.resolved_border_widths(), [0.0; 4]);
        });
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn outline_buttons_stay_clear_across_presets_and_interactions() {
    reset();
    for preset in ThemePreset::ALL {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            set_theme_preset(preset);
            crate::core::reactivity::create_root(|| {
                let mut button: Element = crate::components::button("clear-outline")
                    .variant("outline")
                    .text("Outline")
                    .into();
                for hovered in [false, true, false] {
                    resolve_class_variants(
                        &mut button,
                        &VariantContext {
                            hovered,
                            ..Default::default()
                        },
                    );
                    if !hovered {
                        assert_eq!(button.style.bg_color[3], 0.0, "{preset:?} {mode:?}");
                        assert_eq!(button.style.backdrop_blur, 0.0);
                        assert!(button.style.glass.is_none());
                    }
                }
                let disabled: Element = crate::components::button("clear-disabled-outline")
                    .variant("outline")
                    .disabled(true)
                    .into();
                assert_eq!(disabled.style.bg_color[3], 0.0, "{preset:?} {mode:?}");
                let explicit = with_class_list(button, "bg-#123456");
                assert_eq!(explicit.style.bg_color, parse_color("#123456"));
                assert_eq!(
                    crate::components::button::button_chrome_for_variant("outline", 0.0).0[3],
                    0.0,
                );
            });
        }
    }
    reset();
}

#[test]
fn transparent_surfaces_keep_explicit_tints_and_opaque_fallback() {
    reset();
    for preset in [ThemePreset::Fluent, ThemePreset::LiquidGlass] {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            set_theme_preset(preset);
            assert_eq!(
                get_theme().styles.surface_opacity,
                Some(if preset == ThemePreset::Fluent {
                    if mode == ThemeMode::Dark {
                        0.18
                    } else {
                        0.35
                    }
                } else if mode == ThemeMode::Dark {
                    0.38
                } else {
                    0.44
                })
            );
            for opacity in [0.0, 0.4, 1.0] {
                let mut overrides = theme_overrides();
                overrides.styles.surface_opacity = Some(opacity);
                install_theme_overrides(overrides).unwrap();
                for slot in ["card", "sidebar-inner", "popover-content", "sidebar-inset"] {
                    let mut surface = Element::new().bg(get_theme().colors.card);
                    surface.set_attr("data-slot", slot);
                    let surface = apply_stylesheet_rules(surface);
                    assert_eq!(
                        surface.style.bg_color[3],
                        if preset == ThemePreset::Fluent
                            && matches!(slot, "sidebar-inset" | "sidebar-inner")
                        {
                            0.0
                        } else {
                            opacity
                        },
                        "{preset:?} {mode:?} {slot} opacity={opacity}"
                    );
                    assert_eq!(
                        surface.style.glass.is_some(),
                        preset == ThemePreset::LiquidGlass && opacity < 1.0
                    );
                    assert_eq!(
                        surface.style.backdrop_blur,
                        if opacity == 1.0
                            || (preset == ThemePreset::Fluent
                                && matches!(slot, "sidebar-inset" | "sidebar-inner"))
                        {
                            0.0
                        } else {
                            get_theme().styles.backdrop_blur.unwrap()
                        }
                    );
                    let surface = with_class_list(surface, "bg-#123456");
                    assert_eq!(surface.style.bg_color, parse_color("#123456"));
                }
            }
            reset();
        }
    }
    reset();
}

#[test]
fn preset_utilities_are_supported_for_every_slot() {
    reset();
    for preset in ThemePreset::ALL {
        set_theme_preset(preset);
        for slot in [
            "button",
            "card",
            "input",
            "textarea",
            "card-header",
            "select-content",
            "checkbox",
            "select-trigger",
            "combobox-trigger",
            "combobox-content",
            "dialog-content",
            "alert-dialog-content",
            "sheet-content",
            "drawer-content",
            "popover-content",
            "hover-card-content",
            "dropdown-menu-content",
            "dropdown-menu-sub-content",
            "context-menu-content",
            "context-menu-sub-content",
            "menubar-content",
            "menubar-sub-content",
            "tooltip-content",
            "command",
            "command-overlay-content",
            "sidebar-wrapper",
            "sidebar-inset",
            "sidebar-inner",
            "sidebar-menu-button",
            "tabs-list",
            "tabs-trigger",
            "dropdown-menu-item",
            "context-menu-item",
            "menubar-item",
            "select-item",
            "combobox-item",
            "command-item",
        ] {
            let mut el = Element::new();
            el.set_attr("data-slot", slot);
            for token in slot_theme_classes(&mut el).split_whitespace() {
                let parsed = jadix_style_tokens::parse_class_token(token);
                assert!(
                    jadix_style_tokens::is_known_utility(&parsed.utility),
                    "{preset:?}: {token}"
                );
            }
        }
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn component_sheet_and_reactive_caller_classes_override_theme() {
    reset();
    install_theme_overrides(
        parse_theme_overrides_str(
            r#"
            preset = "material3"
            [components]
            button = "rounded-[13px]"
        "#,
        )
        .unwrap(),
    )
    .unwrap();
    crate::core::reactivity::create_root(|| {
        let themed: Element = crate::components::button("theme-test")
            .text("Themed")
            .into();
        assert_eq!(themed.style.radii, [13.0; 4]);
        assert_eq!(
            themed.text_style.font_family.as_deref(),
            Some("Roboto, sans-serif")
        );
        register_stylesheet(
            parse_stylesheet(
                r#"
                [[rule]]
                selectors = ["[data-slot=button]"]
                classes = "rounded-[9px]"
            "#,
            )
            .unwrap(),
        )
        .unwrap();
        let mut button: Element = crate::components::button("theme-layer-test")
            .text("Layers")
            .class("rounded-[3px] text-[19px]")
            .into();
        assert_eq!(button.style.radii, [3.0; 4]);
        assert_eq!(button.children[0].text_style.size, Some(19.0));
        apply_utility_classes(&mut button, "rounded-[5px]");
        assert_eq!(button.style.radii, [5.0; 4]);
        apply_utility_classes(&mut button, "");
        assert_eq!(button.style.radii, [9.0; 4]);
    });
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn glass_covers_content_cards_and_can_be_disabled_by_caller() {
    reset();
    set_theme_preset(ThemePreset::LiquidGlass);
    crate::core::reactivity::create_root(|| {
        let content: Element = crate::components::card().into();
        assert!(content.style.glass.is_some());
        assert!(content.style.bg_color[3] < 1.0);
        assert!(content.style.backdrop_blur > 0.0);
        let glass: Element = crate::components::card()
            .class("glass backdrop-blur-[2px]")
            .into();
        assert!(glass.style.glass.is_some());
        assert_eq!(glass.style.radii, [16.0; 4]);
        let opaque: Element = crate::components::card()
            .class("glass-none bg-card backdrop-blur-none")
            .into();
        assert!(opaque.style.glass.is_none());
        assert_eq!(opaque.style.backdrop_blur, 0.0);
        let mut root: Element = crate::primitives::vstack()
            .w(400.0)
            .h(300.0)
            .bg("background")
            .child(glass)
            .into();
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0; 2]);
        assert!(result
            .instances
            .iter()
            .any(|instance| instance.glass[0] > 1.0 && instance.backdrop_blur > 0.5));
        assert!(!result.overlay_split.page_stack_bands.is_empty());
    });
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn liquid_glass_input_focus_ring_follows_rounded_chrome() {
    reset();
    set_theme_preset(ThemePreset::LiquidGlass);
    crate::core::reactivity::create_root(|| {
        let body = crate::components::dialog::dialog_scope("glass-focus-body", None, || {
            use crate::prelude::*;
            crate::ui! {
                <DialogBody><Input id="glass-focus-corners" class="w-full" /></DialogBody>
            }
        });
        let mut root = Element::new()
            .flex_col()
            .flex_items("stretch")
            .w(240.0)
            .h(80.0)
            .p(20.0)
            .child(body);
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
        let focus = crate::components::input::focus_id_for_input("glass-focus-corners");
        crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new)
            .set(focus.clone());
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut root,
            [-1.0; 2],
            &focus,
            false,
            240.0,
            80.0,
            &mut layout,
            &mut impact,
        );
        crate::utils::text_edit::sync_text_edit_paint_subtree(&mut root, &mut layout, &mut impact);
        let result = layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
        let Some(gpu) = crate::renderer::offscreen::OffscreenGpu::new() else {
            return;
        };
        let image = gpu
            .render_instances_readback(240, 80, wgpu::Color::WHITE, &result.instances)
            .unwrap();
        if let Some(directory) = std::env::var_os("JADIX_GLASS_EVIDENCE") {
            std::fs::create_dir_all(&directory).unwrap();
            image::save_buffer(
                std::path::Path::new(&directory).join("focused-input.png"),
                &image.pixels,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        let corner = image.pixel(19, 19).unwrap();
        let edge = image.pixel(40, 19).unwrap();
        assert!(
            edge[2].saturating_sub(edge[0]) > 20,
            "focus ring must be visible: {edge:?}"
        );
        assert!(
            corner[2].saturating_sub(corner[0]) < 5,
            "focus ring must not fill square corners: {corner:?}"
        );
    });
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn preset_shapes_elevation_and_typography_follow_component_roles() {
    reset();
    for preset in [ThemePreset::Fluent, ThemePreset::Material3] {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            set_theme_preset(preset);
            crate::core::reactivity::create_root(|| {
                let card: Element = crate::components::card().into();
                assert_eq!(
                    card.style.radii,
                    [match preset {
                        ThemePreset::Fluent => 4.0,
                        _ => 12.0,
                    }; 4]
                );
                let mut dialog = Element::new();
                dialog.set_attr("data-slot", "dialog-content");
                let dialog = apply_stylesheet_rules(dialog);
                assert!(dialog.style.shadow_blur > card.style.shadow_blur);
                let mut tooltip = Element::new();
                tooltip.set_attr("data-slot", "tooltip-content");
                let tooltip = apply_stylesheet_rules(tooltip);
                assert_eq!(tooltip.style.radii, [4.0; 4]);
                let button: Element = crate::components::button("preset-label")
                    .text("Action")
                    .into();
                let input: Element = crate::components::input("preset-body").into();
                assert_eq!(
                    button.children[0].text_style.weight,
                    Some(match preset {
                        ThemePreset::Fluent => 600,
                        _ => 500,
                    })
                );
                assert_eq!(input.input_paint_sync.as_ref().unwrap().font_weight, 400);
                if preset == ThemePreset::Material3 {
                    assert_eq!(card.style.radii, [12.0; 4]);
                    assert_eq!(dialog.style.radii, [28.0; 4]);
                    assert_eq!(input.input_paint_sync.as_ref().unwrap().text_size_px, 16.0);
                    assert_eq!(
                        input.input_paint_sync.as_ref().unwrap().line_height_px,
                        Some(24.0)
                    );
                }
            });
        }
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn preset_button_states_keep_disabled_roles_and_caller_precedence() {
    reset();
    for preset in [ThemePreset::Fluent, ThemePreset::Material3] {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            set_theme_preset(preset);
            crate::core::reactivity::create_root(|| {
                let mut button: Element = crate::components::button("preset-states")
                    .text("Action")
                    .into();
                let rest = button.style.bg_color;
                resolve_class_variants(
                    &mut button,
                    &VariantContext {
                        hovered: true,
                        ..Default::default()
                    },
                );
                assert_ne!(button.style.bg_color, rest, "{preset:?} {mode:?}");
                let hovered = button.style.bg_color;
                resolve_class_variants(
                    &mut button,
                    &VariantContext {
                        hovered: true,
                        active: true,
                        ..Default::default()
                    },
                );
                assert_ne!(button.style.bg_color, hovered);
                resolve_class_variants(
                    &mut button,
                    &VariantContext {
                        hovered: true,
                        active: true,
                        disabled: true,
                        ..Default::default()
                    },
                );
                let disabled = button.style.bg_color;
                assert_eq!(button.style.opacity, 1.0);
                if preset == ThemePreset::Material3 {
                    assert!((disabled[3] - 0.12).abs() < 0.001);
                    assert!(
                        (f32::from(button.text_style.color_resolved()[3]) / 255.0 - 0.38).abs()
                            < 0.004
                    );
                }
                resolve_class_variants(
                    &mut button,
                    &VariantContext {
                        disabled: true,
                        ..Default::default()
                    },
                );
                assert_eq!(button.style.bg_color, disabled);
                let mut custom: Element = crate::components::button("caller-state")
                    .class("hover:bg-secondary disabled:bg-success")
                    .into();
                resolve_class_variants(
                    &mut custom,
                    &VariantContext {
                        hovered: true,
                        ..Default::default()
                    },
                );
                assert_eq!(custom.style.bg_color, parse_color("secondary"));
                resolve_class_variants(
                    &mut custom,
                    &VariantContext {
                        disabled: true,
                        ..Default::default()
                    },
                );
                assert_eq!(custom.style.bg_color, parse_color("success"));
            });
        }
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn opaque_glass_fallback_removes_optics_and_preserves_overrides() {
    reset();
    install_theme_overrides(parse_theme_overrides_str("preset = 'liquid-glass'\n[styles]\nsurface-opacity = 1\npanel-radius = 18\nshadow = 'none'").unwrap()).unwrap();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        set_theme_mode(mode);
        crate::core::reactivity::create_root(|| {
            let panel = crate::components::popover_surface(Element::new().text("Opaque"), None);
            assert!(panel.style.glass.is_none());
            assert_eq!(panel.style.backdrop_blur, 0.0);
            assert_eq!(panel.style.bg_color[3], 1.0);
            assert_eq!(panel.style.radii, [18.0; 4]);
            assert_eq!(panel.style.shadow_blur, 0.0);
        });
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn explicit_common_styles_win_even_when_equal_to_another_preset_default() {
    reset();
    install_theme_overrides(parse_theme_overrides_str("preset = 'material3'\n[styles]\nfont-size = 14\nline-height = 20\nfont-weight = 400\ncontrol-radius = 20\npanel-radius = 28").unwrap()).unwrap();
    for preset in [
        ThemePreset::Material3,
        ThemePreset::Fluent,
        ThemePreset::LiquidGlass,
    ] {
        set_theme_preset(preset);
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            crate::core::reactivity::create_root(|| {
                let button: Element = crate::components::button("explicit-label")
                    .text("Action")
                    .into();
                assert_eq!(button.children[0].text_style.weight, Some(400));
                let input: Element = crate::components::input("explicit-body").into();
                let paint = input.input_paint_sync.as_ref().unwrap();
                assert_eq!(paint.text_size_px, 14.0);
                assert_eq!(paint.line_height_px, Some(20.0));
                assert_eq!(input.style.radii, [20.0; 4]);
                let card: Element = crate::components::card().into();
                assert_eq!(card.style.radii, [28.0; 4]);
            });
        }
    }
    reset();
}

#[test]
fn new_presets_keep_modal_scrims_black_in_both_modes_and_allow_explicit_color() {
    reset();
    for preset in [
        ThemePreset::Fluent,
        ThemePreset::Material3,
        ThemePreset::LiquidGlass,
    ] {
        set_theme_preset(preset);
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            set_theme_mode(mode);
            let scrim = crate::utils::modal_shell::default_modal_overlay_color(None, None);
            assert_eq!(scrim, [0.0, 0.0, 0.0, 0.25]);
            assert_eq!(
                semantic_modal_overlay_scrim(SemanticTone::Warning, None, None),
                scrim
            );
            let mut explicit = parse_color("#123456");
            explicit[3] = 0.6;
            assert_eq!(
                crate::utils::modal_shell::default_modal_overlay_color(Some("#123456"), Some(0.6)),
                explicit
            );
        }
    }
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn floating_surfaces_receive_glass_and_late_slots_are_restyled() {
    reset();
    set_theme_preset(ThemePreset::LiquidGlass);
    crate::core::reactivity::create_root(|| {
        let popover = crate::components::popover_surface(Element::new().text("Popover"), None);
        assert!(popover.style.glass.is_some());
        assert_eq!(popover.style.bg_color[3], 0.44);
        assert_eq!(
            popover.attrs.get("data-slot").map(String::as_str),
            Some("popover-content")
        );
        register_stylesheet(
            parse_stylesheet(
                "[[rule]]\nselectors = ['[data-slot=button]']\nclasses = 'rounded-md'",
            )
            .unwrap(),
        )
        .unwrap();
        let mut panel = apply_stylesheet_rules(Element::new());
        panel.set_attr("data-slot", "dialog-content");
        let panel = apply_stylesheet_rules(panel);
        assert!(
            panel.style.glass.is_some(),
            "late slot must not be skipped as already styled"
        );
        assert_eq!(panel.style.bg_color[3], 0.44);
        let mut root = Element::new()
            .w(400.0)
            .h(300.0)
            .child(crate::utils::portal_panel(panel.w(200.0).h(120.0), 900));
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0; 2]);
        assert!(result.instances[result.overlay_split.instance_start..]
            .iter()
            .any(|instance| instance.glass[0] > 1.0));
        assert!(
            result
                .overlay_split
                .bands
                .iter()
                .all(|band| band.backdrop_blur == 0.0),
            "optical panel must not trigger fullscreen blur"
        );
    });
    reset();
}

#[test]
#[cfg(feature = "product-tests")]
fn density_and_typography_reach_controls_without_overriding_caller() {
    reset();
    install_theme_overrides(
        parse_theme_overrides_str(
            r#"
            preset = "fluent"
            [styles]
            control-height = 40
            font-size = 16
            line-height = 24
        "#,
        )
        .unwrap(),
    )
    .unwrap();
    crate::core::reactivity::create_root(|| {
        let button: Element = crate::components::button("density")
            .size("sm")
            .text("Small")
            .into();
        assert_eq!(
            crate::layout::dim_length_px(button.style.size.height),
            Some(35.0)
        );
        assert_eq!(button.children[0].text_style.size, Some(16.0));
        assert_eq!(button.children[0].text_style.line_height_px, Some(24.0));
        let icon: Element = crate::components::button("icon-density")
            .size("icon")
            .into();
        assert_eq!(
            crate::layout::dim_length_px(icon.style.size.width),
            Some(40.0)
        );
        let mut button: Element = crate::components::button("caller-typography")
            .text("Custom")
            .class("leading-[28px] text-[18px]")
            .into();
        assert_eq!(button.children[0].text_style.line_height_px, Some(28.0));
        apply_utility_classes(&mut button, "leading-tight");
        assert_eq!(button.children[0].text_style.line_height_px, None);
        assert_eq!(button.children[0].text_style.line_height_scale, Some(1.25));
        let input: Element = crate::components::input("density-input").into();
        assert_eq!(
            crate::layout::dim_length_px(input.style.size.height),
            Some(40.0)
        );
        assert_eq!(input.input_paint_sync.as_ref().unwrap().text_size_px, 16.0);
    });
    reset();
}
