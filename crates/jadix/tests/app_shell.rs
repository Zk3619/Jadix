//! `<App>` / `app()` document defaults must not race `--lib` rem tests.

use std::sync::Mutex;

use jadix::app::app;
use jadix::components::{App, AppProps};
use jadix::core::tree::Element;
use jadix::prelude::div;
use jadix::{set_opacity_groups_enabled, set_text_defaults, text_defaults, ui};

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn app_component_sets_root_font_size() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = text_defaults();
    let restored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _el = App(AppProps {
            font_size: Some(19.0),
            font: Some("Inter".into()),
            children: Some(Element::new().text("x")),
            ..Default::default()
        });
        assert!((text_defaults().size_px - 19.0).abs() < 1e-6);
        assert_eq!(text_defaults().family.as_deref(), Some("Inter"));
        assert_eq!(jadix::root_font_size_px(), 19.0);
    }));
    set_text_defaults(prev);
    restored.unwrap();
}

#[test]
fn app_builder_element_sets_root_font_size() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = text_defaults();
    let restored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let el: Element = app().font_size(21).child(Element::new().text("x")).into();
        assert_eq!(el.get_attr("data-slot").map(String::as_str), Some("app"));
        assert!((text_defaults().size_px - 21.0).abs() < 1e-6);
    }));
    set_text_defaults(prev);
    restored.unwrap();
}

#[test]
fn opacity_group_switches_are_available_at_node_and_app_boundaries() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = jadix::opacity_groups_enabled();
    let result = std::panic::catch_unwind(|| {
        let node: Element = div().opacity_group(true).into();
        assert_eq!(node.get_attr("jadix_opacity_group").map(String::as_str), Some("1"));
        let (_owner, rsx) = jadix::core::reactivity::create_root(|| {
            jadix::authoring::mount::with_inline_mount(|| ui! {
                <App opacity_groups={false}><Div opacityGroup="true" /></App>
            })
        });
        assert!(!jadix::opacity_groups_enabled());
        assert_eq!(rsx.children[0].get_attr("jadix_opacity_group").map(String::as_str), Some("1"));
        let _: Element = app().opacity_groups(true).into();
        assert!(jadix::opacity_groups_enabled());

        jadix::core::reactivity::create_root(|| {
            let mut root = Element::new().w(100.0).h(100.0).child(
                div().w(80.0).h(80.0).bg("#ffffff").opacity(0.5).opacity_group(true)
                    .child(div().w(40.0).h(40.0).bg("#ff0000")),
            );
            let mut layout = jadix::layout::LayoutTree::new();
            layout.set_collect_skip_allowed(true);
            let grouped = layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
            assert_eq!(grouped.textured_draws.len(), 1);
            assert_eq!(grouped.textured_draws[0].opacity, 0.5);
            set_opacity_groups_enabled(false);
            let disabled = layout.build_and_layout_fast(&mut root, 100.0, 100.0, [-1.0; 2], true, false);
            assert!(disabled.textured_draws.is_empty());
            assert!(!disabled.instances.is_empty());
            set_opacity_groups_enabled(true);
            let enabled = layout.build_and_layout_fast(&mut root, 100.0, 100.0, [-1.0; 2], true, false);
            assert_eq!(enabled.textured_draws.len(), 1);
            assert_eq!(root.children[0].set_opacity_group(false), jadix::core::tree::LayoutStatus::StyleChanged);
            assert_eq!(root.children[0].set_opacity_group(false), jadix::core::tree::LayoutStatus::Clean);
            let node_disabled = layout.build_and_layout_fast(&mut root, 100.0, 100.0, [-1.0; 2], false, false);
            assert!(node_disabled.textured_draws.is_empty());
        });
    });
    set_opacity_groups_enabled(previous);
    result.unwrap();
}
