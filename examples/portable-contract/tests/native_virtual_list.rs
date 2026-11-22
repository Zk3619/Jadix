use jadix_components::{
    __portable_runtime::virtual_list::virtual_list, authoring::native::ElementProjection,
    prelude::*,
};
use jadix_native::{HeadlessRoot, presentation::*};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

fn rows(tree: &PresentationNode) -> BTreeMap<String, NodeId> {
    let mut result = BTreeMap::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        if let NodeContent::Text(text) = &node.descriptor.content {
            result.insert(text.clone(), node.id);
        }
        pending.extend(&node.children);
    }
    result
}

#[test]
fn viewport_reuses_overlapping_rows_and_retires_scopes_with_their_context() {
    let created = Rc::new(Cell::new(0));
    let retired = Rc::new(Cell::new(0));
    let creates = created.clone();
    let retires = retired.clone();
    let row_context = create_context::<String>();
    let (owner, mut element) = create_root(|| {
        provide_context(row_context, String::from("row"));
        virtual_list(10_000, 20.0, move |index| {
            creates.set(creates.get() + 1);
            let cleanup = retires.clone();
            on_cleanup(move || cleanup.set(cleanup.get() + 1));
            let context = use_context(row_context).unwrap();
            text(format!("{context}-{index}"))
        })
        .h(100.0)
        .overscan(1)
        .scroll_ahead(false)
        .build()
    });
    let root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let first = projection.snapshot(&mut element).unwrap();
    let before = rows(&first);
    assert!(before.len() <= 7);
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: first.id,
                payload: EventPayload::ScrollViewport {
                    x: 0.0,
                    y: 60.0,
                    width: 200.0,
                    height: 100.0,
                },
            },
        )
        .unwrap();
    let second = projection.snapshot(&mut element).unwrap();
    let after = rows(&second);
    assert!(after.len() <= 7);
    assert!(after.contains_key("row-7"));
    for (key, id) in &before {
        if let Some(retained) = after.get(key) {
            assert_eq!(id, retained);
        }
    }
    assert_eq!(
        retired.get(),
        before
            .keys()
            .filter(|key| !after.contains_key(*key))
            .count()
    );
    drop(element);
    drop(owner);
    assert_eq!(created.get(), retired.get());
}


