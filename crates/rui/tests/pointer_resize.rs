use rui::core::{
    reactivity::Signal,
    tree::{Element, LayoutStatus},
};
use rui::style::IntoCursor;
use rui::utils::pointer_action::*;
use std::{cell::Cell, rc::Rc};

#[test]
fn resize_does_not_scroll_other_gallery_regions() {
    use rui::core::reactivity::{create_root, use_signal};
    use rui::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_offset_y_signal_key, scroll_viewport_h_signal_key,
        SCROLL_VIEWPORT_FOCUS_PREFIX,
    };
    let (_owner, ()) = create_root(|| {
        on_up();
        let mut root = Element::new();
        let mut offsets = Vec::new();
        for (rest, top) in [("resize-owner", 100.0), ("unrelated", 600.0)] {
            offsets.push(use_signal(&scroll_offset_y_signal_key(rest), || 100.0_f32));
            use_signal(&scroll_viewport_h_signal_key(rest), || 200.0_f32);
            use_signal(&scroll_content_h_signal_key(rest), || 600.0_f32);
            let mut viewport =
                Element::new().focus_id(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{rest}"));
            viewport.layout_bounds = [0.0, top, 300.0, 200.0];
            root.children.push(viewport);
        }
        begin_resize(
            BoxRect::from_xywh(40.0, 28.0, 160.0, 100.0),
            [200.0, 220.0],
            ResizeOpts::default()
                .edges(Edges::bottom())
                .autoscroll(true),
            None,
        );
        tick_resize(200.0, 290.0);
        assert!(pump_edge_scroll(&root));
        assert!(offsets[0].peek() > 100.0);
        assert_eq!(
            offsets[1].peek(),
            100.0,
            "a resize must not pan a different region"
        );
        let rect = tick_resize(200.0, 290.0).unwrap();
        let rect_scroll = offsets[0].peek();
        for _ in 0..10 {
            pump_edge_scroll(&root);
        }
        let scrolled = tick_resize(200.0, 290.0).unwrap();
        assert!(
            (scrolled.bottom() - rect.bottom() - (offsets[0].peek() - rect_scroll)).abs() < 0.01,
            "scroll displacement must keep the resized edge under a stationary pointer: {rect:?} -> {scrolled:?}"
        );
        on_up();
    });
}

#[test]
fn resize_scroll_target_stays_with_the_press_region() {
    use rui::core::reactivity::{create_root, use_signal};
    use rui::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_offset_y_signal_key, scroll_viewport_h_signal_key,
        SCROLL_VIEWPORT_FOCUS_PREFIX,
    };
    let (_owner, ()) = create_root(|| {
        on_up();
        let mut root = Element::new();
        let mut offsets = Vec::new();
        for (rest, top) in [("press-owner", 100.0), ("sibling", 600.0)] {
            offsets.push(use_signal(&scroll_offset_y_signal_key(rest), || 100.0_f32));
            use_signal(&scroll_viewport_h_signal_key(rest), || 200.0_f32);
            use_signal(&scroll_content_h_signal_key(rest), || 600.0_f32);
            let mut viewport =
                Element::new().focus_id(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{rest}"));
            viewport.layout_bounds = [0.0, top, 300.0, 200.0];
            root.children.push(viewport);
        }
        begin_resize(
            BoxRect::from_xywh(40.0, 28.0, 160.0, 100.0),
            [200.0, 220.0],
            ResizeOpts::default()
                .edges(Edges::bottom())
                .autoscroll(true),
            None,
        );
        tick_resize(200.0, 790.0);
        for _ in 0..20 {
            pump_edge_scroll(&root);
        }
        assert_eq!(
            offsets[1].peek(),
            100.0,
            "resize must not adopt a sibling scroll region"
        );
        tick_resize(200.0, 290.0);
        assert!(
            pump_edge_scroll(&root),
            "returning to the owner edge still scrolls"
        );
        assert!(offsets[0].peek() > 100.0);
        on_up();
    });
}

#[test]
fn resize_autoscroll_uses_scrolled_viewport_edges() {
    use rui::core::reactivity::{create_root, use_signal};
    use rui::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_offset_y_signal_key, scroll_viewport_h_signal_key,
        SCROLL_VIEWPORT_FOCUS_PREFIX,
    };

    let (_owner, ()) = create_root(|| {
        on_up();
        let rest = "resize-scrolled-viewport";
        let offset = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32);
        use_signal(&scroll_viewport_h_signal_key(rest), || 200.0_f32);
        use_signal(&scroll_content_h_signal_key(rest), || 600.0_f32);
        let mut viewport = Element::new().focus_id(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{rest}"));
        viewport.layout_bounds = [0.0, 400.0, 300.0, 200.0];
        let root = Element::new()
            .overflow("scroll")
            .scroll_offset(0.0, 300.0)
            .child(viewport);
        begin_resize(
            BoxRect::from_xywh(40.0, 120.0, 160.0, 100.0),
            [200.0, 220.0],
            ResizeOpts::default()
                .edges(Edges::bottom_right())
                .autoscroll(true),
            None,
        );
        tick_resize(200.0, 290.0);
        assert!(
            pump_edge_scroll(&root),
            "pointer near the visible bottom must scroll down"
        );
        assert!(
            offset.peek() > 0.0,
            "the viewport moves in the correct direction"
        );
        let first = offset.peek();
        assert!(
            pump_edge_scroll(&root),
            "holding the pointer still keeps scrolling"
        );
        assert!(offset.peek() > first);
        on_up();
        assert!(!pump_edge_scroll(&root), "release stops scrolling");
    });
}

