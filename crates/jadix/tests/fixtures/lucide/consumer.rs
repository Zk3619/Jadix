use external_lucide::utils as icons;
use ui_runtime::core::reactivity::create_root;
use ui_runtime::core::tree::Element;

#[test]
fn copied_catalog_font_and_static_paths_use_public_runtime_services() {
    external_lucide::install();
    let icon = ui_runtime::utils::icon_named("check");
    assert_eq!(icon.text.as_deref(), icons::lucide_glyph("check").as_deref());
    assert!(icons::lucide_svg_inner("check").is_some());
    let fonts = ui_runtime::renderer::font_loader::create_font_system();
    assert!(fonts.db().faces().any(|face| face.families.iter().any(|(family, _)| family == "lucide")));
    assert!(ui_runtime::utils::font::register_font_data(vec![0, 1, 2]).is_err());
}

#[test]
fn copied_animated_builder_and_macro_use_node_playback_without_built_in_recipes() {
    external_lucide::install();
    let (_owner, ()) = create_root(|| {
        assert!(icons::animated_icon_names().count() >= 400);
        let mut icon = external_lucide::ui! { <AnimatedIcon name="bell" size={24.0} /> };
        assert!(icon.icon_playback.is_some());
        (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
        ui_runtime::utils::sync_icon_hover(&mut icon);
        assert_eq!(icon.attrs[icons::ATTR_ANIMATED_ICON_STATE], "animate");
        assert!(ui_runtime::authoring::icon::tick_tree(&mut icon, 0.016));
        let playback = icon.icon_playback.unwrap();
        assert!((playback.texture)(&icon, 24.0, [0, 0, 0, 255], 2.0).is_some());
        let mut root = Element::new().child(icon);
        let rendered = ui_runtime::layout::LayoutTree::new().build_and_layout(&mut root, 160.0, 100.0, [-1.0; 2]);
        assert!(!rendered.textured_draws.is_empty());
        (root.children[0].on_hover.as_ref().unwrap().borrow_mut())(false);
        ui_runtime::utils::sync_icon_hover(&mut root);
        assert_eq!(root.children[0].attrs[icons::ATTR_ANIMATED_ICON_STATE], "normal");
    });
}

#[test]
fn copied_catalog_drives_generic_icon_hover_and_morph_without_runtime_product_features() {
    external_lucide::install();
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
