//! `<App>` / `app()` document defaults must not race `--lib` rem tests.

use std::sync::Mutex;

use jadix::app::app;
use jadix::components::{App, AppProps};
use jadix::core::tree::Element;
use jadix::{set_text_defaults, text_defaults};

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
