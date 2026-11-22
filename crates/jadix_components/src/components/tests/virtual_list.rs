use super::*;

#[test]
fn render_mutations_resample_initial_and_live_windows_with_bounded_retries() {
    use crate::core::scroll_viewport::activate_scroll_viewport;
    use crate::core::tree::LayoutTree;

    fn rendered_revisions(root: &Element, revisions: &mut Vec<usize>) {
        if let Some(value) = root.attrs.get("render_revision") {
            revisions.push(value.parse().unwrap());
        }
        for child in &root.children {
            rendered_revisions(child, revisions);
        }
    }

    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for parent in [false, true] {
            for horizontal in [false, true] {
                for inverted in [false, true] {
                    for measured in [false, true] {
                        let id = format!("render-edit-{parent}-{horizontal}-{inverted}-{measured}");
                        let owner_id = if parent {
                            format!("{id}-owner")
                        } else {
                            id.clone()
                        };
                        let count = Rc::new(Cell::new(20));
                        let revision = Rc::new(Cell::new(0));
                        let mode = Rc::new(Cell::new(1));
                        let renders = Rc::new(Cell::new(0));
                        let reports = Rc::new(Cell::new(0));
                        let handle = VirtualListHandle::default();
                        let builder = virtual_list(&id)
                            .handle(handle.clone())
                            .height(60.0)
                            .item_height(20.0)
                            .overscan(0)
                            .item_count_fn({
                                let count = count.clone();
                                move || count.get()
                            })
                            .data_key_fn({
                                let revision = revision.clone();
                                move || revision.get().to_string()
                            })
                            .on_visible_range_change({
                                let count = count.clone();
                                let handle = handle.clone();
                                let reports = reports.clone();
                                move |_| {
                                    assert_eq!(handle.viewport().unwrap().item_count, count.get());
                                    reports.set(reports.get() + 1);
                                }
                            })
                            .render_item({
                                let count = count.clone();
                                let revision = revision.clone();
                                let mode = mode.clone();
                                let renders = renders.clone();
                                move |index| {
                                    assert!(index < count.get(), "stale render index: {index}");
                                    renders.set(renders.get() + 1);
                                    if mode.get() == 1 {
                                        count.set(2);
                                        revision.set(revision.get() + 1);
                                        mode.set(0);
                                    } else if mode.get() == 2 {
                                        revision.set(revision.get() + 1);
                                    } else if mode.get() == 3 {
                                        revision.set(revision.get() + 1);
                                        mode.set(0);
                                    }
                                    let mut row = main_extent(Element::new(), horizontal, 20.0);
                                    row.set_attr("render_revision", revision.get().to_string());
                                    row
                                }
                            });
                        let builder = if parent {
                            builder.use_parent_scroll()
                        } else {
                            builder
                        };
                        let builder = if horizontal {
                            builder.horizontal()
                        } else {
                            builder
                        };
                        let builder = if inverted {
                            builder.inverted()
                        } else {
                            builder
                        };
                        let mut root: Element = if measured {
                            builder.measure_rows().into()
                        } else {
                            builder.into()
                        };
                        if parent {
                            root = activate_scroll_viewport(
                                Element::new().w(60.0).h(60.0).child(root),
                                &owner_id,
                                if horizontal { "auto" } else { "visible" },
                                if horizontal { "visible" } else { "auto" },
                                "auto",
                                "auto",
                            );
                        }
                        let mut layout = LayoutTree::new();
                        layout.build_and_layout(&mut root, 60.0, 60.0, [-1.0; 2]);
                        let viewport_key = if horizontal {
                            scroll_viewport_w_signal_key
                        } else {
                            scroll_viewport_h_signal_key
                        };
                        use_signal(&viewport_key(&owner_id), || 0.0_f32).set_silent(60.0);
                        let sync = parent_list_body(&mut root).reactive_sync.clone().unwrap();
                        sync.run(parent_list_body(&mut root), &mut layout);
                        assert_eq!(handle.viewport().unwrap().item_count, 2, "{id}");
                        let mut values = Vec::new();
                        rendered_revisions(&root, &mut values);
                        assert_eq!(values, vec![1; 2], "initial mutation: {id}");

                        // Same-count edits must discard every row from the old data version.
                        revision.set(2);
                        mode.set(3);
                        sync.run(parent_list_body(&mut root), &mut layout);
                        values.clear();
                        rendered_revisions(&root, &mut values);
                        assert_eq!(values, vec![3; 2], "same-count edit: {id}");

                        let before = renders.get();
                        let before_reports = reports.get();
                        revision.set(4);
                        mode.set(2);
                        sync.run(parent_list_body(&mut root), &mut layout);
                        assert_eq!(renders.get() - before, 4, "retry budget: {id}");
                        assert_eq!(
                            reports.get(),
                            before_reports,
                            "unstable pass must not notify"
                        );
                        mode.set(0);
                        sync.run(parent_list_body(&mut root), &mut layout);
                        values.clear();
                        rendered_revisions(&root, &mut values);
                        assert_eq!(values, vec![revision.get(); 2], "recovery: {id}");
                    }
                }
            }
        }
    });
}

fn parent_list_body(element: &mut Element) -> &mut Element {
    if element.attrs.contains_key(REUSED_ATTR) {
        return element;
    }
    let index = element
        .children
        .iter()
        .position(|child| {
            fn contains(el: &Element) -> bool {
                el.attrs.contains_key(REUSED_ATTR) || el.children.iter().any(contains)
            }
            contains(child)
        })
        .unwrap();
    parent_list_body(&mut element.children[index])
}

