use super::*;

#[test]
fn resizable_builder_builds() {
    let _el: Element = resizable("rz1")
        .default_sizes([30.0, 70.0])
        .left(|| {
            let t: Element = crate::primitives::text::text("L").into();
            r#box().child(t).into()
        })
        .right(|| {
            let t: Element = crate::primitives::text::text("R").into();
            r#box().child(t).into()
        })
        .into();
}

#[test]
fn builder_panel_style() {
    use crate::core::reactivity::create_root;
    let (_o, el): (_, Element) = create_root(|| {
        resizable("rz-chrome")
            .child(|| {
                resizable_panel()
                    .default_size(40.0)
                    .style(|b| b.text_size(20.0))
                    .child(crate::primitives::text::text("L"))
                    .into()
            })
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "resizable_panel .style(|b| b.text_size(20)) must land on the panel tree"
    );
}

#[test]
fn resizable_panel_group_scope_builds() {
    let el = resizable_panel_group_scope("rz2", "horizontal", Some(120.0), None, || {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            ResizablePanel(ResizablePanelProps {
                default_size: Some(40.0),
                min_size: Some(20.0),
                max_size: None,
                children: Some(crate::primitives::text::text("A").into()),
                base: None,
            }),
            ResizableHandle(ResizableHandleProps {
                with_handle: Some(true),
                children: None,
                base: None,
            }),
            ResizablePanel(ResizablePanelProps {
                default_size: Some(60.0),
                min_size: None,
                max_size: None,
                children: Some(crate::primitives::text::text("B").into()),
                base: None,
            }),
        ])
    });
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_GROUP)
    );
}

#[test]
fn orientation_parse() {
    assert_eq!(
        ResizableOrientation::parse("vertical"),
        ResizableOrientation::Vertical
    );
    assert_eq!(
        ResizableOrientation::parse("horizontal"),
        ResizableOrientation::Horizontal
    );
}

#[test]
fn horizontal_handle_is_narrow_and_drag_updates_sizes() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, (mut root, sizes)) = create_root(|| {
        let sizes = use_signal("rz_layout_sizes", || vec![30.0f32, 70.0]);
        let el = resizable_panel_group_scope(
            "rz_layout",
            "horizontal",
            Some(160.0),
            Some(sizes),
            || {
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![
                    ResizablePanel(ResizablePanelProps {
                        default_size: Some(30.0),
                        min_size: Some(10.0),
                        max_size: None,
                        children: Some(r#box().w("100%").h("100%").bg(tokens::muted()).into()),
                        base: None,
                    }),
                    ResizableHandle(ResizableHandleProps {
                        with_handle: Some(true),
                        children: None,
                        base: None,
                    }),
                    ResizablePanel(ResizablePanelProps {
                        default_size: Some(70.0),
                        min_size: Some(10.0),
                        max_size: None,
                        children: Some(r#box().w("100%").h("100%").bg(tokens::background()).into()),
                        base: None,
                    }),
                ])
            },
        );
        (el, sizes)
    });

    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 400.0, 160.0, [0.0, 0.0]);

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
            return Some(el);
        }
        for c in &el.children {
            if let Some(f) = find_slot(c, slot) {
                return Some(f);
            }
        }
        None
    }
    fn find_handle<'a>(el: &'a Element) -> Option<&'a Element> {
        find_slot(el, SLOT_HANDLE)
    }
    fn panel_bounds(el: &Element, idx: usize) -> Option<[f32; 4]> {
        fn walk(el: &Element, idx: usize) -> Option<[f32; 4]> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_PANEL)
                && el.attrs.get(ATTR_PANEL_INDEX).map(|s| s.as_str()) == Some(&idx.to_string())
            {
                return Some(el.layout_bounds);
            }
            for c in &el.children {
                if let Some(b) = walk(c, idx) {
                    return Some(b);
                }
            }
            None
        }
        walk(el, idx)
    }

    let handle = find_handle(&root).expect("handle");
    let hw = handle.layout_bounds[2];
    assert!(
        hw <= 2.0,
        "handle layout width should be 1px hairline, got {hw} bounds={:?}",
        handle.layout_bounds
    );

    // 握柄布局中心须与发丝线中心重合（flex 居中；发丝线铺满 1px 槽）。
    let hairline = handle
        .children
        .first()
        .expect("hairline is first handle child");
    let grip = find_slot(&root, SLOT_HANDLE_ICON).expect("grip icon");
    let cx = |b: [f32; 4]| b[0] + b[2] * 0.5;
    let cy = |b: [f32; 4]| b[1] + b[3] * 0.5;
    let hcx = cx(hairline.layout_bounds);
    let gcx = cx(grip.layout_bounds);
    let hcy = cy(hairline.layout_bounds);
    let gcy = cy(grip.layout_bounds);
    assert!(
            (hcx - gcx).abs() < 0.75,
            "grip center x ({gcx}) must align with hairline center x ({hcx}); handle={:?} hairline={:?} grip={:?}",
            handle.layout_bounds,
            hairline.layout_bounds,
            grip.layout_bounds
        );
    assert!(
            (hcy - gcy).abs() < 0.75,
            "grip center y ({gcy}) must align with hairline center y ({hcy}); handle={:?} hairline={:?} grip={:?}",
            handle.layout_bounds,
            hairline.layout_bounds,
            grip.layout_bounds
        );

    let left = panel_bounds(&root, 0).expect("left");
    let right = panel_bounds(&root, 1).expect("right");
    assert!(
        left[2] > 80.0 && left[2] < 160.0,
        "left panel ~30% of 400, got {:?}",
        left
    );
    assert!(right[2] > 200.0, "right panel ~70% of 400, got {:?}", right);

    // Simulate drag: move pointer +40px on a 400px group → ~10%
    begin_resizable_drag(
        "rz_layout__rz_h0",
        ResizableDrag {
            sizes,
            limits: Rc::new(vec![(10.0, 100.0), (10.0, 100.0)]),
            orientation: ResizableOrientation::Horizontal,
            group_bounds: Rc::new(Cell::new([0.0, 0.0, 400.0, 160.0])),
            handle_index: 0,
            start_sizes: vec![30.0, 70.0],
            start_x: 120.0,
            start_y: 80.0,
        },
        120.0,
        80.0,
    );
    assert!(crate::utils::drag::routing::blocks_content_pan());
    assert!(!crate::utils::drag::routing::freezes_scroll());
    assert!(crate::utils::drag::routing::prepare_move(Some(&root), 160.0, 80.0).dispatch());
    let after = sizes.peek();
    crate::utils::drag::routing::finish_all();
    assert!(!is_resizable_dragging());
    assert!(
        (after[0] - 40.0).abs() < 0.5 && (after[1] - 60.0).abs() < 0.5,
        "expected ~[40,60] after +40px drag, got {after:?}"
    );
}