#[test]
fn visible_range_excludes_overscan_and_same_row_can_be_revealed_again() {
    use std::cell::RefCell;
    let ranges = Rc::new(RefCell::new(Vec::new()));
    let changes = ranges.clone();
    let (_owner, (mut element, focus)) = create_root(|| {
        let focus = create_signal(None::<(usize, usize)>);
        let element = virtual_list(1000, 20., |index| text(index.to_string()))
            .h(100.).overscan(2).scroll_ahead(false)
            .focused_index_request_fn(move || focus.get())
            .on_visible_range_change(move |range| changes.borrow_mut().push(range))
            .build();
        (element, focus)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert_eq!(*ranges.borrow(), vec![Some(0..5)]);
    let scroll = |element: &mut Element, y| projection.dispatch(element, &PlatformEvent {
        node: first.id, payload: EventPayload::ScrollViewport { x: 0., y, width: 200., height: 100. },
    }).unwrap();
    scroll(&mut element, 60.);
    let next = projection.snapshot(&mut element).unwrap();
    assert!(rows(&next).contains_key("1")); // Overscan still owns this row.
    assert_eq!(ranges.borrow().last(), Some(&Some(3..8)));
    focus.set(Some((50, 1)));
    let focused = projection.snapshot(&mut element).unwrap();
    assert_eq!(focused.descriptor.style.scroll_to, Some([0., 920.]));
    scroll(&mut element, 0.);
    let manual = projection.snapshot(&mut element).unwrap();
    assert_eq!(manual.descriptor.style.scroll_to, None);
    focus.set(Some((50, 2)));
    let repeated = projection.snapshot(&mut element).unwrap();
    assert_eq!(repeated.descriptor.style.scroll_to, Some([0., 920.]));
    assert_eq!(ranges.borrow().last(), Some(&Some(46..51)));
}

#[test]
fn per_request_behavior_mounts_immediate_destination_and_tracks_animated_viewport() {
    use jadix_native::presentation::style::ScrollBehavior;
    use jadix_components::virtualization::ListScrollAlign;
    let (_owner, (mut element, request)) = create_root(|| {
        let request = create_signal(None);
        let element = virtual_list(1000, 20., |index| text(index.to_string()))
            .h(100.).overscan(0).scroll_ahead(false).smooth_scroll(true)
            .scroll_request_with_behavior_fn(move || request.get()).build();
        (element, request)
    });
    let projection = ElementProjection::new(RootId::new());
    projection.snapshot(&mut element).unwrap();
    request.set(Some((50, 1, ListScrollAlign::Start, ScrollBehavior::Instant)));
    let jumped = projection.snapshot(&mut element).unwrap();
    assert_eq!(jumped.descriptor.style.scroll_to, Some([0., 1000.]));
    assert_eq!(jumped.descriptor.style.scroll_behavior, Some(ScrollBehavior::Instant));
    assert!(rows(&jumped).contains_key("50"));
    assert!(!rows(&jumped).contains_key("0"));
    request.set(Some((100, 2, ListScrollAlign::Start, ScrollBehavior::Smooth)));
    let pending = projection.snapshot(&mut element).unwrap();
    assert_eq!(pending.descriptor.style.scroll_to, Some([0., 2000.]));
    assert_eq!(pending.descriptor.style.scroll_behavior, Some(ScrollBehavior::Smooth));
    assert!(rows(&pending).contains_key("50"));
    assert!(!rows(&pending).contains_key("100"));
    projection.dispatch(&mut element, &PlatformEvent {node: pending.id, payload: EventPayload::ScrollViewport {
        x: 0., y: 1400., width: 200., height: 100.,
    }}).unwrap();
    let moving = projection.snapshot(&mut element).unwrap();
    assert!(rows(&moving).contains_key("70"));
    assert_eq!(moving.descriptor.style.scroll_to, None);
}

#[test]
fn horizontal_reversed_reveal_uses_logical_indices_and_native_width() {
    use jadix_components::virtualization::ListScrollAlign;
    let (_owner, (mut element, request)) = create_root(|| {
        let request = create_signal(None);
        let list = virtual_list(100, 20., |index| text(index.to_string()))
            .horizontal().reversed(true).w(60.).h(30.).overscan(0).scroll_ahead(false)
            .scroll_request_fn(move || request.get()).build();
        (list, request)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert_eq!(rows(&first).keys().cloned().collect::<Vec<_>>(), ["97", "98", "99"]);
    request.set(Some((90, 1, ListScrollAlign::Start)));
    let reveal = projection.snapshot(&mut element).unwrap();
    assert_eq!(reveal.descriptor.style.scroll_to, Some([180., 0.]));
    assert!(rows(&reveal).contains_key("90"));
    projection.dispatch(&mut element, &PlatformEvent { node: reveal.id,
        payload: EventPayload::ScrollViewport {x: 200., y: 0., width: 40., height: 500.}
    }).unwrap();
    let moved = projection.snapshot(&mut element).unwrap();
    assert_eq!(rows(&moved).keys().cloned().collect::<Vec<_>>(), ["88", "89"]);
}

#[test]
fn keyed_reorder_preserves_owner_state_and_updates_reactive_index() {
    use jadix_components::__portable_runtime::virtual_list::virtual_list_indexed;
    let created = Rc::new(Cell::new(0));
    let creates = created.clone();
    let (_owner, (mut element, order)) = create_root(|| {
        let order = create_signal(vec!["a", "b", "c"]);
        let element = virtual_list_indexed(3, 20., move |index| {
            creates.set(creates.get()+1);
            let mut child = text("");
            child.text_binding = Some(Rc::new(move || format!("{}:{}", order.get()[index.get()], index.get())));
            child
        }).key(move |index| order.get()[index].to_owned()).h(60.).overscan(0).build();
        (element, order)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = rows(&projection.snapshot(&mut element).unwrap());
    order.set(vec!["c", "a", "b"]);
    let after = rows(&projection.snapshot(&mut element).unwrap());
    assert_eq!(created.get(), 3);
    for (old, new) in [("a:0","a:1"),("b:1","b:2"),("c:2","c:0")] {
        assert_eq!(first[old], after[new]);
    }
}

fn measured_layout(projection: &ElementProjection, element: &mut Element, width: f32) -> (PresentationNode, bool) {
    let tree = projection.snapshot(element).unwrap();
    let mut layout = jadix_native::layout::NativeLayout::new(RootLifetime::new(tree.id.root));
    let mut trees = vec![tree];
    layout.layout(&mut trees, [width, 300.], 0, |request, _| Ok(MeasureResult {
        key: request.key, width: 20., height: 20., baseline: None,
    })).unwrap();
    let mut frames = BTreeMap::new();
    let mut pending = vec![&trees[0]];
    while let Some(node) = pending.pop() { frames.insert(node.id, node.descriptor.frame); pending.extend(&node.children); }
    let changed = projection.apply_layout(element, |id| frames.get(&id).copied());
    (trees.remove(0), changed)
}

#[test]
fn measured_rows_preserve_keyed_anchor_after_growth_and_reorder() {
    use jadix_components::__portable_runtime::virtual_list::{virtual_list_indexed, VirtualListState};
    let state = VirtualListState::default();
    let retained_state = state.clone();
    let (_owner, (mut element, order, revision)) = create_root(|| {
        let order = create_signal((0..20).collect::<Vec<_>>());
        let revision = create_signal(0u64);
        let list = virtual_list_indexed(20, 20., move |index| {
            let mut row = Element::new().child(text(""));
            row.descriptor_binding = Some(Rc::new(move |descriptor| {
                descriptor.style.height = Some(if order.get()[index.get()] == 0 { 40. } else { 20. });
            }));
            row.children[0].text_binding = Some(Rc::new(move || order.get()[index.get()].to_string()));
            row
        }).key(move |index| order.get()[index].to_string()).data_revision(revision)
            .state(retained_state).measure_rows().h(60.).overscan(1).scroll_ahead(false).build();
        (list, order, revision)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    projection.dispatch(&mut element, &PlatformEvent { node: first.id, payload: EventPayload::ScrollViewport {
        x: 0., y: 25., width: 200., height: 60.,
    }}).unwrap();
    assert!(measured_layout(&projection, &mut element, 200.).1);
    let corrected = projection.snapshot(&mut element).unwrap();
    assert_eq!(corrected.descriptor.style.scroll_to, Some([0., 45.]));
    assert_eq!(state.offset(), 45.);
    let before = rows(&corrected);
    order.set((0..20).map(|i| if i == 0 { 1 } else if i == 1 { 0 } else { i }).collect());
    revision.set(1);
    let reordered = projection.snapshot(&mut element).unwrap();
    assert_eq!(state.offset(), 5.);
    assert_eq!(before["1"], rows(&reordered)["1"]);
    assert!(measured_layout(&projection, &mut element, 200.).1 == false);
}

#[test]
fn measured_cross_axis_resize_remeasures_rows_and_then_settles() {
    let (_owner, mut element) = create_root(|| virtual_list(30, 20., |index| {
        Element::new().h(40.).child(text(index.to_string()))
    }).measure_rows().h(60.).overscan(1).scroll_ahead(false).build());
    let projection = ElementProjection::new(RootId::new());
    assert!(measured_layout(&projection, &mut element, 200.).1);
    assert!(!measured_layout(&projection, &mut element, 200.).1);
    assert!(measured_layout(&projection, &mut element, 100.).1);
    assert!(!measured_layout(&projection, &mut element, 100.).1);
}

#[test]
fn measured_follow_end_tracks_append_until_user_leaves_trailing_edge() {
    use jadix_components::__portable_runtime::virtual_list::VirtualListState;
    let state = VirtualListState::default();
    let retained_state = state.clone();
    let (_owner, (mut element, count)) = create_root(|| {
        let count = create_signal(10usize);
        let list = virtual_list(10, 20., |index| text(index.to_string())).count(count)
            .state(retained_state).follow_end(true).h(60.).overscan(0).scroll_ahead(false).build();
        (list, count)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert_eq!(state.offset(), 140.);
    count.set(11);
    projection.snapshot(&mut element).unwrap();
    assert_eq!(state.offset(), 160.);
    projection.dispatch(&mut element, &PlatformEvent {node: first.id, payload: EventPayload::ScrollViewport {
        x: 0., y: 40., width: 200., height: 60.,
    }}).unwrap();
    count.set(12);
    let away = projection.snapshot(&mut element).unwrap();
    assert_eq!(state.offset(), 40.);
    assert_eq!(away.descriptor.style.scroll_to, None);
}