#[test]
fn parent_scroll_uses_real_ancestor_origin_full_range_and_signed_visibility() {
    use crate::core::scroll_viewport::activate_scroll_viewport;
    use crate::core::tree::LayoutTree;
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                let id = format!("parent-list-{horizontal}-{measured}");
                let owner_id = format!("{id}-owner");
                let cross_id = format!("{id}-cross");
                let handle = VirtualListHandle::default();
                let starts = Rc::new(Cell::new(0));
                let ends = Rc::new(Cell::new(0));
                let builder = virtual_list(&id)
                    .use_parent_scroll()
                    .inverted()
                    .initial_scroll_at_end()
                    .follow_end(true)
                    .handle(handle.clone())
                    .item_count(20)
                    .item_height(20.0)
                    .overscan(3)
                    .retained_indices_fn(|| vec![19])
                    .sticky_header_indices([0])
                    .load_more_threshold(0.0)
                    .on_load_more_at_start({
                        let calls = starts.clone();
                        move || calls.set(calls.get() + 1)
                    })
                    .on_load_more({
                        let calls = ends.clone();
                        move || calls.set(calls.get() + 1)
                    })
                    .render_item(move |_| main_extent(Element::new(), horizontal, 20.0));
                let builder = if horizontal {
                    builder.horizontal()
                } else {
                    builder
                };
                let list: Element = if measured {
                    builder.measure_rows().into()
                } else {
                    builder.into()
                };
                assert!(
                    list.focus_id.is_none(),
                    "parent list creates no private viewport"
                );
                assert_eq!(handle.0.order.get(), ListOrder::Forward);
                let cross = activate_scroll_viewport(
                    main_extent(
                        Element::new()
                            .w(100.0)
                            .h(100.0)
                            .flex_shrink(0.0)
                            .child(list),
                        horizontal,
                        400.0,
                    ),
                    &cross_id,
                    if horizontal { "visible" } else { "auto" },
                    if horizontal { "auto" } else { "visible" },
                    "auto",
                    "auto",
                );
                let stack: Element = if horizontal {
                    hstack().into()
                } else {
                    vstack().into()
                };
                let mut root = activate_scroll_viewport(
                    stack
                        .w(100.0)
                        .h(100.0)
                        .child(main_extent(
                            Element::new().flex_shrink(0.0),
                            horizontal,
                            150.0,
                        ))
                        .child(cross)
                        .child(main_extent(
                            Element::new().flex_shrink(0.0),
                            horizontal,
                            250.0,
                        )),
                    &owner_id,
                    if horizontal { "auto" } else { "visible" },
                    if horizontal { "visible" } else { "auto" },
                    "auto",
                    "auto",
                );
                let mut layout = LayoutTree::new();
                layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                let (offset_key, viewport_key, content_key): (
                    fn(&str) -> String,
                    fn(&str) -> String,
                    fn(&str) -> String,
                ) = if horizontal {
                    (
                        scroll_offset_x_signal_key,
                        scroll_viewport_w_signal_key,
                        scroll_content_w_signal_key,
                    )
                } else {
                    (
                        scroll_offset_y_signal_key,
                        scroll_viewport_h_signal_key,
                        scroll_content_h_signal_key,
                    )
                };
                let offset = use_signal(&offset_key(&owner_id), || 0.0_f32);
                let viewport = use_signal(&viewport_key(&owner_id), || 0.0_f32);
                viewport.set_silent(100.0);
                use_signal(&content_key(&owner_id), || 0.0_f32).set_silent(800.0);
                let body = parent_list_body(&mut root);
                assert_eq!(
                    layout.ancestor_scroll_viewport(body.layout_node_id().unwrap(), horizontal),
                    Some((owner_id.as_str(), 150.0))
                );
                let sync = body.reactive_sync.clone().unwrap();
                offset.set_silent(0.0);
                sync.run(body, &mut layout);
                let state = handle.viewport().unwrap();
                assert_eq!((state.offset, state.viewport_extent), (-150.0, 100.0));
                assert!(state.visible.is_empty());
                assert_eq!(
                    body.children
                        .iter()
                        .filter(|el| el.attrs.contains_key(ROW_KEY_ATTR))
                        .count(),
                    1,
                    "offscreen overscan mounts no edge window"
                );
                assert_eq!((starts.get(), ends.get()), (0, 0));
                offset.set_silent(125.0);
                sync.run(body, &mut layout);
                assert_eq!(handle.viewport().unwrap().visible, 0..4);
                assert_eq!(handle.viewport().unwrap().offset, -25.0);
                assert_eq!(starts.get(), 1);
                offset.set_silent(650.0);
                sync.run(body, &mut layout);
                assert_eq!(
                    offset.peek(),
                    650.0,
                    "parent footer keeps its own scroll range"
                );
                assert!(handle.viewport().unwrap().visible.is_empty());
                assert!(handle.sticky_header().is_none());
                assert!(handle.scroll_to_index(19, ListScrollAlign::Start, 0.0));
                sync.run(body, &mut layout);
                assert_eq!(
                    offset.peek(),
                    530.0,
                    "aligning a last row may exceed the list's own maximum"
                );
                assert_eq!(handle.viewport().unwrap().visible, 19..20);
                assert!(handle.scroll_to_end());
                sync.run(body, &mut layout);
                assert_eq!(
                    offset.peek(),
                    450.0,
                    "end means embedded list end, not parent footer end"
                );
                viewport.set_silent(0.0);
                assert!(handle.scroll_to_index(8, ListScrollAlign::Start, 0.0));
                sync.run(body, &mut layout);
                assert!(handle.viewport().is_none());
                assert_eq!(offset.peek(), 450.0);
                viewport.set_silent(100.0);
                sync.run(body, &mut layout);
                assert_eq!(offset.peek(), 310.0);
                // Let a measured destination finish its post-layout alignment
                // before testing an unrelated preceding-sibling resize.
                layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                sync.run(parent_list_body(&mut root), &mut layout);
                assert_eq!(offset.peek(), 310.0);
                // A preceding sibling changes the natural origin without
                // transferring ownership or anchoring the parent's offset.
                root.children[0] =
                    main_extent(std::mem::take(&mut root.children[0]), horizontal, 200.0);
                layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                use_signal(&content_key(&owner_id), || 0.0_f32).set_silent(850.0);
                let body = parent_list_body(&mut root);
                assert_eq!(
                    layout.ancestor_scroll_viewport(body.layout_node_id().unwrap(), horizontal),
                    Some((owner_id.as_str(), 200.0))
                );
                sync.run(body, &mut layout);
                assert_eq!(offset.peek(), 310.0);
                assert_eq!(handle.viewport().unwrap().offset, 110.0);
                // Remove the owning viewport identity without changing Taffy style.
                root.focus_id = None;
                layout.update_taffy_node_style(&root);
                let body = parent_list_body(&mut root);
                assert!(
                    layout
                        .ancestor_scroll_viewport(body.layout_node_id().unwrap(), horizontal)
                        .is_none()
                );
                assert!(handle.scroll_to_end());
                sync.run(body, &mut layout);
                assert!(handle.viewport().is_none());
                assert!(handle.0.request.get().is_some());
                assert_eq!(offset.peek(), 310.0);
                let retired_node = body.layout_node_id().unwrap();
                root.children.clear();
                layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                assert!(
                    layout
                        .ancestor_scroll_viewport(retired_node, horizontal)
                        .is_none()
                );
            }
        }
    });
}

