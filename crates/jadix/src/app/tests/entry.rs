use super::*;

#[test]
fn app_builder_preserves_native_window_controls() {
    let options = app()
        .min_size(320.0, 240.0)
        .max_size(1280.0, 900.0)
        .position(-20.0, 50.0)
        .resizable(false)
        .maximized(true)
        .fullscreen(true)
        .always_on_top(true)
        .into_window_options();
    assert_eq!(options.min_size, Some([320.0, 240.0]));
    assert_eq!(options.max_size, Some([1280.0, 900.0]));
    assert_eq!(options.position, Some([-20.0, 50.0]));
    assert!(!options.resizable);
    assert!(options.maximized && options.fullscreen && options.always_on_top);
}

#[test]
fn desktop_policies_are_explicit_and_default_to_compatible_lifetimes() {
    let defaults = app().into_window_options();
    assert!(defaults.fullscreen_shortcuts);
    assert!(defaults.close_with_primary);
    assert_eq!(
        defaults.instance_policy,
        super::super::InstancePolicy::Multiple
    );
    let options = app()
        .single_instance("org.jadix.editor:profile-a")
        .fullscreen_shortcuts(false)
        .close_with_primary(false)
        .into_window_options();
    assert!(!options.fullscreen_shortcuts);
    assert!(!options.close_with_primary);
    assert_eq!(
        options.instance_policy,
        super::super::InstancePolicy::Single {
            id: "org.jadix.editor:profile-a".into()
        }
    );
    let multiple = app()
        .single_instance("org.jadix.editor")
        .instance_policy(super::super::InstancePolicy::Multiple)
        .into_window_options();
    assert_eq!(
        multiple.instance_policy,
        super::super::InstancePolicy::Multiple
    );
}

#[test]
fn app_builder_maps_title_size_font_and_frameless() {
    let o = app()
        .title("Hello")
        .window_label("main")
        .size(640.0, 480.0)
        .font("Inter, sans-serif")
        .font_size(20)
        .font_weight(500)
        .frameless()
        .transparent(true)
        .into_window_options();
    assert_eq!(o.title, "Hello");
    assert_eq!(o.window_label.as_deref(), Some("main"));
    assert_eq!(o.inner_width, Some(640.0));
    assert_eq!(o.inner_height, Some(480.0));
    assert!(!o.decorations);
    assert!(o.transparent);
    assert_eq!(o.default_font_family.as_deref(), Some("Inter, sans-serif"));
    assert_eq!(o.default_font_size, Some(20.0));
    assert_eq!(o.default_font_weight, Some(500));
}

#[test]
fn app_builder_maps_glass_recipe_knobs() {
    let o = app()
        .blur(true)
        .backdrop("aero")
        .blur_amount(18.0)
        .glass_tint("#80FF0000")
        .glass_tint_dark("#80202020")
        .luminosity_opacity(0.4)
        .aero_color_balance(0.1)
        .aero_afterglow("#40FFE8A0")
        .aero_reflection(0.2)
        .into_window_options();
    assert!(o.blur);
    assert_eq!(o.backdrop, WindowBackdrop::Aero);
    assert_eq!(o.blur_amount, Some(18.0));
    assert_eq!(o.glass_tint, Some([0x80, 0xff, 0x00, 0x00]));
    assert_eq!(o.glass_tint_dark, Some([0x80, 0x20, 0x20, 0x20]));
    assert_eq!(o.luminosity_opacity, Some(0.4));
    assert_eq!(o.aero_color_balance, Some(0.1));
    assert_eq!(o.aero_afterglow, Some([0x40, 0xff, 0xe8, 0xa0]));
    assert_eq!(o.aero_reflection, Some(0.2));
    assert!(o.wants_custom_dcomp());
}

#[test]
fn unset_app_builder_keeps_window_option_defaults() {
    let d = WindowOptions::default();
    let o = app().into_window_options();
    assert_eq!(o.title, d.title);
    assert_eq!(o.inner_width, d.inner_width);
    assert_eq!(o.decorations, d.decorations);
    assert!(o.default_font_size.is_none());
    assert!(o.default_font_family.is_none());
}

#[test]
fn app_builder_locale_is_recorded() {
    let cfg = app().locale("zh-CN").i18n("i18n.toml").config().clone();
    assert_eq!(cfg.locale.as_deref(), Some("zh-CN"));
    assert_eq!(cfg.i18n.as_deref(), Some("i18n.toml"));
}

#[test]
fn app_element_is_full_size_document_shell() {
    let el: Element = app().title("Shell").child(Element::new().text("hi")).into();
    assert_eq!(el.get_attr("data-slot").map(String::as_str), Some("app"));
    assert_eq!(el.children.len(), 1);
    assert_eq!(el.children[0].text.as_deref(), Some("hi"));
}