#[test]
fn handleless_resize_hits_scrolled_edges_updates_geometry_and_keeps_cursor() {
    use rui::app::pointer_events::{dispatch_pointer, hit_path, MouseEvent, PointerKind};
    use rui::core::reactivity::create_root;
    use rui::core::tree::{CursorKind, LayoutTree};
    use winit::event::MouseButton;

    let (_owner, ()) = create_root(|| {
        for (offset, edges, expected) in [
            ([0.0, 0.0], Edges::top_left(), [50.0, 43.0, 150.0, 85.0]),
            ([80.0, 0.0], Edges::top(), [40.0, 43.0, 160.0, 85.0]),
            ([159.0, 0.0], Edges::top_right(), [40.0, 43.0, 170.0, 85.0]),
            ([0.0, 50.0], Edges::left(), [50.0, 28.0, 150.0, 100.0]),
            ([159.0, 50.0], Edges::right(), [40.0, 28.0, 170.0, 100.0]),
            (
                [0.0, 99.0],
                Edges::bottom_left(),
                [50.0, 28.0, 150.0, 115.0],
            ),
            ([80.0, 99.0], Edges::bottom(), [40.0, 28.0, 160.0, 115.0]),
            (
                [159.0, 99.0],
                Edges::bottom_right(),
                [40.0, 28.0, 170.0, 115.0],
            ),
        ] {
            on_up();
            let rect = Signal::new([40.0, 28.0, 160.0, 100.0]);
            let interior_presses = Rc::new(Cell::new(0));
            let presses = interior_presses.clone();
            let mut panel = resize_on_edges(
                Element::new()
                    .absolute()
                    .cursor_kind(CursorKind::Pointer)
                    .on_mousedown(move |_| presses.set(presses.get() + 1)),
                rect,
                ResizeOpts::default().min_size(64.0, 48.0),
                8.0,
            );
            panel.layout_bounds = [140.0, 328.0, 160.0, 100.0];
            let mut root = Element::new()
                .overflow("scroll")
                .scroll_offset(0.0, 80.0)
                .child(panel);
            root.layout_bounds = [100.0, 200.0, 800.0, 500.0];
            let pointer = [140.0 + offset[0], 248.0 + offset[1]];
            let path = hit_path(&root, pointer[0], pointer[1]);
            assert_eq!(path.last().unwrap().bounds, [140.0, 248.0, 160.0, 100.0]);
            let mut event = MouseEvent::new(pointer[0], pointer[1], MouseButton::Left, 0);
            dispatch_pointer(&path, &mut event, PointerKind::Move);
            let sync = root.children[0].reactive_sync.clone().unwrap();
            sync.run(&mut root.children[0], &mut LayoutTree::new());
            assert_eq!(root.children[0].cursor, edges.cursor().into_cursor());
            dispatch_pointer(&path, &mut event, PointerKind::Down);
            assert!(resize_is_active());
            assert!(event.is_consumed());
            assert_eq!(interior_presses.get(), 0);
            assert_eq!(active_resize_cursor(), edges.cursor().into_cursor());
            assert!(on_move(pointer[0] + 10.0, pointer[1] + 15.0));
            assert_eq!(rect.peek(), expected);
            assert_eq!(
                sync.run(&mut root.children[0], &mut LayoutTree::new()),
                LayoutStatus::LayoutChanged
            );
            let want = Element::new()
                .left(expected[0])
                .top(expected[1])
                .w(expected[2])
                .h(expected[3]);
            assert_eq!(root.children[0].style.size, want.style.size);
            assert_eq!(root.children[0].style.inset, want.style.inset);
            assert!(
                on_move(-100.0, -100.0),
                "resize continues outside the hit box"
            );
            assert_eq!(active_resize_cursor(), edges.cursor().into_cursor());
            let final_rect = rect.peek();
            assert!(final_rect[2] >= 64.0 && final_rect[3] >= 48.0);
            assert!(on_up());
            assert_eq!(active_resize_cursor(), None);
            assert!(!on_move(0.0, 0.0));
            assert_eq!(rect.peek(), final_rect, "release keeps the resized box");
        }
        let rect = Signal::new([0.0, 0.0, 160.0, 100.0]);
        let presses = Rc::new(Cell::new(0));
        let counted = presses.clone();
        let mut panel = resize_on_edges(
            Element::new()
                .absolute()
                .cursor_kind(CursorKind::Pointer)
                .on_mousedown(move |_| counted.set(counted.get() + 1)),
            rect,
            ResizeOpts::default(),
            8.0,
        );
        panel.layout_bounds = [0.0, 0.0, 160.0, 100.0];
        let path = hit_path(&panel, 80.0, 50.0);
        let mut event = MouseEvent::new(80.0, 50.0, MouseButton::Left, 0);
        dispatch_pointer(&path, &mut event, PointerKind::Down);
        assert!(!resize_is_active());
        assert_eq!(presses.get(), 1, "interior keeps its original handler");
        assert_eq!(panel.cursor, Some(CursorKind::Pointer.into()));
        event = MouseEvent::new(0.0, 0.0, MouseButton::Right, 0);
        dispatch_pointer(&hit_path(&panel, 0.0, 0.0), &mut event, PointerKind::Down);
        assert!(!resize_is_active(), "right button does not resize");
    });
}
