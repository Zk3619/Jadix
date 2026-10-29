use super::*;
use crate::authoring::presentation;
use crate::core::reactivity::create_root;
use crate::layout::LayoutTree;
use std::time::Duration;

fn track(layout: &mut LayoutTree, root: &mut Element) {
    layout.begin_presentation_frame();
    presentation::sync_tree(
        root,
        layout,
        presentation::Context {
            phase: presentation::Phase::AfterLayout,
            viewport: [200.0, 200.0],
            width_budget: None,
        },
    );
}

#[test]
fn dropped_node_or_window_retires_already_prepared_work() {
    let calls = Rc::new(Cell::new(0));
    let mut root = Element::new();
    let captured = calls.clone();
    attach(&mut root, Visibility::Always, move |_| {
        captured.set(captured.get() + 1);
        Poll::default()
    });
    let mut layout = LayoutTree::new();
    track(&mut layout, &mut root);
    let pending = layout.deadlines.prepare(true);
    drop(root);
    for task in pending {
        task.dispatch(Instant::now());
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(Rc::strong_count(&calls), 1);
    assert!(layout.deadlines.prepare(true).is_empty());
    let captured = calls.clone();
    let mut root = Element::new();
    attach(&mut root, Visibility::Always, move |_| {
        captured.set(captured.get() + 1);
        Poll::default()
    });
    track(&mut layout, &mut root);
    let pending = layout.deadlines.prepare(true);
    drop(layout);
    for task in pending {
        task.dispatch(Instant::now());
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(
        Rc::strong_count(&calls),
        1,
        "a retained node cannot retain a closed window's callbacks"
    );
    drop(root);
}

#[test]
fn owner_disposal_releases_captures_while_node_is_retained() {
    let captured = Rc::new(());
    let state = captured.clone();
    let (owner, mut root) = create_root(|| {
        let mut root = Element::new();
        attach(&mut root, Visibility::Always, move |_| {
            let _ = &state;
            Poll::default()
        });
        root
    });
    let mut layout = LayoutTree::new();
    track(&mut layout, &mut root);
    let pending = layout.deadlines.prepare(true);
    assert_eq!(Rc::strong_count(&captured), 2);
    drop(owner);
    assert_eq!(Rc::strong_count(&captured), 1);
    for task in pending {
        assert!(task.dispatch(Instant::now()).next_wake.is_none());
    }
    assert!(layout.deadlines.prepare(true).is_empty());
}

#[test]
fn offscreen_and_hidden_work_pauses_and_resumes_without_catchup() {
    let resumed = Rc::new(RefCell::new(Vec::new()));
    let seen = resumed.clone();
    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 50.0, 20.0];
    attach(&mut root, Visibility::Visible, move |context| {
        seen.borrow_mut().push(context.resumed);
        Poll {
            redraw: false,
            next_wake: Some(context.now + Duration::from_secs(1)),
        }
    });
    let mut layout = LayoutTree::new();
    track(&mut layout, &mut root);
    for task in layout.deadlines.prepare(true) {
        assert!(task.dispatch(Instant::now()).next_wake.is_some());
    }
    for task in layout.deadlines.prepare(true) {
        task.dispatch(Instant::now());
    }
    root.layout_bounds[1] = 900.0;
    track(&mut layout, &mut root);
    assert!(layout.deadlines.prepare(true).is_empty());
    root.layout_bounds[1] = 0.0;
    track(&mut layout, &mut root);
    for task in layout.deadlines.prepare(true) {
        task.dispatch(Instant::now());
    }
    assert!(layout.deadlines.prepare(false).is_empty());
    for task in layout.deadlines.prepare(true) {
        task.dispatch(Instant::now());
    }
    assert_eq!(*resumed.borrow(), [true, false, true, true]);
}

#[test]
fn visible_image_deadline_keeps_ticking_while_partly_outside_viewport() {
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let mut image = Element::new().image_texture("deadline-stream");
    image.layout_bounds = [10.0, 10.0, 80.0, 60.0];
    attach(&mut image, Visibility::Visible, move |_| {
        seen.set(seen.get() + 1);
        Poll {
            redraw: true,
            next_wake: None,
        }
    });
    let mut layout = LayoutTree::new();
    for top in [10.0, -20.0, -59.0, 199.0] {
        image.layout_bounds[1] = top;
        track(&mut layout, &mut image);
        let pending = layout.deadlines.prepare(true);
        assert_eq!(pending.len(), 1, "partly visible image stopped at {top}");
        assert!(
            pending
                .into_iter()
                .next()
                .unwrap()
                .dispatch(Instant::now())
                .redraw
        );
    }
    assert_eq!(calls.get(), 4);
    image.layout_bounds[1] = 900.0;
    track(&mut layout, &mut image);
    assert!(layout.deadlines.prepare(true).is_empty());
}

#[test]
fn callbacks_run_after_layout_borrow_release_and_restore_window_scope() {
    let layout = Rc::new(RefCell::new(LayoutTree::new()));
    let callback_layout = layout.clone();
    let mut root = {
        let _scope = window_scope::enter(WindowScope::Secondary(71));
        let mut root = Element::new();
        attach(&mut root, Visibility::Always, move |_| {
            assert_eq!(window_scope::active(), WindowScope::Secondary(71));
            callback_layout.borrow_mut().request_caret_blink();
            Poll {
                redraw: true,
                next_wake: None,
            }
        });
        track(&mut layout.borrow_mut(), &mut root);
        root
    };
    let pending = layout.borrow_mut().deadlines.prepare(true);
    assert_eq!(pending.len(), 1);
    for task in pending {
        assert!(task.dispatch(Instant::now()).redraw);
    }
    assert_eq!(window_scope::active(), WindowScope::Primary);
    assert!(layout.borrow_mut().take_caret_blink_request());
    root.layout_bounds[1] = 900.0;
}

#[test]
#[cfg(feature = "product-tests")]
fn actual_toast_host_and_carousel_install_window_deadlines() {
    create_root(|| {
        crate::components::toast::toast("node deadline")
            .duration(2000)
            .show();
        let toaster = crate::components::toast::host::paint_toast_host();
        let carousel: Element = crate::components::carousel::carousel("deadline-carousel")
            .autoplay_ms(3000)
            .slide(|| Element::new().text("slide"))
            .into();
        let mut root = Element::new().child(toaster).child(carousel);
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0; 2]);
        track(&mut layout, &mut root);
        let polls: Vec<_> = layout
            .deadlines
            .prepare(true)
            .into_iter()
            .map(|task| task.dispatch(Instant::now()))
            .collect();
        assert_eq!(polls.len(), 2);
        assert!(polls.iter().all(|poll| poll.next_wake.is_some()));
    });
}
