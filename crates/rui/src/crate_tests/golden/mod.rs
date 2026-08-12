//! WP-2.6 component golden scenes (stable-face only).
//!
//! Scenes: Button · Input · Dialog · Tabs · ScrollArea.
//! Chrome instances only (no glyphon) for cross-OS CI; see `render_element_scene`.
//!
//! Record: `RUI_UPDATE_GOLDEN=1 cargo test -p rui --lib golden_component`

use crate::components::button::button;
use crate::components::dialog::{
    dialog, dialog_content, DialogBody, DialogBodyProps, DialogContentOpts,
};
use crate::components::input::input;
use crate::components::scroll_area::scroll_area;
use crate::components::{
    tabs_content, tabs_scope, TabsList, TabsListProps, TabsTrigger, TabsTriggerProps,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::renderer::offscreen::{assert_matches_golden, render_element_scene};
use crate::style::{parse_color, set_theme_mode, ThemeMode};
use std::sync::Mutex;

/// Serialize offscreen GPU device creation (WARP crashes under parallel `request_device`).
static GOLDEN_GPU_LOCK: Mutex<()> = Mutex::new(());

/// AA / WARP slack for rounded-rect edges (no glyph AA in these scenes).
const GOLDEN_TOLERANCE: u8 = 16;

fn clear_background() -> wgpu::Color {
    // Match light theme page bg ≈ white/near-white; exact theme token varies — pin clear.
    wgpu::Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    }
}

fn assert_scene(name: &str, root: &mut Element, w: u32, h: u32) {
    let _guard = GOLDEN_GPU_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    set_theme_mode(ThemeMode::Light);
    let Some(img) = render_element_scene(root, w, h, clear_background()) else {
        // Dedicated CI step must exercise a real adapter (Linux mesa / Windows backend-dx12).
        eprintln!(
            "skip {name}: no wgpu adapter (RUI_FORCE_SOFTWARE=1; Windows: --features backend-dx12)"
        );
        return;
    };
    assert_matches_golden(name, &img, GOLDEN_TOLERANCE);
}

#[test]
fn golden_component_button_default() {
    let mut root = Element::new()
        .w(160.0)
        .h(64.0)
        .bg(parse_color("background"))
        .child(
            Element::from(
                button("golden-btn")
                    .w(96.0)
                    .h(36.0)
                    .child(Element::new().text("OK")),
            )
            .m(12.0),
        );
    assert_scene("golden_button_default", &mut root, 160, 64);
}

#[test]
fn golden_component_input_idle() {
    let mut root = Element::new()
        .w(240.0)
        .h(64.0)
        .bg(parse_color("background"))
        .child(
            Element::from(input("golden-input").w(200.0).h(36.0).placeholder("Name")).m(12.0),
        );
    assert_scene("golden_input_idle", &mut root, 240, 64);
}

#[test]
fn golden_component_dialog_open() {
    let open = use_signal("golden-dialog__open", || true);
    let dialog_el = dialog("golden-dialog").open(open).child(|| {
        dialog_content(
            DialogContentOpts {
                show_close: false,
                ..Default::default()
            },
            || {
                DialogBody(DialogBodyProps {
                    children: Some(
                        Element::new()
                            .w(160.0)
                            .h(48.0)
                            .bg(parse_color("muted"))
                            .rounded(4.0),
                    ),
                    base: None,
                })
            },
        )
    });
    let mut root = Element::new()
        .w(320.0)
        .h(200.0)
        .bg(parse_color("background"))
        .child(dialog_el);
    assert_scene("golden_dialog_open", &mut root, 320, 200);
}

#[test]
fn golden_component_tabs_two() {
    let value = use_signal("golden-tabs__value", || "one".to_string());
    let el = tabs_scope(
        "golden-tabs",
        Some(value),
        "one",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![
                        TabsTrigger(TabsTriggerProps {
                            value: Some("one".into()),
                            disabled: None,
                            children: Some(Element::new().text("A")),
                            base: None,
                        }),
                        TabsTrigger(TabsTriggerProps {
                            value: Some("two".into()),
                            disabled: None,
                            children: Some(Element::new().text("B")),
                            base: None,
                        }),
                    ])
                }),
                base: None,
            });
            let panel = tabs_content("one", || {
                Element::new()
                    .w(200.0)
                    .h(40.0)
                    .bg(parse_color("muted"))
                    .rounded(4.0)
            });
            let mut pack = Element::new();
            pack.set_attr("rui_fragment", "1");
            pack = pack.children(vec![list, panel]);
            pack
        },
    );
    let mut root = Element::new()
        .w(280.0)
        .h(120.0)
        .bg(parse_color("background"))
        .child(el);
    assert_scene("golden_tabs_two", &mut root, 280, 120);
}

#[test]
fn golden_component_scroll_area() {
    let tall = Element::new()
        .w(120.0)
        .h(240.0)
        .bg(parse_color("accent"))
        .rounded(4.0);
    let sa = Element::from(
        scroll_area("golden-scroll")
            .w(140.0)
            .h(80.0)
            .overflow_y("auto")
            .child(tall),
    );
    let mut root = Element::new()
        .w(180.0)
        .h(120.0)
        .bg(parse_color("background"))
        .child(sa.m(12.0));
    assert_scene("golden_scroll_area", &mut root, 180, 120);
}
