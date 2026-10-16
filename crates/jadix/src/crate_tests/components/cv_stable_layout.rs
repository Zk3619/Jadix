//! WP-CV.2 / C-WP-V.2 — stable-face layout + minimal behavior matrix.
//!
//! Button · Input · Dialog · Tabs · Checkbox · Switch must `build_and_layout` with w/h > 0;
//! Checkbox / Switch also assert checked sync without full subtree rebuild.

use crate::components::button::button;
use crate::components::checkbox::{checkbox, sync_checkbox_checked_subtree};
use crate::components::dialog::{
    dialog, dialog_content, DialogBody, DialogBodyProps, DialogContentOpts,
};
use crate::components::input::input;
use crate::components::switch::switch;
use crate::components::{
    tabs_content, tabs_scope, TabsList, TabsListProps, TabsTrigger, TabsTriggerProps,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};

fn assert_positive_bounds(el: &Element, label: &str) {
    let w = el.layout_bounds[2];
    let h = el.layout_bounds[3];
    assert!(
        w > 0.0 && h > 0.0,
        "{label}: expected positive bounds, got w={w} h={h} {:?}",
        el.layout_bounds
    );
}

fn find_first<'a>(el: &'a Element, pred: &impl Fn(&Element) -> bool) -> Option<&'a Element> {
    if pred(el) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(hit) = find_first(c, pred) {
            return Some(hit);
        }
    }
    None
}

fn layout_in_viewport(root: &mut Element, vw: f32, vh: f32) {
    let mut tree = LayoutTree::new();
    tree.build_and_layout(root, vw, vh, [-1.0, -1.0]);
}

fn find_checkbox_indicator<'a>(el: &'a Element) -> &'a Element {
    find_first(el, &|c| {
        c.attrs.get("data-slot").map(|s| s.as_str()) == Some("checkbox-indicator")
    })
    .expect("checkbox indicator")
}
#[test]
fn cv_button_layout_nonzero_bounds() {
    let mut root = Element::new().w(320.0).h(80.0).child(Element::from(
        button("cv-btn").child(Element::new().text("OK")),
    ));
    layout_in_viewport(&mut root, 320.0, 80.0);
    assert_positive_bounds(&root, "button root");
    assert_positive_bounds(&root.children[0], "button");
}

#[test]
fn cv_input_layout_nonzero_bounds() {
    let mut root = Element::new()
        .w(400.0)
        .h(80.0)
        .child(Element::from(input("cv-input").placeholder("Name")));
    layout_in_viewport(&mut root, 400.0, 80.0);
    assert_positive_bounds(&root, "input root");
    assert_positive_bounds(&root.children[0], "input");
}

#[test]
fn cv_dialog_open_layout_nonzero_bounds() {
    let open = use_signal("cv-dialog__open", || true);
    let dialog_el = dialog("cv-dialog").open(open).child(|| {
        dialog_content(
            DialogContentOpts {
                show_close: false,
                ..Default::default()
            },
            || {
                DialogBody(DialogBodyProps {
                    children: Some(Element::new().text("Dialog body")),
                    base: None,
                })
            },
        )
    });
    let mut root = Element::new().w(800.0).h(600.0).child(dialog_el);
    layout_in_viewport(&mut root, 800.0, 600.0);
    assert_positive_bounds(&root, "dialog host");
}

#[test]
fn cv_tabs_layout_nonzero_bounds() {
    let value = use_signal("cv-tabs__value", || "one".to_string());
    let el = tabs_scope("cv-tabs", Some(value), "one", "horizontal", false, || {
        let list = TabsList(TabsListProps {
            id: String::new(),
            variant: None,
            value: None,
            default_value: None,
            options: None,
            children: Some({
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![
                    TabsTrigger(TabsTriggerProps {
                        value: Some("one".into()),
                        disabled: None,
                        children: Some(Element::new().text("One")),
                        base: None,
                    }),
                    TabsTrigger(TabsTriggerProps {
                        value: Some("two".into()),
                        disabled: None,
                        children: Some(Element::new().text("Two")),
                        base: None,
                    }),
                ])
            }),
            base: None,
        });
        let panel = tabs_content("one", || Element::new().text("Panel one"));
        let mut pack = Element::new();
        pack.set_attr("jadix_fragment", "1");
        pack = pack.children(vec![list, panel]);
        pack
    });
    let mut root = Element::new().w(640.0).h(240.0).child(el);
    layout_in_viewport(&mut root, 640.0, 240.0);
    assert_positive_bounds(&root, "tabs root");
    let tabs = &root.children[0];
    assert_positive_bounds(tabs, "tabs scope");
    let list = find_first(tabs, &|c| {
        c.attrs.get("data-slot").map(|s| s.as_str()) == Some("tabs-list")
    })
    .expect("tabs list");
    assert_positive_bounds(list, "tabs list");
}

#[test]
fn cv_checkbox_layout_nonzero_bounds() {
    let mut root = Element::new().w(400.0).h(80.0).child(Element::from(
        checkbox("cv-cb").child(Element::new().text("Accept")),
    ));
    layout_in_viewport(&mut root, 400.0, 80.0);
    let row = &root.children[0];
    assert_positive_bounds(row, "checkbox row");
    let ctrl = find_first(row, &|c| c.checkbox_sync.is_some()).expect("checkbox control");
    assert_positive_bounds(ctrl, "checkbox control");
}

#[test]
fn cv_switch_layout_nonzero_bounds() {
    let mut root = Element::new()
        .w(400.0)
        .h(80.0)
        .child(Element::from(switch("cv-sw")));
    layout_in_viewport(&mut root, 400.0, 80.0);
    let track = &root.children[0];
    assert_positive_bounds(track, "switch track");
}

#[test]
fn cv_checkbox_checked_sync_updates_indicator() {
    let checked = use_signal("cv-cb-sync__checkbox", || false);
    let mut root: Element = checkbox("cv-cb-sync").checked_signal(checked).into();

    assert!(
        find_checkbox_indicator(&root).text_style.color_resolved()[3] < 10,
        "unchecked indicator should be transparent"
    );

    checked.set(true);
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_checkbox_checked_subtree(&mut root, &mut tree, &mut impact);
    assert!(
        find_checkbox_indicator(&root).text_style.color_resolved()[3] > 200,
        "checked indicator should be visible"
    );
    assert_ne!(impact, LayoutStatus::Clean);
}

#[test]
fn cv_switch_click_toggles_checked() {
    let checked = use_signal("cv-sw-click__switch", || false);
    let root: Element = switch("cv-sw-click").checked_signal(checked).into();
    let track = find_first(&root, &|c| c.switch_checked_sync.is_some()).expect("switch track");
    track.on_click.as_ref().expect("switch click").borrow_mut()();
    assert!(checked.peek(), "click should toggle checked signal");
}
