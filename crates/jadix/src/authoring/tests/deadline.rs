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

#[test]
#[cfg(feature = "product-tests")]
fn virtual_list_viewability_uses_per_row_deadlines_and_visibility_lifecycle() {
    use crate::components::virtual_list::{ListViewability, VirtualListHandle, virtual_list};
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_viewport_h_signal_key};
    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if element.attrs.contains_key("__jadix_vlist_range") {
            element.reactive_sync.clone().unwrap().run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn poll(layout: &mut LayoutTree, now: Instant) -> Poll {
        let mut tasks = layout.deadlines.prepare(true);
        assert_eq!(tasks.len(), 1);
        tasks.pop().unwrap().dispatch(now)
    }
    for measured in [false, true] {
        let (_owner, ()) = create_root(|| {
            let id = if measured {
                "viewability-measured"
            } else {
                "viewability-fixed"
            };
            let offset = use_signal(&scroll_offset_y_signal_key(id), || 5.0_f32);
            let viewport = use_signal(&scroll_viewport_h_signal_key(id), || 40.0_f32);
            let revision = Rc::new(Cell::new(0));
            let data_revision = revision.clone();
            let changes = Rc::new(RefCell::new(Vec::new()));
            let seen = changes.clone();
            let handle = VirtualListHandle::default();
            let callback_handle = handle.clone();
            let builder = virtual_list(id)
                .item_count(100_000)
                .item_height(20.0)
                .height(40.0)
                .overscan(8)
                .handle(handle)
                .viewability(ListViewability::ItemFraction(0.75))
                .viewability(ListViewability::ItemFraction(f64::NAN))
                .minimum_view_time(Duration::from_millis(100))
                .data_key_fn(move || data_revision.get().to_string())
                .on_viewable_items_change(move |change| {
                    assert!(callback_handle.viewport().is_some() || change.viewable.is_empty());
                    assert_eq!(callback_handle.row_offset(1), Some(20.0));
                    // A callback can mutate a signal after all geometry borrows end.
                    viewport.set_silent(viewport.peek());
                    seen.borrow_mut().push(change.viewable);
                });
            let mut root: Element = if measured {
                builder.measure_rows()
            } else {
                builder
            }
            .into();
            let mut layout = LayoutTree::new();
            sync(&mut root, &mut layout);
            root.layout_bounds = [0.0, 0.0, 100.0, 40.0];
            track(&mut layout, &mut root);
            let start = Instant::now();
            assert_eq!(
                poll(&mut layout, start).next_wake,
                Some(start + Duration::from_millis(100))
            );
            assert!(changes.borrow().is_empty());
            offset.set_silent(20.0);
            sync(&mut root, &mut layout);
            assert!(
                poll(&mut layout, start + Duration::from_millis(50))
                    .next_wake
                    .is_some()
            );
            assert!(
                poll(&mut layout, start + Duration::from_millis(100))
                    .next_wake
                    .is_some()
            );
            assert_eq!(*changes.borrow(), [vec![1]]);
            assert!(
                poll(&mut layout, start + Duration::from_millis(150))
                    .next_wake
                    .is_none()
            );
            assert_eq!(*changes.borrow(), [vec![1], vec![1, 2]]);
            assert!(
                poll(&mut layout, start + Duration::from_millis(200))
                    .next_wake
                    .is_none()
            );
            assert_eq!(changes.borrow().len(), 2);
            root.layout_bounds[1] = 900.0;
            track(&mut layout, &mut root);
            assert!(layout.deadlines.prepare(true).is_empty());
            root.layout_bounds[1] = 0.0;
            track(&mut layout, &mut root);
            assert!(
                poll(&mut layout, start + Duration::from_millis(1000))
                    .next_wake
                    .is_some()
            );
            assert!(changes.borrow().last().unwrap().is_empty());
            assert!(
                poll(&mut layout, start + Duration::from_millis(1100))
                    .next_wake
                    .is_none()
            );
            assert_eq!(changes.borrow().last().unwrap(), &[1, 2]);
            revision.set(1);
            sync(&mut root, &mut layout);
            assert!(
                poll(&mut layout, start + Duration::from_millis(1200))
                    .next_wake
                    .is_some()
            );
            assert!(changes.borrow().last().unwrap().is_empty());
            assert!(
                poll(&mut layout, start + Duration::from_millis(1300))
                    .next_wake
                    .is_none()
            );
            viewport.set_silent(0.0);
            sync(&mut root, &mut layout);
            assert!(
                poll(&mut layout, start + Duration::from_millis(1400))
                    .next_wake
                    .is_none()
            );
            assert!(changes.borrow().last().unwrap().is_empty());
            drop(root);
            assert!(layout.deadlines.prepare(true).is_empty());
            let mut plain: Element = virtual_list("no-viewability-observer")
                .item_count(10)
                .into();
            plain.layout_bounds = [0.0, 0.0, 100.0, 40.0];
            track(&mut layout, &mut plain);
            assert!(layout.deadlines.prepare(true).is_empty());
        });
    }
}

