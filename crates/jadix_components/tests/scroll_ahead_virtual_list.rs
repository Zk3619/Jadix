use jadix_components::{
    components::virtual_list::{ListScrollAlign, VirtualListHandle, virtual_list},
    core::{
        reactivity::{create_root, use_signal},
        scroll_viewport::ScrollViewportSignalKeys,
        tree::{Element, LayoutTree, find_in_tree},
    },
};
use std::{cell::Cell, collections::BTreeSet, ops::Range, rc::Rc};

fn sync(el: &mut Element, layout: &mut LayoutTree) {
    if let Some(f) = el.reactive_sync.clone() {
        f.run(el, layout);
    }
    for child in &mut el.children {
        sync(child, layout);
    }
}

fn settle(root: &mut Element, layout: &mut LayoutTree) {
    for _ in 0..4 {
        sync(root, layout);
        layout.build_and_layout(root, 200.0, 150.0, [-1.0; 2]);
    }
}

fn mounted(root: &Element, count: usize, inverted: bool) -> BTreeSet<usize> {
    let mut indices = BTreeSet::new();
    fn collect(el: &Element, indices: &mut BTreeSet<usize>, count: usize, inverted: bool) {
        if let Some(index) = el.attrs.get("row-index") {
            let index = index.parse::<usize>().unwrap();
            indices.insert(if inverted { count - 1 - index } else { index });
        }
        for child in &el.children {
            collect(child, indices, count, inverted);
        }
    }
    collect(root, &mut indices, count, inverted);
    indices
}

fn assert_window(root: &Element, count: usize, inverted: bool, expected: Range<usize>) {
    assert_eq!(
        mounted(root, count, inverted),
        expected.collect(),
        "count={count}, inverted={inverted}"
    );
}