#[test]
fn parent_scroll_compensates_only_measured_changes_entirely_above_viewport() {
    use crate::core::scroll_viewport::activate_scroll_viewport;
    use crate::core::tree::LayoutTree;
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for horizontal in [false, true] {
            let id = format!("parent-measure-{horizontal}");
            let owner_id = format!("{id}-owner");
            let handle = VirtualListHandle::default();
            let builder = virtual_list(&id)
                .use_parent_scroll()
                .handle(handle.clone())
                .measure_rows()
                .item_count(20)
                .item_height(20.0)
                .overscan(0)
                .retained_indices_fn(|| vec![1, 2, 19])
                .render_item(move |_| main_extent(Element::new(), horizontal, 20.0));
            let list: Element = if horizontal {
                builder.horizontal().into()
            } else {
                builder.into()
            };
            let stack: Element = if horizontal {
                hstack().into()
            } else {
                vstack().into()
            };
            let mut root = activate_scroll_viewport(
                stack
                    .w(100.0)
                    .h(100.0)
                    .child(main_extent(
                        Element::new().flex_shrink(0.0),
                        horizontal,
                        100.0,
                    ))
                    .child(list)
                    .child(main_extent(
                        Element::new().flex_shrink(0.0),
                        horizontal,
                        300.0,
                    )),
                &owner_id,
                if horizontal { "auto" } else { "visible" },
                if horizontal { "visible" } else { "auto" },
                "auto",
                "auto",
            );
            let mut layout = LayoutTree::new();
            layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
            let (offset_key, viewport_key, content_key): (
                fn(&str) -> String,
                fn(&str) -> String,
                fn(&str) -> String,
            ) = if horizontal {
                (
                    scroll_offset_x_signal_key,
                    scroll_viewport_w_signal_key,
                    scroll_content_w_signal_key,
                )
            } else {
                (
                    scroll_offset_y_signal_key,
                    scroll_viewport_h_signal_key,
                    scroll_content_h_signal_key,
                )
            };
            let offset = use_signal(&offset_key(&owner_id), || 0.0_f32);
            offset.set_silent(180.0);
            use_signal(&viewport_key(&owner_id), || 0.0_f32).set_silent(100.0);
            use_signal(&content_key(&owner_id), || 0.0_f32).set_silent(800.0);
            let body = parent_list_body(&mut root);
            let sync = body.reactive_sync.clone().unwrap();
            sync.run(body, &mut layout);
            for row in &mut body.children {
                let measured = match row.attrs.get(ROW_KEY_ATTR).map(String::as_str) {
                    Some("i:1") => Some(30.0),
                    Some("i:2") => Some(40.0),
                    Some("i:4") => Some(35.0),
                    Some("i:19") => Some(50.0),
                    _ => None,
                };
                if let Some(extent) = measured {
                    row.layout_bounds[if horizontal { 2 } else { 3 }] = extent;
                }
            }
            sync.run(body, &mut layout);
            assert_eq!(
                offset.peek(),
                210.0,
                "only rows 1 and 2 were entirely above the old viewport"
            );
            assert_eq!(handle.viewport().unwrap().offset, 110.0);
            assert_eq!(handle.viewport().unwrap().visible.start, 4);
            assert_eq!(handle.row_offset(20), Some(475.0));
            // Once the parent publishes the new extent, repeated sync is stable.
            use_signal(&content_key(&owner_id), || 0.0_f32).set_silent(875.0);
            sync.run(body, &mut layout);
            assert_eq!(offset.peek(), 210.0);
        }
    });
}

