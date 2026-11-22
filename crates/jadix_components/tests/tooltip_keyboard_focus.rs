use jadix_components::components::tooltip::{
    tooltip_scope, use_tooltip, TooltipTrigger, TooltipTriggerProps,
};
use jadix_components::core::reactivity::{create_root, use_signal};
use jadix_components::core::tree::Element;
use jadix_components::style::TextSpan;
use jadix_components::utils::hover_intent::hover_intent_next_wake_at;
use jadix_components::{app::pointer_events::MouseEvent, authoring::keys};
use std::{cell::Cell, rc::Rc};
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{Key, ModifiersState, NamedKey},
};

fn build_tooltip(
    focus_visible_initially: bool,
    open_delay: Option<u64>,
) -> (
    jadix_components::core::reactivity::OwnerHandle,
    jadix_components::core::reactivity::Signal<bool>,
    Element,
    jadix_components::core::reactivity::Signal<bool>,
) {
    build_tooltip_with_child(
        focus_visible_initially,
        open_delay,
        Element::new().text("Help"),
    )
}

fn build_tooltip_with_child(
    focus_visible_initially: bool,
    open_delay: Option<u64>,
    child: Element,
) -> (
    jadix_components::core::reactivity::OwnerHandle,
    jadix_components::core::reactivity::Signal<bool>,
    Element,
    jadix_components::core::reactivity::Signal<bool>,
) {
    let (owner, (open, tree, focus_visible)) = create_root(|| {
        let focused_id = use_signal("__jadix_button_focus_id", String::new);
        let focus_visible = use_signal("__jadix_focus_visible", || false);
        focused_id.set("tip-keyboard__tooltip_trigger".to_string());
        focus_visible.set(focus_visible_initially);
        let open = use_signal("tip-keyboard-open", || false);
        let tree = tooltip_scope("tip-keyboard", Some(open), || {
            let _ctx = use_tooltip().expect("tooltip scope");
            TooltipTrigger(TooltipTriggerProps {
                id: String::new(),
                open_delay,
                close_delay: None,
                children: Some(child),
                base: None,
            })
        });
        (open, tree, focus_visible)
    });
    (owner, open, tree, focus_visible)
}

#[test]
fn restored_panel_focus_keeps_the_ring_without_reopening_tooltips() {
    use jadix_components::utils::{
        anchored_overlay::{retained_anchored_panel, AnchoredOverlayContentOpts},
        roving::{focus_by_id, focus_id_only},
    };
    let (_owner, ()) = create_root(|| {
        let focus = use_signal("__jadix_button_focus_id", String::new);
        let visible = use_signal("__jadix_focus_visible", || false);
        let hint = use_signal("restore-hint", || false);
        let panel_open = use_signal("restore-panel", || false);
        let trigger = "restore-trigger";
        let tree = tooltip_scope("restore-hint", Some(hint), || {
            TooltipTrigger(TooltipTriggerProps {
                id: String::new(),
                open_delay: None,
                close_delay: None,
                base: None,
                children: Some(Element::new().focus_id(trigger).text("Open panel")),
            })
        });
        let opts = AnchoredOverlayContentOpts {
            focus_id: trigger.into(),
            ..Default::default()
        };
        for keyboard in [true, false] {
            visible.set(keyboard);
            focus_id_only(trigger);
            panel_open.set(true);
            let mut item = Element::new().focus_id("restore-panel-item");
            item.focusable = true;
            let _ = retained_anchored_panel("restore-panel", panel_open, &opts, item);
            focus_by_id("restore-panel-item");
            visible.set(keyboard);
            panel_open.set(false);
            let _ = retained_anchored_panel("restore-panel", panel_open, &opts, Element::new());
            assert_eq!(focus.peek(), trigger);
            assert_eq!(
                visible.peek(),
                keyboard,
                "restoration preserves the input modality"
            );
            assert!(
                !hint.peek(),
                "returning from a panel must not reopen its trigger hint"
            );
            // A real pointer re-entry is still allowed after restored focus.
            tree.on_hover.as_ref().unwrap().borrow_mut()(true);
            tree.on_hover.as_ref().unwrap().borrow_mut()(false);
            focus_by_id(trigger);
            assert!(hint.peek(), "fresh keyboard focus can show the hint again");
            assert!(escape(&tree, ElementState::Pressed));
            focus_id_only("elsewhere");
        }
    });
}

