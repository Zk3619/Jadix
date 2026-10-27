use ui_runtime::core::reactivity::create_root;
use ui_runtime::core::tree::Element;

#[test]
fn component_builder_and_props_use_the_same_product_contract() {
    let (_owner, ()) = create_root(|| {
        let builder: Element = widgets::button("builder").text("Save").into();
        let props = widgets::Button(widgets::ButtonProps {
            id: "props".into(),
            children: Some(Element::new().text("Save")),
            ..Default::default()
        });
        assert_eq!(builder.attrs["data-slot"], props.attrs["data-slot"]);
        assert_eq!(builder.template_id.as_deref(), Some("builder"));
        assert_eq!(props.template_id.as_deref(), Some("props"));
    });
}

#[test]
fn unified_icons_install_catalog_font_recipes_and_playback() {
    icons::install().unwrap();
    let (_owner, ()) = create_root(|| {
        assert!(icons::glyph("check").is_some());
        assert!(icons::svg_inner("check").is_some());
        assert!(icons::animated_icon_names().count() >= 400);
        let mut icon: Element = icons::animated_icon("bell").into();
        (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
        ui_runtime::utils::sync_icon_hover(&mut icon);
        assert_eq!(icon.attrs[icons::ATTR_ANIMATED_ICON_STATE], "animate");
        assert!(ui_runtime::authoring::icon::tick_tree(&mut icon, 0.016));
        let playback = icon.icon_playback.unwrap();
        assert!((playback.texture)(&icon, 24.0, [0, 0, 0, 255], 2.0).is_some());
    });
}

#[test]
fn high_level_drag_owns_a_disposable_scope_over_the_host_protocol() {
    let (owner, ()) = create_root(|| {
        let mut tree = drag_product::utils::drag::drag_scope("product-scope")
            .child(|| Element::new().w(40.0).h(24.0));
        let mut layout = ui_runtime::layout::LayoutTree::new();
        layout.build_and_layout(&mut tree, 400.0, 300.0, [-1.0; 2]);
        assert!(tree.layout_bounds[2] > 0.0);
        assert!(drag_product::utils::drag::session::scope_exists(
            "product-scope"
        ));
    });
    owner.dispose();
    assert!(!drag_product::utils::drag::session::scope_exists(
        "product-scope"
    ));
}

#[cfg(feature = "macros")]
#[test]
fn renamed_products_expand_optional_macro_paths() {
    icons::install().unwrap();
    let (_owner, ()) = create_root(|| {
        use widgets::{Button, ButtonProps};
        let button = widgets::ui! { <Button id="macro-button">"Save"</Button> };
        assert_eq!(button.template_id.as_deref(), Some("macro-button"));
        let icon = icons::ui! { <AnimatedIcon name="bell" controlled={true} /> };
        assert!(icon.icon_playback.is_some());
        let drag = drag_product::ui! { <DragProvider id="macro-scope"><Draggable id="card">{Element::new()}</Draggable></DragProvider> };
        assert!(!drag.children.is_empty());
    });
}

#[cfg(feature = "macros")]
mod retained_lucide_contracts {
    use ::icons as catalog;
    use catalog::utils::lucide_animated as icons;
    use ui_runtime::core::reactivity::create_root;
    use ui_runtime::core::tree::Element;

    #[test]
    fn copied_catalog_font_and_static_paths_use_public_runtime_services() {
        catalog::install().unwrap();
        let icon = ui_runtime::utils::icon_named("check");
        assert_eq!(
            icon.text.as_deref(),
            catalog::lucide_glyph("check").as_deref()
        );
        assert!(catalog::svg_inner("check").is_some());
        let fonts = ui_runtime::renderer::font_loader::create_font_system();
        assert!(fonts
            .db()
            .faces()
            .any(|face| face.families.iter().any(|(family, _)| family == "lucide")));
        assert!(ui_runtime::utils::font::register_font_data(vec![0, 1, 2]).is_err());
    }

    #[test]
    fn copied_animated_builder_and_macro_use_node_playback_without_built_in_recipes() {
        catalog::install().unwrap();
        let (_owner, ()) = create_root(|| {
            assert!(icons::animated_icon_names().count() >= 400);
            let mut icon = catalog::ui! { <AnimatedIcon name="bell" size={24.0} /> };
            assert!(icon.icon_playback.is_some());
            (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
            ui_runtime::utils::sync_icon_hover(&mut icon);
            assert_eq!(icon.attrs[icons::ATTR_ANIMATED_ICON_STATE], "animate");
            assert!(ui_runtime::authoring::icon::tick_tree(&mut icon, 0.016));
            let playback = icon.icon_playback.unwrap();
            assert!((playback.texture)(&icon, 24.0, [0, 0, 0, 255], 2.0).is_some());
            let mut root = Element::new().child(icon);
            let rendered = ui_runtime::layout::LayoutTree::new()
                .build_and_layout(&mut root, 160.0, 100.0, [-1.0; 2]);
            assert!(!rendered.textured_draws.is_empty());
            (root.children[0].on_hover.as_ref().unwrap().borrow_mut())(false);
            ui_runtime::utils::sync_icon_hover(&mut root);
            assert_eq!(
                root.children[0].attrs[icons::ATTR_ANIMATED_ICON_STATE],
                "normal"
            );
        });
    }

    #[cfg(feature = "animation-path")]
    #[test]
    fn copied_catalog_drives_generic_icon_hover_and_morph_without_runtime_product_features() {
        catalog::install().unwrap();
        let (_owner, ()) = create_root(|| {
            let mut icon = ui_runtime::ui! { <Icon name="bell" hover_animation="animated" change_animation="morph" /> };
            assert!(icon.icon_playback.is_some());
            assert!(icon.attrs.contains_key(ui_runtime::utils::ATTR_MORPH_ICON));
            (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
            ui_runtime::utils::sync_icon_hover(&mut icon);
            assert_eq!(icon.attrs[icons::ATTR_ANIMATED_ICON_STATE], "animate");
            ui_runtime::utils::apply_bound_icon_name(&mut icon, "check");
            assert_eq!(icon.attrs[icons::ATTR_ANIMATED_ICON], "check");
        });
    }
}