#[test]
fn retained_rows_preserve_disjoint_layout_sticky_slots_and_keyed_state() {
    use crate::core::tree::LayoutTree;
    for horizontal in [false, true] {
        for measured in [false, true] {
            let count = 12;
            let keys: Vec<_> = (0..count).map(|i| format!("key-{i}")).collect();
            let extent = |i| {
                if measured {
                    20.0 + (i % 3) as f32 * 7.0
                } else {
                    20.0
                }
            };
            let mut axis = VirtualAxis::new(axis_keys(count, Some(&keys)), 20.0).unwrap();
            axis.set_gap(3.0).unwrap();
            for i in 0..count {
                axis.measure(&axis_key(i, Some(&keys)), f64::from(extent(i)))
                    .unwrap();
            }
            let slots = ListSlots::new(
                [
                    Some((11.0, Element::new())),
                    Some((17.0, Element::new())),
                    None,
                ],
                horizontal,
                false,
            );
            let calls = Cell::new(0);
            let render = |i| {
                calls.set(calls.get() + 1);
                main_extent(Element::new(), horizontal, extent(i))
            };
            let mut body: Element = if horizontal {
                hstack().into()
            } else {
                vstack().into()
            };
            let geometry = measured.then_some(&axis);
            let apply = |body: &mut Element, start, end, retained: &[usize], sticky| {
                apply_body_window(
                    horizontal,
                    body,
                    "retained-layout",
                    count,
                    "",
                    20.0,
                    3.0,
                    start,
                    end,
                    &render,
                    geometry,
                    Some(&keys),
                    None,
                    &|| count,
                    &|| true,
                    sticky,
                    true,
                    [11.0, 17.0],
                    &slots,
                    retained,
                )
            };
            apply(
                &mut body,
                4,
                6,
                &[11, 0, 9, 2, 0, 5, count, usize::MAX],
                Some(ListStickyHeader {
                    index: 1,
                    translation: 0.0,
                }),
            );
            let mounted = |body: &Element| {
                body.children
                    .iter()
                    .filter_map(|row| {
                        row.attrs.get(ROW_KEY_ATTR).and_then(|key| {
                            keys.iter()
                                .position(|candidate| key == &format!("k:{candidate}"))
                        })
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(mounted(&body), [0, 1, 2, 4, 5, 9, 11]);
            assert_eq!(
                calls.get(),
                7,
                "no rows bridging retained islands are rendered"
            );
            let mut layout = LayoutTree::new();
            let total = axis.total() as f32 + 28.0;
            layout.build_and_layout(
                &mut body,
                if horizontal { total } else { 100.0 },
                if horizontal { 100.0 } else { total },
                [-1.0; 2],
            );
            let position = usize::from(!horizontal);
            let dimension = position + 2;
            let ids: HashSet<_> = body
                .children
                .iter()
                .filter_map(|el| el.template_id.clone())
                .collect();
            assert_eq!(
                ids.len(),
                body.children
                    .iter()
                    .filter(|el| el.template_id.is_some())
                    .count(),
                "every gap has a distinct layout identity"
            );
            for (i, row) in mounted(&body).into_iter().zip(
                body.children
                    .iter()
                    .filter(|row| row.attrs.contains_key(ROW_KEY_ATTR)),
            ) {
                assert_eq!(
                    row.layout_bounds[position] - body.layout_bounds[position],
                    11.0 + axis.position(i).unwrap() as f32
                );
                assert_eq!(row.layout_bounds[dimension], extent(i));
            }
            let last = body.children.last().unwrap();
            assert_eq!(
                last.layout_bounds[position] + last.layout_bounds[dimension]
                    - body.layout_bounds[position],
                total
            );
            let survivors: HashMap<_, _> = body
                .children
                .iter()
                .filter(|row| row.attrs.contains_key(ROW_KEY_ATTR))
                .map(|row| (row.attrs[ROW_KEY_ATTR].clone(), row.layout_node_id()))
                .collect();
            apply(
                &mut body,
                6,
                8,
                &[0, 2, 9, 11],
                Some(ListStickyHeader {
                    index: 2,
                    translation: 0.0,
                }),
            );
            assert_eq!(mounted(&body), [0, 2, 6, 7, 9, 11]);
            assert_eq!(calls.get(), 9);
            for row in body
                .children
                .iter()
                .filter(|row| row.attrs.contains_key(ROW_KEY_ATTR))
            {
                if let Some(node) = survivors.get(&row.attrs[ROW_KEY_ATTR]) {
                    assert_eq!(&row.layout_node_id(), node);
                }
            }
            assert_eq!(body.attrs[DROPPED_ATTR], "3");
            assert_eq!(
                apply(
                    &mut body,
                    6,
                    8,
                    &[11, 2, 0, 9, 2, 6],
                    Some(ListStickyHeader {
                        index: 2,
                        translation: 0.0
                    })
                ),
                LayoutStatus::Clean
            );
            assert_eq!(
                calls.get(),
                9,
                "duplicate or already visible pins do not rebuild"
            );
            apply(&mut body, 6, 8, &[], None);
            assert_eq!(mounted(&body), [6, 7]);
            assert_eq!(body.attrs[DROPPED_ATTR], "4");
        }
    }
}

#[test]
fn public_retained_rows_poll_without_scroll_or_visibility_changes_on_either_order() {
    use crate::core::tree::LayoutTree;
    fn body(el: &mut Element) -> &mut Element {
        if el.attrs.contains_key(RANGE_ATTR) {
            return el;
        }
        el.children
            .iter_mut()
            .find_map(|child| {
                fn has_body(el: &Element) -> bool {
                    el.attrs.contains_key(RANGE_ATTR) || el.children.iter().any(has_body)
                }
                has_body(child).then(|| body(child))
            })
            .expect("builder installs its virtual body")
    }
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for horizontal in [false, true] {
            for inverted in [false, true] {
                for measured in [false, true] {
                    let id = format!("retained-public-{horizontal}-{inverted}-{measured}");
                    let retained = Rc::new(RefCell::new(vec![90, 50, 90, 100, usize::MAX]));
                    let render_calls = Rc::new(RefCell::new(Vec::new()));
                    let ranges = Rc::new(RefCell::new(Vec::new()));
                    let builder = virtual_list(&id)
                        .item_count(100)
                        .item_height(20.0)
                        .height(60.0)
                        .overscan(0)
                        .follow_end(false)
                        .maintain_visible_content_position(false)
                        .item_key_fn(|i| format!("key-{i}"))
                        .retained_indices_fn({
                            let retained = retained.clone();
                            move || retained.borrow().clone()
                        })
                        .on_visible_range_change({
                            let ranges = ranges.clone();
                            move |range| ranges.borrow_mut().push(range)
                        })
                        .render_item({
                            let calls = render_calls.clone();
                            move |i| {
                                calls.borrow_mut().push(i);
                                main_extent(Element::new(), horizontal, 20.0)
                            }
                        });
                    let builder = if horizontal {
                        builder.horizontal()
                    } else {
                        builder
                    };
                    let builder = if inverted {
                        builder.inverted()
                    } else {
                        builder
                    };
                    let builder = if measured {
                        builder.measure_rows()
                    } else {
                        builder
                    };
                    let mut root: Element = builder.into();
                    let body = body(&mut root);
                    let mounted = |body: &Element| {
                        let mut keys = body
                            .children
                            .iter()
                            .filter_map(|row| row.attrs.get(ROW_KEY_ATTR).cloned())
                            .collect::<Vec<_>>();
                        keys.sort();
                        keys
                    };
                    assert_eq!(
                        mounted(body),
                        ["k:key-0", "k:key-1", "k:key-2", "k:key-50", "k:key-90"]
                    );
                    for row in body.children.iter_mut().filter(|row| {
                        row.attrs.get(ROW_KEY_ATTR).map(String::as_str) == Some("k:key-50")
                    }) {
                        row.attrs.insert("retained-state".into(), "alive".into());
                    }
                    let offset_key = if horizontal {
                        scroll_offset_x_signal_key
                    } else {
                        scroll_offset_y_signal_key
                    };
                    let offset = use_signal(&offset_key(&id), || 0.0_f32);
                    offset.set_silent(if inverted { 1940.0 } else { 0.0 });
                    let viewport_key = if horizontal {
                        scroll_viewport_w_signal_key
                    } else {
                        scroll_viewport_h_signal_key
                    };
                    use_signal(&viewport_key(&id), || 0.0_f32).set_silent(60.0);
                    let before = offset.peek();
                    *retained.borrow_mut() = vec![70, 50, 0, 70, usize::MAX];
                    let sync = body.reactive_sync.clone().unwrap();
                    sync.run(body, &mut LayoutTree::new());
                    assert_eq!(
                        mounted(body),
                        ["k:key-0", "k:key-1", "k:key-2", "k:key-50", "k:key-70"]
                    );
                    assert_eq!(offset.peek(), before, "retention must not navigate");
                    let survivor = body
                        .children
                        .iter()
                        .find(|row| {
                            row.attrs.get(ROW_KEY_ATTR).map(String::as_str) == Some("k:key-50")
                        })
                        .unwrap();
                    assert_eq!(
                        survivor.attrs.get("retained-state").map(String::as_str),
                        Some("alive")
                    );
                    assert_eq!(
                        render_calls.borrow().len(),
                        6,
                        "only the newly retained row is rendered"
                    );
                    assert_eq!(ranges.borrow().as_slice(), [Some(0..3)]);
                    retained.borrow_mut().clear();
                    sync.run(body, &mut LayoutTree::new());
                    assert_eq!(mounted(body), ["k:key-0", "k:key-1", "k:key-2"]);
                    assert_eq!(render_calls.borrow().len(), 6);
                    assert_eq!(
                        ranges.borrow().len(),
                        1,
                        "outliers never enter visible range events"
                    );
                }
            }
        }
    });
}

// Set up the same adapter used by the public builder, with no slots or sticky rows.
#[allow(clippy::too_many_arguments)]
fn render_test_window(
    horizontal: bool,
    list_id: &str,
    item_count: usize,
    item_height: f32,
    item_gap: f32,
    start: usize,
    end: usize,
    render_item: &dyn Fn(usize) -> Element,
    axis: Option<&VirtualAxis<String>>,
    stable_keys: Option<&[String]>,
    animate_item_changes: Option<LayoutAnimation>,
) -> Element {
    let mut body: Element = if horizontal {
        hstack().into()
    } else {
        vstack().into()
    };
    apply_body_window(
        horizontal,
        &mut body,
        list_id,
        item_count,
        "",
        item_height,
        item_gap,
        start,
        end,
        render_item,
        axis,
        stable_keys,
        animate_item_changes,
        &|| item_count,
        &|| true,
        None,
        false,
        [0.0; 2],
        &ListSlots::new([None, None, None], horizontal, false),
        &[],
    );
    body
}

#[test]
fn spaced_rows_match_layout_with_spacers_and_clear_reused_trailing_margin() {
    use crate::core::tree::LayoutTree;
    for horizontal in [false, true] {
        let position = usize::from(!horizontal);
        let dimension = position + 2;
        for measured in [false, true] {
            let mut axis = VirtualAxis::new((0..6).map(|i| format!("i:{i}")), 20.0).unwrap();
            axis.set_gap(10.0).unwrap();
            for (start, end) in [(0, 3), (2, 5), (4, 6), (3, 3)] {
                let mut body = render_test_window(
                    horizontal,
                    "spaced",
                    6,
                    20.0,
                    10.0,
                    start,
                    end,
                    &|_| {
                        if horizontal {
                            Element::new().w(20.0)
                        } else {
                            Element::new().h(20.0)
                        }
                    },
                    measured.then_some(&axis),
                    None,
                    None,
                );
                let mut layout = LayoutTree::new();
                layout.build_and_layout(
                    &mut body,
                    if horizontal { 170.0 } else { 100.0 },
                    if horizontal { 100.0 } else { 170.0 },
                    [-1.0, -1.0],
                );
                let last = body.children.last().unwrap();
                assert_eq!(
                    last.layout_bounds[position] + last.layout_bounds[dimension]
                        - body.layout_bounds[position],
                    170.0
                );
                for row in &body.children {
                    if let Some(index) = row
                        .template_id
                        .as_deref()
                        .and_then(|id| parse_row_index("spaced", id))
                    {
                        assert_eq!(
                            row.layout_bounds[position] - body.layout_bounds[position],
                            index as f32 * 30.0
                        );
                        assert_eq!(
                            row.layout_bounds[dimension], 20.0,
                            "gap is outside the measured border box"
                        );
                    }
                }
            }
        }
        let keys: Vec<String> = (0..6).map(|i| i.to_string()).collect();
        let mut body = render_test_window(
            horizontal,
            "spaced",
            6,
            20.0,
            10.0,
            0,
            3,
            &|_| {
                if horizontal {
                    Element::new().w(20.0)
                } else {
                    Element::new().h(20.0)
                }
            },
            None,
            Some(&keys),
            None,
        );
        body.attrs
            .insert(RANGE_ATTR.into(), window_key(6, "", 0, 3));
        for count in [3, 6] {
            apply_body_window(
                horizontal,
                &mut body,
                "spaced",
                count,
                "",
                20.0,
                10.0,
                0,
                3,
                &|_| panic!("stable keyed rows must survive resize"),
                None,
                Some(&keys),
                None,
                &|| count,
                &|| true,
                None,
                false,
                [0.0; 2],
                &ListSlots::new([None, None, None], horizontal, false),
                &[],
            );
            let row = body
                .children
                .iter()
                .find(|row| row.attrs.get(ROW_KEY_ATTR).map(String::as_str) == Some("k:2"))
                .unwrap();
            assert_eq!(
                if horizontal {
                    row.style.margin.right
                } else {
                    row.style.margin.bottom
                },
                Element::new()
                    .mb(if count == 3 { 0.0 } else { 10.0 })
                    .style
                    .margin
                    .bottom
            );
        }
    }
}
use crate::utils::virtual_list::visible_item_range;

#[test]
fn visible_range_helper() {
    let (a, b) = visible_item_range(0.0, 100.0, 20.0, 100, 1);
    assert_eq!(a, 0);
    assert!(b > a);
    let (a2, b2) = visible_item_range(40.0, 100.0, 20.0, 100, 2);
    assert_eq!(a2, 0);
    assert_eq!(b2, 9);
}

#[test]
fn focused_window_mounts_neighbors_and_reveals_offscreen_rows() {
    assert_eq!(
        focused_item_window(f32::NAN, 100.0, 20.0, 10.0, 100, 1, None),
        (0, 0, None)
    );
    assert_eq!(
        focused_item_window(-10.0, 100.0, 20.0, 0.0, 100, 1, None),
        (0, 6, None)
    );
    let (start, end, reveal) = focused_item_window(0.0, 100.0, 20.0, 0.0, 100, 1, Some(3));
    assert_eq!((start, end, reveal), (0, 6, None));

    let (start, end, reveal) = focused_item_window(0.0, 100.0, 20.0, 0.0, 100, 1, Some(20));
    assert_eq!(reveal, Some("start"));
    assert!(start <= 19);
    assert!(20 < end);
    assert!(end - start <= 9);

    let (start, end, reveal) = focused_item_window(200.0, 100.0, 20.0, 0.0, 100, 1, Some(3));
    assert_eq!(reveal, Some("end"));
    assert!(start <= 2);
    assert!(3 < end);
    assert!(end - start <= 9);
}

#[test]
fn virtual_list_builds() {
    let _el: Element = virtual_list("vl1")
        .item_count(100)
        .item_height(24.0)
        .focused_index_fn(|| Some(0))
        .render_item(|i| Element::new().text(format!("#{i}")))
        .into();
}

#[test]
fn virtual_list_item_motion_is_opt_in_and_uses_stable_keys() {
    let plain: Element = virtual_list("vl-motion-plain")
        .item_count(2)
        .item_key_fn(|index| format!("item-{index}"))
        .render_item(|index| Element::new().text(format!("{index}")))
        .into();
    let plain_row = crate::core::tree::find_in_tree(&plain, |node| {
        node.attrs
            .get(ROW_KEY_ATTR)
            .is_some_and(|key| key == "k:item-0")
    })
    .expect("stable-key row");
    assert!(!plain_row.attrs.contains_key("jadix_layout_id"));

    let animated: Element = virtual_list("vl-motion")
        .item_count(2)
        .item_key_fn(|index| format!("item-{index}"))
        .animate_item_changes(LayoutAnimation::Position)
        .render_item(|index| Element::new().text(format!("{index}")))
        .into();
    let animated_row = crate::core::tree::find_in_tree(&animated, |node| {
        node.attrs
            .get(ROW_KEY_ATTR)
            .is_some_and(|key| key == "k:item-0")
    })
    .expect("animated stable-key row");
    assert_eq!(
        animated_row
            .attrs
            .get("jadix_layout_id")
            .map(String::as_str),
        Some("vl-motion__vlist_item_6974656d2d30")
    );
    assert!(animated_row.layout_motion.is_some());

    let unkeyed: Element = virtual_list("vl-motion-unkeyed")
        .item_count(2)
        .animate_item_changes(LayoutAnimation::Both)
        .render_item(|index| Element::new().text(format!("{index}")))
        .into();
    let unkeyed_row = crate::core::tree::find_in_tree(&unkeyed, |node| {
        node.attrs.get(ROW_KEY_ATTR).is_some_and(|key| key == "i:0")
    })
    .expect("unkeyed row");
    assert!(!unkeyed_row.attrs.contains_key("jadix_layout_id"));
}

#[test]
fn initial_scroll_at_end_builds_the_trailing_window() {
    let el: Element = virtual_list("vl-end")
        .item_count(100)
        .item_height(20.0)
        .height(100.0)
        .overscan(0)
        .initial_scroll_at_end()
        .render_item(|i| Element::new().text(format!("{i}")))
        .into();
    let body = crate::core::tree::find_in_tree(&el, |node| node.attrs.contains_key(RANGE_ATTR))
        .expect("virtual list body");
    let rows: Vec<_> = body
        .children
        .iter()
        .filter_map(|child| {
            child
                .template_id
                .as_deref()
                .and_then(|id| parse_row_index("vl-end", id))
        })
        .collect();
    assert_eq!(rows, (95..100).collect::<Vec<_>>());
}

#[test]
fn builder_style_reaches_list_root() {
    let el: Element = virtual_list("vl-chrome")
        .item_count(3)
        .item_height(24.0)
        .style(|b| b.text_size(20.0))
        .render_item(|i| Element::new().text(format!("#{i}")))
        .into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "virtual_list .style must land on the scroll root"
    );
}