#[test]
fn tooltip_press_cancels_pending_open_and_preserves_trigger_actions() {
    let clicks = Rc::new(Cell::new(0));
    let presses = Rc::new(Cell::new(0));
    let child = Element::new()
        .child(Element::new().text("Action"))
        .on_click({
            let calls = clicks.clone();
            move || calls.set(calls.get() + 1)
        })
        .on_pointer_down({
            let calls = presses.clone();
            move |_| calls.set(calls.get() + 1)
        });
    let (owner, open, tree, _) = build_tooltip_with_child(false, None, child);
    let hover = tree.on_hover.as_ref().unwrap();
    hover.borrow_mut()(true);
    assert!(!open.peek());
    assert!(hover_intent_next_wake_at().is_some());

    let press = tree.on_mousedown.as_ref().unwrap();
    press.borrow_mut()(&MouseEvent::new(0.0, 0.0, MouseButton::Left, 1));
    tree.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!((presses.get(), clicks.get()), (1, 1));
    hover.borrow_mut()(true);
    assert!(!open.peek());
    assert!(
        hover_intent_next_wake_at().is_none(),
        "press cancels pending open"
    );

    hover.borrow_mut()(false);
    hover.borrow_mut()(true);
    assert!(
        hover_intent_next_wake_at().is_some(),
        "real re-entry arms the delay"
    );
    owner.dispose();
    assert!(hover_intent_next_wake_at().is_none());
}

#[test]
fn tooltip_press_and_escape_stay_dismissed_until_pointer_reentry() {
    let (_owner, open, tree, _) = build_tooltip(false, Some(0));
    let hover = tree.on_hover.as_ref().unwrap();
    hover.borrow_mut()(true);
    assert!(open.peek());
    tree.on_mousedown.as_ref().unwrap().borrow_mut()(&MouseEvent::new(
        0.0,
        0.0,
        MouseButton::Left,
        1,
    ));
    assert!(!open.peek());
    hover.borrow_mut()(true);
    assert!(!open.peek());
    hover.borrow_mut()(false);
    hover.borrow_mut()(true);
    assert!(open.peek());
    assert!(escape(&tree, ElementState::Pressed));
    assert!(!open.peek());
    hover.borrow_mut()(true);
    assert!(!open.peek());
    assert!(
        !escape(&tree, ElementState::Pressed),
        "dismissed hint releases Escape"
    );
    hover.borrow_mut()(false);
    hover.borrow_mut()(true);
    assert!(open.peek());
}

fn escape(tree: &Element, state: ElementState) -> bool {
    keys::handlers(tree).iter().any(|handler| {
        handler.dispatch(keys::Context {
            key: &Key::Named(NamedKey::Escape),
            modifiers: ModifiersState::empty(),
            state,
            repeat: false,
        })
    })
}

#[test]
fn tooltip_escape_keeps_keyboard_focus_and_recovers_on_refocus() {
    let (owner, open, tree, focus_visible) = build_tooltip(true, None);
    assert!(open.peek());
    assert!(!escape(&tree, ElementState::Released));
    assert!(open.peek());
    assert!(escape(&tree, ElementState::Pressed));
    assert!(!open.peek());
    assert!(focus_visible.peek());
    focus_visible.set(true);
    assert!(!open.peek());
    focus_visible.set(false);
    focus_visible.set(true);
    assert!(open.peek());
    owner.dispose();
    assert!(
        !escape(&tree, ElementState::Pressed),
        "disposed shortcuts expire"
    );
}

