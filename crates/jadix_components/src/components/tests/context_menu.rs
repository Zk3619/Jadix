use super::*;
use std::cell::Cell;

#[test]
fn rsx_menu_hugs_content_in_tall_viewport_after_open_and_idle() {
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};

    clear_overlay_stack();
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, (open, cursor)) = create_root(|| {
        let open = Signal::new(false);
        let cursor = use_signal("height-menu__ctx_cursor", || [1200.0_f32, 1450.0]);
        let live = live.clone();
        create_effect(move || {
            *live.borrow_mut() = crate::ui! {
                <ContextMenu id="height-menu" open={open}>
                    <ContextMenuTrigger>"Right click"</ContextMenuTrigger>
                    <ContextMenuContent>
                        <ContextMenuLabel>"Actions"</ContextMenuLabel>
                        <ContextMenuItem>"Copy"</ContextMenuItem>
                        <ContextMenuItem>"Paste"</ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuCheckboxItem defaultChecked={true}>"Show Bookmarks Bar"</ContextMenuCheckboxItem>
                        <ContextMenuCheckboxItem>"Show Full URLs"</ContextMenuCheckboxItem>
                        <ContextMenuSeparator />
                        <ContextMenuRadioGroup defaultValue="pedro">
                            <ContextMenuLabel>"People"</ContextMenuLabel>
                            <ContextMenuRadioItem value="pedro">"Pedro Duarte"</ContextMenuRadioItem>
                            <ContextMenuRadioItem value="colm">"Colm Tuite"</ContextMenuRadioItem>
                        </ContextMenuRadioGroup>
                        <ContextMenuSeparator />
                        <ContextMenuSub>
                            <ContextMenuSubTrigger>"More Tools"</ContextMenuSubTrigger>
                            <ContextMenuSubContent>
                                <ContextMenuItem>"Save Page As..."</ContextMenuItem>
                                <ContextMenuItem>"Developer Tools"</ContextMenuItem>
                            </ContextMenuSubContent>
                        </ContextMenuSub>
                        <ContextMenuSeparator />
                        <ContextMenuItem variant="destructive">"Delete"</ContextMenuItem>
                    </ContextMenuContent>
                </ContextMenu>
            };
        });
        (open, cursor)
    });
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut live.borrow_mut(), 2500.0, 1500.0, [0.0, 0.0]);
    open.set(true);
    drain_deferred_view_remounts();
    for frame in 0..64 {
        let mut root = live.borrow_mut();
        layout.build_and_layout_fast_ex(&mut root, 2500.0, 1500.0, [0.0, 0.0], true, false, None);
        crate::utils::floating_motion::flush_floating_motion_subtree(&mut root);
        crate::utils::floating_motion::tick_hoisted_floating_css(&mut root);
        let mut panels = 0;
        crate::utils::popup::visit_floating_panels(&root, |panel| {
            if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT) {
                panels += 1;
                assert!(
                    (panel.layout_bounds[3] - 326.0).abs() < 0.5,
                    "menu must hug its rows from the first frame, frame={frame}: {:?}",
                    panel.layout_bounds
                );
                assert!(
                    (panel.layout_bounds[1] + panel.layout_bounds[3] - 1448.0).abs() < 0.5,
                    "menu must remain above the cursor: {:?}",
                    panel.layout_bounds
                );
            }
        });
        assert_eq!(panels, 1);
    }
    // A short viewport must clip for scrolling, then restore the content
    // height when the menu is reopened in a larger window.
    for (height, point, expected_h) in [
        (200.0, [1200.0, 150.0], 184.0),
        (1500.0, [1200.0, 1450.0], 326.0),
        (1500.0, [1200.0, 100.0], 326.0),
    ] {
        open.set(false);
        drain_deferred_view_remounts();
        cursor.set(point);
        open.set(true);
        drain_deferred_view_remounts();
        for frame in 0..4 {
            let mut root = live.borrow_mut();
            layout.build_and_layout_fast_ex(
                &mut root,
                2500.0,
                height,
                [0.0, 0.0],
                true,
                false,
                None,
            );
            let mut panels = 0;
            crate::utils::popup::visit_floating_panels(&root, |panel| {
                if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT) {
                    panels += 1;
                    assert!(
                        (panel.layout_bounds[3] - expected_h).abs() < 0.5,
                        "height must follow content and viewport after reopen, frame={frame}: {:?}",
                        panel.layout_bounds
                    );
                    assert!(
                        panel.layout_bounds[1] >= 8.0
                            && panel.layout_bounds[1] + panel.layout_bounds[3] <= height - 7.5,
                        "menu must remain inside viewport: {:?}",
                        panel.layout_bounds
                    );
                }
            });
            assert_eq!(panels, 1);
        }
    }
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    reset_registered_anchors();
    clear_overlay_stack();
}

#[test]
fn context_menu_records_items() {
    let fired = Rc::new(Cell::new(false));
    let f = fired.clone();
    let _el: Element = context_menu("cm1").item("Copy", move || f.set(true)).into();
    assert!(!fired.get());
}

#[test]
fn builder_trigger_style() {
    use crate::core::reactivity::create_root;
    let (_o, el): (_, Element) = create_root(|| {
        context_menu("cm-chrome").child(|| {
            context_menu_trigger()
                .style(|b| b.text_size(20.0))
                .text("area")
                .into()
        })
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "context_menu_trigger .style(|b| b.text_size(20)) must land on the trigger tree"
    );
}

#[test]
fn compound_scope_exposes_template() {
    let el = context_menu_scope("cm-scope", None, || {
        let trigger = ContextMenuTrigger(ContextMenuTriggerProps {
            id: String::new(),
            children: Some(Element::new().text("area")),
            base: None,
        });
        let content = context_menu_content(ContextMenuContentOpts::default(), || {
            ContextMenuItem(ContextMenuItemProps {
                disabled: Some(false),
                inset: Some(false),
                variant: None,
                on_click: None,
                children: Some(Element::new().text("One")),
                base: None,
            })
        });
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![trigger, content])
    });
    assert_eq!(
        el.template_id.as_deref(),
        Some("cm-scope__context_menu"),
        "scope must expose a stable template id"
    );
}

#[test]
fn compound_sub_provides_context_before_children() {
    let _el = context_menu_scope("cm-sub", None, || {
        context_menu_content(ContextMenuContentOpts::default(), || {
            context_menu_sub(None, || {
                let st = ContextMenuSubTrigger(ContextMenuSubTriggerProps {
                    disabled: Some(false),
                    inset: Some(false),
                    children: Some(Element::new().text("More")),
                    base: None,
                });
                let sc = context_menu_sub_content(Default::default(), || {
                    ContextMenuItem(ContextMenuItemProps {
                        disabled: Some(false),
                        inset: Some(false),
                        variant: None,
                        on_click: None,
                        children: Some(Element::new().text("Nested")),
                        base: None,
                    })
                });
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![st, sc])
            })
        })
    });
}