#[test]
fn build_body_total_height_matches_item_count() {
    let body = render_test_window(
        false,
        "vl",
        120,
        21.0,
        0.0,
        11,
        28,
        &|i| Element::new().text(format!("{i}")),
        None,
        None,
        None,
    );
    match crate::layout::dim_kind(body.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 120.0 * 21.0).abs() < 0.01),
        other => panic!("expected Length total height, got {other:?}"),
    }
}

#[test]
fn apply_body_window_is_idempotent_for_same_range() {
    let mut root = render_test_window(
        false,
        "vl",
        50,
        20.0,
        0.0,
        0,
        10,
        &|i| Element::new().text(format!("{i}")),
        None,
        None,
        None,
    );
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
    assert_eq!(
        apply_body_window(
            false,
            &mut root,
            "vl",
            50,
            "",
            20.0,
            0.0,
            0,
            10,
            &|i| { Element::new().text(format!("{i}")) },
            None,
            None,
            None,
            &|| 50,
            &|| true,
            None,
            false,
            [0.0; 2],
            &ListSlots::new([None, None, None], false, false),
            &[],
        ),
        LayoutStatus::Clean
    );
}

#[test]
fn apply_body_window_updates_when_item_count_changes_same_range() {
    let mut root = render_test_window(
        false,
        "vl",
        50,
        20.0,
        0.0,
        0,
        10,
        &|i| Element::new().text(format!("{i}")),
        None,
        None,
        None,
    );
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(50, "", 0, 10));
    assert_eq!(
        apply_body_window(
            false,
            &mut root,
            "vl",
            20,
            "",
            20.0,
            0.0,
            0,
            10,
            &|i| { Element::new().text(format!("n{i}")) },
            None,
            None,
            None,
            &|| 20,
            &|| true,
            None,
            false,
            [0.0; 2],
            &ListSlots::new([None, None, None], false, false),
            &[],
        ),
        LayoutStatus::StructureChanged
    );
    match crate::layout::dim_kind(root.style.size.height) {
        crate::layout::DimKind::Length(h) => assert!((h - 20.0 * 20.0).abs() < 0.01),
        other => panic!("expected Length total height, got {other:?}"),
    }
}