#[test]
fn tooltip_opens_for_keyboard_focus_but_not_pointer_focus() {
    let (_keyboard_owner, keyboard_open, _tree, _focus_visible) = build_tooltip(true, None);
    assert!(keyboard_open.peek());
    drop(_keyboard_owner);

    let (_pointer_owner, pointer_open, tree, focus_visible) = build_tooltip(false, Some(0));
    assert!(!pointer_open.peek());

    let on_hover = tree
        .on_hover
        .as_ref()
        .expect("trigger hover handler")
        .clone();
    (on_hover.borrow_mut())(true);
    assert!(pointer_open.peek());

    focus_visible.set(true);
    focus_visible.set(false);
    assert!(pointer_open.peek());

    (on_hover.borrow_mut())(false);
    assert!(!pointer_open.peek());
}

#[test]
fn tooltip_defaults_to_shared_hover_delay_and_trigger_can_override_it() {
    let (_owner, (default_delay, overridden_delay)) = create_root(|| {
        let default_value = use_signal("tip-default-delay-observed", || 0_u64);
        let _ = tooltip_scope("tip-default-delay", None, || {
            default_value.set(use_tooltip().expect("tooltip scope").open_delay_ms.get());
            Element::new()
        });
        let overridden_value = use_signal("tip-custom-delay-observed", || 0_u64);
        let _ = tooltip_scope("tip-custom-delay", None, || {
            let trigger = TooltipTrigger(TooltipTriggerProps {
                id: String::new(),
                open_delay: Some(75),
                close_delay: None,
                children: Some(Element::new().text("Help")),
                base: None,
            });
            let _ = trigger;
            overridden_value.set(use_tooltip().expect("tooltip scope").open_delay_ms.get());
            Element::new()
        });
        (default_value.get(), overridden_value.get())
    });

    assert_eq!(default_delay, 300);
    assert_eq!(overridden_delay, 75);
}

#[test]
fn tooltip_keeps_authored_text_identity_styles_events_and_wrapping() {
    use jadix_components::components::tooltip::tooltip_bubble;
    use jadix_components::core::tree::LayoutTree;

    let clicks = Rc::new(Cell::new(0));
    let spans = vec![
        TextSpan::new("Read the ").italic(),
        TextSpan::new("linked documentation ".repeat(8))
            .weight(700)
            .href("https://example.com"),
    ];
    let make_child = || {
        Element::new()
            .template_id("authored-caption")
            .focus_id("authored-focus")
            .text_spans(spans.clone())
            .text_size(19.0)
            .font_weight(600.0)
            .text_color([12, 34, 56, 255])
            .text_line_height(28.0)
            .w("100%")
            .on_click({
                let calls = clicks.clone();
                move || calls.set(calls.get() + 1)
            })
    };
    let child = make_child();
    let (_owner, open, trigger, _) = build_tooltip_with_child(false, Some(0), make_child());
    assert_eq!(trigger.template_id, child.template_id);
    assert_eq!(trigger.focus_id, child.focus_id);
    assert_eq!(trigger.text_style, child.text_style);
    assert_eq!(trigger.text_spans, child.text_spans);
    assert!(trigger.focusable);
    trigger.on_hover.as_ref().unwrap().borrow_mut()(true);
    assert!(open.peek());
    trigger.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(clicks.get(), 1);
    assert!(!open.peek());

    let bubble = tooltip_bubble(make_child());
    let body = &bubble.children[0];
    assert_eq!(body.template_id, child.template_id);
    assert_eq!(body.text_style, child.text_style);
    assert_eq!(body.text_spans.as_deref(), Some(spans.as_slice()));
    body.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(clicks.get(), 2);
    let mut root = Element::new().flex_col().child(bubble);
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut root, 400.0, 900.0, [-1.0, -1.0]);
    let wide = root.children[0].children[0].layout_bounds;
    layout.build_and_layout(&mut root, 160.0, 900.0, [-1.0, -1.0]);
    let narrow = root.children[0].children[0].layout_bounds;
    assert!(
        narrow[2] < wide[2] && narrow[3] > wide[3],
        "{wide:?} -> {narrow:?}"
    );
    assert_eq!(
        root.children[0].children[0].text_spans.as_deref(),
        Some(spans.as_slice())
    );
    let plain = tooltip_bubble(Element::new().text("Plain"));
    assert_eq!(plain.children[0].text_style.size, Some(12.0));
}