#[test]
fn vertical_grip_centers_on_hairline() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, mut root) = create_root(|| {
        resizable_panel_group_scope("rz_v_align", "vertical", Some(224.0), None, || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![
                ResizablePanel(ResizablePanelProps {
                    default_size: Some(30.0),
                    min_size: Some(10.0),
                    max_size: None,
                    children: Some(r#box().w("100%").h("100%").bg(tokens::muted()).into()),
                    base: None,
                }),
                ResizableHandle(ResizableHandleProps {
                    with_handle: Some(true),
                    children: None,
                    base: None,
                }),
                ResizablePanel(ResizablePanelProps {
                    default_size: Some(70.0),
                    min_size: Some(10.0),
                    max_size: None,
                    children: Some(r#box().w("100%").h("100%").bg(tokens::background()).into()),
                    base: None,
                }),
            ])
        })
    });

    let mut layout = LayoutTree::new();
    let result = layout.build_and_layout(&mut root, 400.0, 224.0, [0.0, 0.0]);

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
            return Some(el);
        }
        for c in &el.children {
            if let Some(f) = find_slot(c, slot) {
                return Some(f);
            }
        }
        None
    }
    fn find_panel<'a>(el: &'a Element, idx: usize) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_PANEL)
            && el.attrs.get(ATTR_PANEL_INDEX).map(|s| s.as_str()) == Some(&idx.to_string())
        {
            return Some(el);
        }
        for c in &el.children {
            if let Some(f) = find_panel(c, idx) {
                return Some(f);
            }
        }
        None
    }

    let handle = find_slot(&root, SLOT_HANDLE).expect("handle");
    assert!(
        handle.style.z_index >= 1,
        "handle must stack above sibling panels so overflowing grip is not covered"
    );
    let grip = find_slot(&root, SLOT_HANDLE_ICON).expect("grip");
    let hairline = handle.children.first().expect("hairline");
    assert!(
        handle.layout_bounds[3] <= 2.0,
        "vertical handle height should be ~1px, got {:?}",
        handle.layout_bounds
    );
    let cy = |b: [f32; 4]| b[1] + b[3] * 0.5;
    let dy = (cy(grip.layout_bounds) - cy(hairline.layout_bounds)).abs();
    assert!(
            dy < 0.75,
            "vertical grip must sit on hairline; dy={dy} handle={:?} hair={:?} grip={:?} align={:?} justify={:?}",
            handle.layout_bounds,
            hairline.layout_bounds,
            grip.layout_bounds,
            handle.style.align_items,
            handle.style.justify_content
        );

    // 绘制序：握柄实例须晚于下一栏面板底色，否则溢出半截会被盖住。
    let grip_b = grip.layout_bounds;
    let p1 = find_panel(&root, 1).expect("panel 1").layout_bounds;
    let grip_inst = result
        .instances
        .iter()
        .position(|i| {
            (i.rect_pos[0] - grip_b[0]).abs() < 2.0
                && (i.rect_pos[1] - grip_b[1]).abs() < 2.0
                && (i.rect_size[0] - grip_b[2]).abs() < 2.0
                && (i.rect_size[1] - grip_b[3]).abs() < 2.0
        })
        .expect("grip paint instance");
    let panel_inst = result
        .instances
        .iter()
        .position(|i| {
            (i.rect_pos[0] - p1[0]).abs() < 2.0
                && (i.rect_pos[1] - p1[1]).abs() < 2.0
                && i.rect_size[0] > p1[2] * 0.5
                && i.color[3] > 0.5
        })
        .expect("panel1 paint instance");
    assert!(
        grip_inst > panel_inst,
        "grip instance #{grip_inst} must paint after panel1 instance #{panel_inst}"
    );
}
