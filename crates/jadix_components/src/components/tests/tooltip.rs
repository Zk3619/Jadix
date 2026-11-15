use super::*;

#[test]
fn never_hover_delay_resists_tooltip_handoff_but_keeps_keyboard_focus() {
    use crate::core::reactivity::create_root;
    use crate::utils::hover_intent::NEVER_HOVER_DELAY_MS;

    let (_owner, _) = create_root(|| {
        let focused = use_signal("__jadix_button_focus_id", String::new);
        let visible = use_signal("__jadix_focus_visible", || false);
        let first = use_signal("tip-never-first", || true);
        pointer_tooltip_enter("tip-never-first", first);
        tooltip_scope_with_delays("tip-never", None, Some(NEVER_HOVER_DELAY_MS), None, || {
            let ctx = use_tooltip().unwrap();
            let trigger = chain_trigger_hover(
                Element::new().focus_id("tip-never-trigger"),
                ctx.clone(),
                "tip-never-trigger".into(),
            );
            let hover = trigger.on_hover.as_ref().unwrap();
            hover.borrow_mut()(true);
            assert!(
                !ctx.open.peek(),
                "an open sibling cannot bypass never-hover"
            );
            hover.borrow_mut()(false);
            pointer_tooltip_leave("tip-never-first");
            hover.borrow_mut()(true);
            assert!(!ctx.open.peek(), "handoff grace cannot bypass never-hover");
            hover.borrow_mut()(false);
            focused.set("tip-never-trigger".into());
            visible.set(true);
            assert!(ctx.open.peek(), "keyboard focus remains accessible");
            focused.set(String::new());
            assert!(!ctx.open.peek());
            end_pointer_tooltip_grace();
            trigger
        })
    });
}

#[test]
fn tooltip_builds_tree() {
    let _el: Element = tooltip("tip1", "Hello").into();
}

#[test]
fn tooltip_hover_open_delay_defaults_to_300ms_and_is_configurable() {
    use crate::core::reactivity::create_root;
    use crate::utils::hover_intent::DEFAULT_HOVER_OPEN_DELAY_MS;

    let (_owner, _) = create_root(|| {
        tooltip_scope("tip-default-delay", None, || {
            assert_eq!(
                use_tooltip().unwrap().open_delay_ms.peek(),
                DEFAULT_HOVER_OPEN_DELAY_MS
            );
            Element::new()
        });
        tooltip_scope_with_delays("tip-custom-delay", None, Some(725), None, || {
            assert_eq!(use_tooltip().unwrap().open_delay_ms.peek(), 725);
            Element::new()
        })
    });
}

#[test]
fn tooltip_side_maps() {
    assert_eq!(tooltip_side_to_placement("top"), PopupPlacement::TopCenter);
    assert_eq!(
        tooltip_side_to_placement("bottom"),
        PopupPlacement::BottomCenter
    );
    assert_eq!(
        tooltip_side_to_placement("bottom-start"),
        PopupPlacement::BottomStart
    );
}

#[test]
fn tooltip_content_anchor_is_zero_sized() {
    use crate::core::reactivity::create_root;
    let (_owner, el) = create_root(|| {
        tooltip_scope("tip-anchor", None, || {
            tooltip_content(TooltipContentOpts::default(), || Element::new().text("hi"))
        })
    });
    fn find_zero(el: &Element) -> bool {
        let is_view = el.template_id.as_deref() == Some("tip-anchor__view");
        let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
            && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
        (is_view && dim0) || el.children.iter().any(find_zero)
    }
    assert!(
        find_zero(&el),
        "TooltipContent must use a 0×0 portal anchor"
    );
}