#[test]
fn scrolling_prefetches_only_ahead_preserves_visibility_and_rebases_on_changes() {
    let (_owner, ()) = create_root(|| {
        for horizontal in [false, true] {
            for measured in [false, true] {
                for inverted in [false, true] {
                    for enabled in [false, true] {
                        let id = format!("ahead-{horizontal}-{measured}-{inverted}-{enabled}");
                        let count = Rc::new(Cell::new(if measured { 1_000 } else { 1_000_000 }));
                        let revision = Rc::new(Cell::new(0));
                        let renders = Rc::new(Cell::new(0));
                        let handle = VirtualListHandle::default();
                        let mut builder = virtual_list(&id)
                            .handle(handle.clone())
                            .item_count_fn({
                                let count = count.clone();
                                move || count.get()
                            })
                            .data_key_fn({
                                let revision = revision.clone();
                                move || revision.get().to_string()
                            })
                            .item_extent(20.0)
                            .item_gap(5.0)
                            .height(100.0)
                            .overscan(1)
                            .scroll_ahead(enabled)
                            .follow_end(false)
                            .render_item({
                                let renders = renders.clone();
                                move |index| {
                                    renders.set(renders.get() + 1);
                                    let mut row = Element::new().w(20.0).h(20.0);
                                    row.attrs.insert("row-index".into(), index.to_string());
                                    row
                                }
                            });
                        if horizontal {
                            builder = builder.horizontal();
                        }
                        if measured {
                            builder = builder.measure_rows();
                        }
                        if inverted {
                            builder = builder.inverted();
                        }
                        let mut root: Element = builder.into();
                        let keys = ScrollViewportSignalKeys::new(&id);
                        let offset = use_signal(
                            if horizontal {
                                &keys.offset_x
                            } else {
                                &keys.offset_y
                            },
                            || 0.0_f32,
                        );
                        let viewport = use_signal(
                            if horizontal {
                                &keys.viewport_w
                            } else {
                                &keys.viewport_h
                            },
                            || 100.0_f32,
                        );
                        viewport.set_silent(100.0);
                        let mut layout = LayoutTree::new();
                        settle(&mut root, &mut layout);
                        assert!(
                            renders.get() <= 7,
                            "initial work must be independent of dataset size"
                        );

                        offset.set_silent(1000.0);
                        settle(&mut root, &mut layout);
                        assert_window(&root, count.get(), inverted, 39..45);
                        offset.set_silent(1050.0);
                        settle(&mut root, &mut layout);
                        assert_window(
                            &root,
                            count.get(),
                            inverted,
                            if enabled { 41..50 } else { 41..47 },
                        );
                        let actual = handle.viewport().unwrap().visible;
                        assert_eq!(
                            actual,
                            if inverted {
                                count.get() - 46..count.get() - 42
                            } else {
                                42..46
                            }
                        );
                        let idle_renders = renders.get();
                        settle(&mut root, &mut layout);
                        assert_eq!(
                            renders.get(),
                            idle_renders,
                            "idle syncs must not rebuild predictions"
                        );

                        offset.set_silent(1000.0);
                        settle(&mut root, &mut layout);
                        assert_window(
                            &root,
                            count.get(),
                            inverted,
                            if enabled { 36..45 } else { 39..45 },
                        );
                        revision.set(1);
                        settle(&mut root, &mut layout);
                        assert_window(&root, count.get(), inverted, 39..45);
                        offset.set_silent(1050.0);
                        settle(&mut root, &mut layout);
                        assert!(handle.scroll_to_index(
                            if inverted { count.get() - 41 } else { 40 },
                            ListScrollAlign::Start,
                            0.0
                        ));
                        settle(&mut root, &mut layout);
                        assert_eq!(offset.peek(), 1000.0);
                        assert_window(&root, count.get(), inverted, 39..45);

                        let before_jump = renders.get();
                        offset.set_silent(10_000.0);
                        settle(&mut root, &mut layout);
                        assert_window(&root, count.get(), inverted, 399..405);
                        assert!(
                            renders.get() - before_jump <= 6,
                            "teleports must not render the intervening rows"
                        );
                        offset.set_silent(10_050.0);
                        settle(&mut root, &mut layout);
                        viewport.set_silent(150.0);
                        settle(&mut root, &mut layout);
                        // Unkeyed fixed inverted lists retain distance from the
                        // visual end during resize; measured lists retain the row.
                        assert_window(
                            &root,
                            count.get(),
                            inverted,
                            if inverted && !measured {
                                399..407
                            } else {
                                401..409
                            },
                        );
                        count.set(0);
                        settle(&mut root, &mut layout);
                        assert!(mounted(&root, 0, inverted).is_empty());
                        assert_eq!(handle.viewport().unwrap().visible, 0..0);
                    }
                }
            }
        }
    });
}

#[test]
fn native_focus_reveal_never_mounts_a_corridor_from_the_old_prediction() {
    let (_owner, ()) = create_root(|| {
        let id = "ahead-native-focus";
        let focus = Rc::new(Cell::new(None));
        let mut root: Element = virtual_list(id)
            .item_count(1_000_000)
            .item_extent(20.0)
            .height(100.0)
            .overscan(1)
            .scroll_ahead(true)
            .focused_index_fn({
                let focus = focus.clone();
                move || focus.get()
            })
            .render_item(|_| Element::new().h(20.0))
            .into();
        let keys = ScrollViewportSignalKeys::new(id);
        let offset = use_signal(&keys.offset_y, || 0.0_f32);
        let mut layout = LayoutTree::new();
        settle(&mut root, &mut layout);
        offset.set_silent(50.0);
        settle(&mut root, &mut layout);
        focus.set(Some(900_000));
        sync(&mut root, &mut layout);
        let body = find_in_tree(&root, |el| el.attrs.contains_key("__jadix_vlist_range")).unwrap();
        assert!(
            body.children.len() <= 12,
            "far native focus must keep a bounded mount window"
        );
        assert!(body.attrs["__jadix_vlist_range"].contains("899999|"));
    });
}