#[test]
fn apply_body_window_reuses_overlap_and_drops_old() {
    let mut root = render_test_window(
        false,
        "vl",
        100,
        20.0,
        0.0,
        0,
        10,
        &|i| Element::new().text(format!("r{i:03}")),
        None,
        None,
        None,
    );
    // Stamp fake layout nodes on rows 5..10 so we can assert identity after reuse.
    for child in &mut root.children {
        if let Some(i) = child
            .template_id
            .as_deref()
            .and_then(|id| parse_row_index("vl", id))
        {
            if (5..10).contains(&i) {
                child.set_layout_node_id(Some(taffy::prelude::NodeId::from(1000u64 + i as u64)));
            }
        }
    }
    root.attrs
        .insert(RANGE_ATTR.to_string(), window_key(100, "", 0, 10));

    assert_eq!(
        apply_body_window(
            false,
            &mut root,
            "vl",
            100,
            "",
            20.0,
            0.0,
            5,
            15,
            &|i| { Element::new().text(format!("r{i:03}")) },
            None,
            None,
            None,
            &|| 100,
            &|| true,
            None,
            false,
            [0.0; 2],
            &ListSlots::new([None, None, None], false, false),
            &[],
        ),
        LayoutStatus::StructureChanged
    );
    assert_eq!(root.attrs.get(REUSED_ATTR).map(String::as_str), Some("5"));
    assert_eq!(root.attrs.get(CREATED_ATTR).map(String::as_str), Some("5"));
    assert_eq!(root.attrs.get(DROPPED_ATTR).map(String::as_str), Some("5"));

    let mut texts = Vec::new();
    for c in &root.children {
        if let Some(t) = &c.text {
            texts.push(t.clone());
        }
    }
    assert!(texts.iter().any(|t| t == "r005"));
    assert!(texts.iter().any(|t| t == "r014"));
    assert!(!texts.iter().any(|t| t == "r000"));

    let node5 = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("vl__row_5"))
        .and_then(|c| c.layout_node_id());
    assert_eq!(node5, Some(taffy::prelude::NodeId::from(1005u64)));
}