#[test]
#[cfg(feature = "product-tests")]
fn virtual_wrap_viewability_tracks_flat_indices_reflow_and_continuous_dwell() {
    use crate::components::virtual_grid::{VirtualGridHandle, virtual_wrap};
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;
    use crate::virtualization::ListViewability;
    fn sync(element: &mut Element, layout: &mut LayoutTree) {
        if element
            .template_id
            .as_deref()
            .is_some_and(|id| id.ends_with("__grid_body"))
        {
            element.reactive_sync.clone().unwrap().run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn poll(layout: &mut LayoutTree, now: Instant) -> Poll {
        let mut tasks = layout.deadlines.prepare(true);
        assert_eq!(tasks.len(), 1);
        tasks.pop().unwrap().dispatch(now)
    }
    let (_owner, ()) = create_root(|| {
        let keys = ScrollViewportSignalKeys::new("viewability-wrap");
        let offset = use_signal(&keys.offset_y, || 5.0_f32);
        let width = use_signal(&keys.viewport_w, || 270.0_f32);
        let height = use_signal(&keys.viewport_h, || 40.0_f32);
        let count = Rc::new(Cell::new(100_001));
        let revision = Rc::new(Cell::new(0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let handle = VirtualGridHandle::default();
        let callback_handle = handle.clone();
        let mut root: Element = virtual_wrap("viewability-wrap")
            .handle(handle.clone())
            .item_count_fn({
                let count = count.clone();
                move || count.get()
            })
            .data_key_fn({
                let revision = revision.clone();
                move || revision.get().to_string()
            })
            .minimum_item_width(80.0)
            .item_height(20.0)
            .height(40.0)
            .overscan(2)
            .scroll_ahead(true)
            .max_mounted_cells(64)
            .viewability(ListViewability::ItemFraction(0.75))
            .viewability(ListViewability::ItemFraction(f64::NAN))
            .minimum_view_time(Duration::from_millis(100))
            .on_viewable_items_change({
                let changes = changes.clone();
                move |change| {
                    assert!(callback_handle.viewport().is_some() || change.viewable.is_empty());
                    // Reading geometry and mutating signals is safe after layout borrows end.
                    let _ = callback_handle.item_origin(0);
                    width.set_silent(width.peek());
                    changes.borrow_mut().push(change);
                }
            })
            .into();
        let mut layout = LayoutTree::new();
        sync(&mut root, &mut layout);
        root.layout_bounds = [0.0, 0.0, 270.0, 40.0];
        track(&mut layout, &mut root);
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        assert_eq!(poll(&mut layout, at(0)).next_wake, Some(at(100)));
        assert!(changes.borrow().is_empty());
        offset.set_silent(20.0);
        sync(&mut root, &mut layout);
        assert_eq!(poll(&mut layout, at(50)).next_wake, Some(at(100)));
        assert_eq!(poll(&mut layout, at(100)).next_wake, Some(at(150)));
        assert_eq!(changes.borrow().last().unwrap().viewable, [3, 4, 5]);
        assert!(poll(&mut layout, at(150)).next_wake.is_none());
        assert_eq!(
            changes.borrow().last().unwrap().viewable,
            [3, 4, 5, 6, 7, 8]
        );
        width.set_silent(180.0);
        sync(&mut root, &mut layout);
        assert_eq!(poll(&mut layout, at(200)).next_wake, Some(at(300)));
        assert_eq!(changes.borrow().last().unwrap().viewable, [3, 4, 5]);
        assert_eq!(
            changes
                .borrow()
                .last()
                .unwrap()
                .changed()
                .collect::<Vec<_>>(),
            [(6, false), (7, false), (8, false)]
        );
        assert!(poll(&mut layout, at(300)).next_wake.is_none());
        assert_eq!(changes.borrow().last().unwrap().viewable, [2, 3, 4, 5]);
        let delivered = changes.borrow().len();
        assert!(poll(&mut layout, at(350)).next_wake.is_none());
        assert_eq!(changes.borrow().len(), delivered);
        // Actual last-row data only: no padded slot and no overscan/predicted rows.
        assert!(handle.scroll_to_end());
        sync(&mut root, &mut layout);
        poll(&mut layout, at(400));
        assert!(poll(&mut layout, at(500)).next_wake.is_none());
        assert_eq!(
            changes.borrow().last().unwrap().viewable,
            [99_998, 99_999, 100_000]
        );
        root.layout_bounds[1] = 900.0;
        track(&mut layout, &mut root);
        assert!(layout.deadlines.prepare(true).is_empty());
        root.layout_bounds[1] = 0.0;
        track(&mut layout, &mut root);
        assert_eq!(poll(&mut layout, at(1000)).next_wake, Some(at(1100)));
        assert!(changes.borrow().last().unwrap().viewable.is_empty());
        assert!(poll(&mut layout, at(1100)).next_wake.is_none());
        assert!(layout.deadlines.prepare(false).is_empty());
        assert_eq!(poll(&mut layout, at(1500)).next_wake, Some(at(1600)));
        poll(&mut layout, at(1600));
        revision.set(1);
        sync(&mut root, &mut layout);
        assert_eq!(poll(&mut layout, at(1700)).next_wake, Some(at(1800)));
        assert!(changes.borrow().last().unwrap().viewable.is_empty());
        poll(&mut layout, at(1800));
        count.set(7);
        handle.scroll_to_end();
        sync(&mut root, &mut layout);
        assert_eq!(poll(&mut layout, at(1900)).next_wake, Some(at(2000)));
        assert!(changes.borrow().last().unwrap().viewable.is_empty());
        poll(&mut layout, at(2000));
        assert_eq!(changes.borrow().last().unwrap().viewable, [4, 5, 6]);
        width.set_silent(0.0);
        sync(&mut root, &mut layout);
        assert!(poll(&mut layout, at(2100)).next_wake.is_none());
        assert!(changes.borrow().last().unwrap().viewable.is_empty());
        width.set_silent(180.0);
        sync(&mut root, &mut layout);
        assert_eq!(poll(&mut layout, at(2200)).next_wake, Some(at(2300)));
        poll(&mut layout, at(2300));
        // A capacity failure invalidates the candidate range; recovery earns new dwell.
        count.set(100_001);
        height.set_silent(1000.0);
        sync(&mut root, &mut layout);
        assert!(poll(&mut layout, at(2400)).next_wake.is_none());
        assert!(changes.borrow().last().unwrap().viewable.is_empty());
        assert!(handle.viewport().is_none());
        height.set_silent(40.0);
        sync(&mut root, &mut layout);
        assert_eq!(poll(&mut layout, at(2500)).next_wake, Some(at(2600)));
        poll(&mut layout, at(2600));
        drop(root);
        assert!(layout.deadlines.prepare(true).is_empty());
        let mut plain: Element = virtual_wrap("no-wrap-observer").item_count(10).into();
        plain.layout_bounds = [0.0, 0.0, 100.0, 40.0];
        track(&mut layout, &mut plain);
        assert!(layout.deadlines.prepare(true).is_empty());
    });
}
