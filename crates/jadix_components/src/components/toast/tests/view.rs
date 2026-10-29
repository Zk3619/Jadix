use super::*;
use crate::style::{
    is_variant_style_overridden, parse_color, variant_chrome_enabled, VariantStyleField,
};

fn sample_item() -> ToastItem {
    ToastItem {
        id: 1,
        title: "T".into(),
        description: String::new(),
        tone: ToastTone::Success,
        show_icon: None,
        chrome: true,
        utility_class: None,
        bg_override: None,
        border_override: None,
        fg_override: None,
        created: Instant::now(),
        duration: Duration::from_secs(3),
        exiting: false,
        exit_started: None,
    }
}

#[test]
fn toast_child_hover_expands_stack_and_freezes_timers() {
    use crate::app::tree_hit::{
        apply_pointer_hover_reconcile, clear_hover_tracking, find_paint_aabb_by_focus_id,
    };
    use crate::core::reactivity::create_root;

    for layout in [ToastLayout::Stack, ToastLayout::List] {
        clear_hover_tracking();
        let (_owner, (mut root, ids)) = create_root(|| {
            toast_set_layout(if layout == ToastLayout::Stack {
                "stack"
            } else {
                "list"
            });
            toast_set_overflow("displace");
            toast_set_limit(3);
            toast_queue().set(vec![]);
            toast_pending().set(vec![]);
            set_stack_expanded(false);
            set_stack_timers_paused(false);
            let ids = [
                toast("Older").duration(10_000).show(),
                toast("Saved")
                    .description("Changes applied.")
                    .duration(10_000)
                    .show(),
            ];
            for id in ids {
                let _ = take_toast_needs_enter(id);
            }
            let panel = render_queue(layout, "top-left", &toast_queue().peek(), false);
            (Element::new().w(800.0).h(600.0).child(panel), ids)
        });
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
        let region_id = if layout == ToastLayout::Stack {
            "__jadix_toast_stack_pile".to_string()
        } else {
            item_template_id(TOAST_CARD_SCOPE, ids[1])
        };
        let transitions = std::rc::Rc::new(RefCell::new(Vec::new()));
        let seen = transitions.clone();
        let region = crate::core::tree::find_in_tree_mut(&mut root, |el| {
            el.template_id.as_deref() == Some(region_id.as_str())
        })
        .expect("notification hover region");
        let on_hover = region.on_hover.clone().unwrap();
        region.on_hover = Some(std::rc::Rc::new(RefCell::new(move |hovered| {
            seen.borrow_mut().push(hovered);
            (on_hover.borrow_mut())(hovered);
        })));
        let close_id = format!("toast-close-{}", ids[1]);
        let (bounds, _) = find_paint_aabb_by_focus_id(&root, &close_id, [0.0, 0.0])
            .expect("close button must be laid out");
        let close = [bounds[0] + bounds[2] / 2.0, bounds[1] + bounds[3] / 2.0];
        let body = [bounds[0] - 30.0, close[1]];

        // Enter the interactive child directly, then move within the notification.
        for point in [close, body, close] {
            apply_pointer_hover_reconcile(&root, point[0], point[1], true);
            assert!(
                toast_timer_deadline(ids[1]).is_none(),
                "{layout:?}: child hover must freeze dismissal"
            );
            if layout == ToastLayout::Stack {
                assert!(
                    stack_expanded_now(),
                    "close button hover must expand the stack"
                );
                assert!(
                    toast_timer_deadline(ids[0]).is_none(),
                    "stack hover must freeze every toast"
                );
            } else {
                assert!(
                    toast_timer_deadline(ids[0]).is_some(),
                    "list hover must only freeze its own toast"
                );
            }
            assert!(
                crate::core::reactivity::use_signal(&format!("{close_id}_hover"), || false).peek()
                    == (point == close),
                "close button must retain its own hover chrome"
            );
        }
        apply_pointer_hover_reconcile(&root, 799.0, 599.0, true);
        assert_eq!(
            *transitions.borrow(),
            [true, false],
            "moving within the toast must not collapse or briefly resume timers"
        );
        assert!(!stack_expanded_now());
        assert!(
            ids.iter().all(|id| toast_timer_deadline(*id).is_some()),
            "leaving must resume timers"
        );
        clear_hover_tracking();
        toast_queue().set(vec![]);
        for id in ids {
            timer::clear(id);
        }
    }
}

#[test]
fn toast_chrome_false_skips_semantic_paint() {
    let mut item = sample_item();
    item.chrome = false;
    let card = build_toast_card(
        &item,
        swipe_dirs_for_position(ToastPosition::TopRight),
        None,
        false,
    );
    assert!(!variant_chrome_enabled(&card.attrs));
    let chromed = build_toast_card(
        &sample_item(),
        swipe_dirs_for_position(ToastPosition::TopRight),
        None,
        false,
    );
    assert_ne!(card.style.bg_color, chromed.style.bg_color);
}

#[test]
fn toast_bg_override_wins_over_variant() {
    let mut item = sample_item();
    let custom = parse_color("red-500");
    item.bg_override = Some(custom);
    let card = build_toast_card(
        &item,
        swipe_dirs_for_position(ToastPosition::TopRight),
        None,
        false,
    );
    assert_eq!(card.style.bg_color, custom);
    assert!(is_variant_style_overridden(
        &card.attrs,
        VariantStyleField::Bg
    ));
}

#[test]
fn toast_utility_class_applied_after_chrome() {
    let mut item = sample_item();
    item.utility_class = Some("bg-card".into());
    let card = build_toast_card(
        &item,
        swipe_dirs_for_position(ToastPosition::TopRight),
        None,
        false,
    );
    assert_eq!(card.style.bg_color, parse_color("card"));
}