#[test]
fn interaction_owners_survive_scroll_and_release_without_navigation_on_all_adapters() {
    use crate::core::scroll_viewport::activate_scroll_viewport;
    use crate::core::tree::LayoutTree;
    use crate::layout::layout_tree::{ATTR_PORTAL_KEY, hoist_portal_subtrees};
    use crate::utils::text_selection::label_selection_signal_key;
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                // Ordinary, inverted and ancestor-owned scrolling share retention.
                for mode in 0..3 {
                    let id = format!("interaction-list-{horizontal}-{measured}-{mode}");
                    let focus_id = format!("{id}-focus");
                    let label_id = format!("{id}-label");
                    let invalid_id = format!("{id}-invalid");
                    let portal_key = format!("{id}-portal");
                    let focus = use_signal("__jadix_button_focus_id", String::new);
                    focus.set_silent(focus_id.clone());
                    let selection = use_signal(&label_selection_signal_key(&label_id), || {
                        None::<(usize, usize)>
                    });
                    selection.set_silent(Some((3, 0)));
                    use_signal(&label_selection_signal_key(&invalid_id), || {
                        None::<(usize, usize)>
                    })
                    .set_silent(Some((1, 2)));
                    let calls = Rc::new(Cell::new(0));
                    let ranges = Rc::new(RefCell::new(Vec::new()));
                    let builder = virtual_list(&id)
                        .item_count(100)
                        .item_height(20.0)
                        .height(100.0)
                        .overscan(0)
                        .follow_end(false)
                        .maintain_visible_content_position(false)
                        .item_key_fn(|i| format!("key-{i}"))
                        .on_visible_range_change({
                            let ranges = ranges.clone();
                            move |r| ranges.borrow_mut().push(r)
                        })
                        .render_item({
                            let calls = calls.clone();
                            let focus_id = focus_id.clone();
                            let label_id = label_id.clone();
                            let invalid_id = invalid_id.clone();
                            let portal_key = portal_key.clone();
                            move |i| {
                                calls.set(calls.get() + 1);
                                let child = match i {
                                    0 => Element::new().focus_id(&focus_id),
                                    1 => Element::new()
                                        .focus_id(&label_id)
                                        .text("中x")
                                        .selectable(true),
                                    2 => {
                                        let mut portal = Element::new().absolute().w(10.0).h(10.0);
                                        portal.attrs.insert("jadix_portal".into(), "1".into());
                                        portal
                                            .attrs
                                            .insert(ATTR_PORTAL_KEY.into(), portal_key.clone());
                                        portal
                                    }
                                    3 => {
                                        let mut stale = Element::new().text("stale paint");
                                        stale.text_style.input_caret =
                                            Some(crate::style::InputCaretDraw::default());
                                        stale
                                    }
                                    4 => Element::new()
                                        .focus_id(&invalid_id)
                                        .text("中x")
                                        .selectable(true),
                                    _ => Element::new(),
                                };
                                main_extent(Element::new().child(child), horizontal, 20.0)
                            }
                        });
                    let builder = if horizontal {
                        builder.horizontal()
                    } else {
                        builder
                    };
                    let builder = if measured {
                        builder.measure_rows()
                    } else {
                        builder
                    };
                    let builder = match mode {
                        1 => builder.inverted(),
                        2 => builder.use_parent_scroll(),
                        _ => builder,
                    };
                    let list: Element = builder.into();
                    let owner_id = if mode == 2 {
                        format!("{id}-owner")
                    } else {
                        id.clone()
                    };
                    let mut root = if mode == 2 {
                        let stack: Element = if horizontal {
                            hstack().into()
                        } else {
                            vstack().into()
                        };
                        activate_scroll_viewport(
                            stack
                                .w(100.0)
                                .h(100.0)
                                .child(main_extent(Element::new(), horizontal, 40.0))
                                .child(list)
                                .child(main_extent(Element::new(), horizontal, 60.0)),
                            &owner_id,
                            if horizontal { "auto" } else { "visible" },
                            if horizontal { "visible" } else { "auto" },
                            "auto",
                            "auto",
                        )
                    } else {
                        list
                    };
                    let mut layout = LayoutTree::new();
                    layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                    let (offset_key, viewport_key, content_key): (
                        fn(&str) -> String,
                        fn(&str) -> String,
                        fn(&str) -> String,
                    ) = if horizontal {
                        (
                            scroll_offset_x_signal_key,
                            scroll_viewport_w_signal_key,
                            scroll_content_w_signal_key,
                        )
                    } else {
                        (
                            scroll_offset_y_signal_key,
                            scroll_viewport_h_signal_key,
                            scroll_content_h_signal_key,
                        )
                    };
                    let offset = use_signal(&offset_key(&owner_id), || 0.0_f32);
                    offset.set_silent(match mode {
                        1 => 1900.0,
                        2 => 40.0,
                        _ => 0.0,
                    });
                    use_signal(&viewport_key(&owner_id), || 0.0_f32).set_silent(100.0);
                    use_signal(&content_key(&owner_id), || 0.0_f32).set_silent(if mode == 2 {
                        2100.0
                    } else {
                        2000.0
                    });
                    let sync = parent_list_body(&mut root).reactive_sync.clone().unwrap();
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert_eq!(calls.get(), 5, "{id}: initial window");
                    let mounted = |body: &Element| {
                        let mut indices = body
                            .children
                            .iter()
                            .filter_map(|row| {
                                row.attrs
                                    .get(ROW_INDEX_ATTR)
                                    .and_then(|i| i.parse::<usize>().ok())
                            })
                            .map(|i| if mode == 1 { 99 - i } else { i })
                            .collect::<Vec<_>>();
                        indices.sort_unstable();
                        indices
                    };
                    assert_eq!(
                        mounted(parent_list_body(&mut root)),
                        [0, 1, 2, 3, 4],
                        "{id}"
                    );
                    assert!(subtree_needs_virtual_retention(
                        &parent_list_body(&mut root)
                            .children
                            .iter()
                            .find(|row| row.attrs.get(ROW_INDEX_ATTR).map(String::as_str)
                                == Some(if mode == 1 { "97" } else { "2" }))
                            .unwrap(),
                        ""
                    ));
                    hoist_portal_subtrees(&mut root);
                    layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                    let survivors: HashMap<_, _> = parent_list_body(&mut root)
                        .children
                        .iter()
                        .filter_map(|row| {
                            row.attrs
                                .get(ROW_KEY_ATTR)
                                .map(|key| (key.clone(), row.layout_node_id()))
                        })
                        .collect();
                    offset.set_silent(match mode {
                        1 => 900.0,
                        2 => 1040.0,
                        _ => 1000.0,
                    });
                    let expected_offset = offset.peek();
                    sync.run(parent_list_body(&mut root), &mut layout);
                    let body = parent_list_body(&mut root);
                    assert_eq!(mounted(body), [0, 1, 2, 50, 51, 52, 53, 54], "{id}");
                    assert_eq!(calls.get(), 10, "only five entering rows render");
                    assert_eq!(
                        offset.peek(),
                        expected_offset,
                        "retention never reveals the old focus"
                    );
                    assert_eq!(ranges.borrow().last(), Some(&Some(50..55)));
                    for row in body
                        .children
                        .iter()
                        .filter(|r| r.attrs.contains_key(ROW_KEY_ATTR))
                    {
                        if let Some(node) = survivors.get(&row.attrs[ROW_KEY_ATTR]) {
                            assert_eq!(&row.layout_node_id(), node);
                        }
                    }
                    let event_count = ranges.borrow().len();
                    focus.set_silent(String::new());
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert_eq!(
                        mounted(parent_list_body(&mut root)),
                        [1, 2, 50, 51, 52, 53, 54]
                    );
                    selection.set_silent(Some((0, 0)));
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert_eq!(
                        mounted(parent_list_body(&mut root)),
                        [2, 50, 51, 52, 53, 54]
                    );
                    parent_list_body(&mut root)
                        .children
                        .iter_mut()
                        .find(|r| {
                            r.attrs.get(ROW_INDEX_ATTR).map(String::as_str)
                                == Some(if mode == 1 { "97" } else { "2" })
                        })
                        .unwrap()
                        .children
                        .clear();
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert_eq!(mounted(parent_list_body(&mut root)), [50, 51, 52, 53, 54]);
                    hoist_portal_subtrees(&mut root);
                    fn owns_portal(el: &Element, key: &str) -> bool {
                        el.attrs.get(ATTR_PORTAL_KEY).is_some_and(|k| k == key)
                            || el.children.iter().any(|c| owns_portal(c, key))
                    }
                    assert!(!owns_portal(&root, &portal_key), "closed portal is pruned");
                    assert_eq!(ranges.borrow().len(), event_count);
                    assert_eq!(calls.get(), 10);
                    assert_eq!(offset.peek(), expected_offset);
                }
            }
        }
    });
}

#[test]
fn interaction_retention_tracks_key_moves_removal_and_content_replacement() {
    use crate::utils::text_selection::label_selection_signal_key;
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for measured in [false, true] {
            let id = format!("interaction-key-move-{measured}");
            let keys = Rc::new(RefCell::new(
                (0..12).map(|i| format!("key-{i}")).collect::<Vec<_>>(),
            ));
            let generation = Cell::new(0);
            let focus = use_signal("__jadix_button_focus_id", String::new);
            focus.set_silent(format!("{id}-key-0"));
            let selection = use_signal(&label_selection_signal_key(&format!("{id}-key-1")), || {
                None::<(usize, usize)>
            });
            selection.set_silent(Some((0, 3)));
            let render = |i: usize| {
                let key = keys.borrow()[i].clone();
                let mut child = Element::new()
                    .focus_id(format!("{id}-{key}"))
                    .text(format!("中 {key} {}", generation.get()))
                    .selectable(true);
                if key == "key-2" {
                    child.attrs.insert("jadix_portal".into(), "1".into());
                }
                main_extent(Element::new().child(child), false, 20.0)
            };
            let mut axis = VirtualAxis::new(axis_keys(12, Some(&keys.borrow())), 20.0).unwrap();
            let mut body = render_test_window(
                false,
                &id,
                12,
                20.0,
                0.0,
                0,
                4,
                &render,
                measured.then_some(&axis),
                Some(&keys.borrow()),
                None,
            );
            for row in body
                .children
                .iter_mut()
                .filter(|r| r.attrs.contains_key(ROW_KEY_ATTR))
            {
                row.attrs.insert("local-state".into(), "alive".into());
            }
            let apply =
                |body: &mut Element, revision: &str, axis: &VirtualAxis<String>, keyed: bool| {
                    let keys = keys.borrow();
                    apply_body_window(
                        false,
                        body,
                        &id,
                        keys.len(),
                        revision,
                        20.0,
                        0.0,
                        5,
                        7,
                        &render,
                        measured.then_some(axis),
                        keyed.then_some(keys.as_slice()),
                        None,
                        &|| keys.len(),
                        &|| true,
                        None,
                        false,
                        [0.0; 2],
                        &ListSlots::new([None, None, None], false, false),
                        &[],
                    )
                };
            keys.borrow_mut().rotate_left(3);
            axis.replace(axis_keys(12, Some(&keys.borrow())), 20.0)
                .unwrap();
            apply(&mut body, "", &axis, true);
            let mounted = |body: &Element| {
                body.children
                    .iter()
                    .filter_map(|r| r.attrs.get(ROW_KEY_ATTR).cloned())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                mounted(&body),
                ["k:key-8", "k:key-9", "k:key-0", "k:key-1", "k:key-2"]
            );
            for row in body.children.iter().filter(|r| {
                r.attrs
                    .get(ROW_KEY_ATTR)
                    .is_some_and(|k| ["k:key-0", "k:key-1", "k:key-2"].contains(&k.as_str()))
            }) {
                assert_eq!(
                    row.attrs.get("local-state").map(String::as_str),
                    Some("alive")
                );
                assert_eq!(
                    row.attrs[ROW_INDEX_ATTR].parse::<usize>().unwrap(),
                    keys.borrow()
                        .iter()
                        .position(|k| row.attrs[ROW_KEY_ATTR] == format!("k:{k}"))
                        .unwrap()
                );
            }
            keys.borrow_mut().retain(|k| k != "key-0");
            axis.replace(axis_keys(11, Some(&keys.borrow())), 20.0)
                .unwrap();
            apply(&mut body, "", &axis, true);
            assert_eq!(
                mounted(&body),
                ["k:key-8", "k:key-9", "k:key-1", "k:key-2"],
                "deleted focused key is released"
            );
            generation.set(1);
            apply(&mut body, "revision-1", &axis, true);
            let label = body
                .children
                .iter()
                .find(|r| r.attrs.get(ROW_KEY_ATTR).map(String::as_str) == Some("k:key-1"))
                .unwrap();
            fn find_text(el: &Element) -> Option<&str> {
                el.text
                    .as_deref()
                    .or_else(|| el.children.iter().find_map(find_text))
            }
            assert_eq!(
                find_text(label),
                Some("中 key-1 1"),
                "revision rebuilds retained content"
            );
            assert!(!label.attrs.contains_key("local-state"));
            selection.set_silent(None);
            body.children
                .iter_mut()
                .find(|r| r.attrs.get(ROW_KEY_ATTR).map(String::as_str) == Some("k:key-2"))
                .unwrap()
                .children
                .clear();
            apply(&mut body, "revision-1", &axis, true);
            assert_eq!(mounted(&body), ["k:key-8", "k:key-9"]);
            // A replacement unkeyed dataset must not inherit the old focused row.
            let mut unkeyed =
                render_test_window(false, &id, 11, 20.0, 0.0, 0, 4, &render, None, None, None);
            focus.set_silent(
                unkeyed
                    .children
                    .iter()
                    .find(|r| r.attrs.contains_key(ROW_KEY_ATTR))
                    .unwrap()
                    .children[0]
                    .focus_id
                    .clone()
                    .unwrap(),
            );
            apply(&mut unkeyed, "replacement", &axis, false);
            assert_eq!(mounted(&unkeyed), ["i:5", "i:6"]);
        }
    });
}

#[test]
fn one_shot_focus_waits_for_viewport_and_yields_to_new_navigation_or_data() {
    use crate::core::reactivity::Signal;
    use crate::core::scroll_viewport::activate_scroll_viewport;
    use crate::core::tree::LayoutTree;
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        for parent in [false, true] {
            for horizontal in [false, true] {
                for measured in [false, true] {
                    let id = format!("pending-focus-{parent}-{horizontal}-{measured}");
                    let owner_id = if parent {
                        format!("{id}-owner")
                    } else {
                        id.clone()
                    };
                    let focus = Rc::new(Cell::new(Some(89)));
                    let revision = Signal::new(0);
                    let handle = VirtualListHandle::default();
                    let builder = virtual_list(&id)
                        .handle(handle.clone())
                        .item_count(100)
                        .item_height(20.0)
                        .height(100.0)
                        .overscan(0)
                        .data_key_fn(move || revision.peek().to_string())
                        .focused_index_fn({
                            let focus = focus.clone();
                            move || focus.take()
                        })
                        .render_item(move |_| main_extent(Element::new(), horizontal, 20.0));
                    let builder = if parent {
                        builder.use_parent_scroll()
                    } else {
                        builder
                    };
                    let builder = if horizontal {
                        builder.horizontal()
                    } else {
                        builder
                    };
                    let list: Element = if measured {
                        builder.measure_rows().into()
                    } else {
                        builder.into()
                    };
                    let mut root = if parent {
                        let stack: Element = if horizontal {
                            hstack().into()
                        } else {
                            vstack().into()
                        };
                        activate_scroll_viewport(
                            stack
                                .w(100.0)
                                .h(100.0)
                                .child(main_extent(
                                    Element::new().flex_shrink(0.0),
                                    horizontal,
                                    40.0,
                                ))
                                .child(list),
                            &owner_id,
                            if horizontal { "auto" } else { "visible" },
                            if horizontal { "visible" } else { "auto" },
                            "auto",
                            "auto",
                        )
                    } else {
                        list
                    };
                    let mut layout = LayoutTree::new();
                    layout.build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
                    let (offset_key, viewport_key, content_key): (
                        fn(&str) -> String,
                        fn(&str) -> String,
                        fn(&str) -> String,
                    ) = if horizontal {
                        (
                            scroll_offset_x_signal_key,
                            scroll_viewport_w_signal_key,
                            scroll_content_w_signal_key,
                        )
                    } else {
                        (
                            scroll_offset_y_signal_key,
                            scroll_viewport_h_signal_key,
                            scroll_content_h_signal_key,
                        )
                    };
                    let offset = use_signal(&offset_key(&owner_id), || 0.0_f32);
                    let viewport = use_signal(&viewport_key(&owner_id), || 0.0_f32);
                    use_signal(&content_key(&owner_id), || 0.0_f32).set_silent(2040.0);
                    offset.set_silent(0.0);
                    viewport.set_silent(0.0);
                    let sync = parent_list_body(&mut root).reactive_sync.clone().unwrap();
                    for _ in 0..3 {
                        sync.run(parent_list_body(&mut root), &mut layout);
                        assert_eq!(
                            offset.peek(),
                            0.0,
                            "hidden navigation must not write an offset: {id}"
                        );
                        assert!(
                            parent_list_body(&mut root).children.len() < 12,
                            "bounded hidden window: {id}"
                        );
                    }
                    viewport.set_silent(100.0);
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert!(
                        handle.viewport().unwrap().visible.contains(&89),
                        "one-shot destination survives: {id}"
                    );
                    assert!(offset.peek() > 1600.0);
                    // A consumed focus must not pull subsequent wheel/drag movement back.
                    offset.set_silent(0.0);
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert!(!handle.viewport().unwrap().visible.contains(&89));
                    viewport.set_silent(0.0);
                    focus.set(Some(70));
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert!(handle.scroll_to_index(20, ListScrollAlign::Start, 0.0));
                    sync.run(parent_list_body(&mut root), &mut layout);
                    viewport.set_silent(100.0);
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert!(
                        handle.viewport().unwrap().visible.contains(&20),
                        "new navigation replaces pending focus: {id}"
                    );
                    assert!(!handle.viewport().unwrap().visible.contains(&70));
                    viewport.set_silent(0.0);
                    focus.set(Some(80));
                    sync.run(parent_list_body(&mut root), &mut layout);
                    revision.set_silent(1);
                    sync.run(parent_list_body(&mut root), &mut layout);
                    viewport.set_silent(100.0);
                    sync.run(parent_list_body(&mut root), &mut layout);
                    assert!(
                        !handle.viewport().unwrap().visible.contains(&80),
                        "data revision retires stale focus: {id}"
                    );
                }
            }
        }
    });
}
