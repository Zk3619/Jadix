use jadix_core::presentation::{style::NodeStyle, *};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect as Cells, widgets::Widget},
    CellSurface,
};
fn node(root: RootId, content: NodeContent, frame: [f64; 4]) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect {
                x: frame[0],
                y: frame[1],
                width: frame[2],
                height: frame[3],
            },
            style: NodeStyle {
                foreground: [255; 4],
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn paint(s: &mut CellSurface) -> Buffer {
    let mut b = Buffer::empty(Cells::new(0, 0, 40, 20));
    s.render(Cells::new(3, 2, 30, 15), &mut b);
    b
}
fn mouse(s: &mut CellSurface, kind: MouseEventKind, x: u16, y: u16) -> Option<PlatformEvent> {
    s.handle(Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap()
}
#[test]
fn edge_tracks_page_drag_clamp_and_cancel_when_the_root_or_scope_changes() {
    let root = RootId::new();
    let mut s = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    s.set_scrollbars(true);
    let mut scroll = node(root, NodeContent::Scroll, [0., 0., 10., 8.]);
    let id = scroll.id;
    scroll.children = (0..40)
        .map(|i| {
            node(
                root,
                NodeContent::Text(format!("row{i}")),
                [0., i as f64, 20., 1.],
            )
        })
        .collect();
    s.present(0, vec![scroll.clone()]).unwrap();
    let b = paint(&mut s);
    assert_eq!(b[(12, 2)].symbol(), "█");
    assert_eq!(b[(12, 8)].symbol(), "│");
    assert_eq!(b[(3, 9)].symbol(), "█");
    let e = mouse(&mut s, MouseEventKind::Down(MouseButton::Left), 12, 8).unwrap();
    assert!(matches!(e.payload, EventPayload::Scroll { y: 8., .. }));
    paint(&mut s);
    // Thumb is one cell at relative y=2 after paging; preserve its grab point.
    mouse(&mut s, MouseEventKind::Down(MouseButton::Left), 12, 4);
    let e = mouse(&mut s, MouseEventKind::Drag(MouseButton::Left), 30, 19).unwrap();
    assert!(matches!(e.payload, EventPayload::Scroll { y: 32., .. }));
    assert!(mouse(&mut s, MouseEventKind::Up(MouseButton::Left), 30, 19).is_none());
    let b = paint(&mut s);
    assert_eq!(b[(12, 8)].symbol(), "█");
    // Horizontal drag has independent offset and a shared reserved corner.
    mouse(&mut s, MouseEventKind::Down(MouseButton::Left), 3, 9);
    let e = mouse(&mut s, MouseEventKind::Drag(MouseButton::Left), 30, 9).unwrap();
    assert!(matches!(e.payload, EventPayload::Scroll { x: 10., y: 32. }));
    s.handle(Event::FocusLost).unwrap();
    s.handle(Event::FocusGained).unwrap();
    assert!(mouse(&mut s, MouseEventKind::Drag(MouseButton::Left), 3, 9).is_none());
    // A modal opens while a thumb is held: no background drag survives.
    mouse(&mut s, MouseEventKind::Down(MouseButton::Left), 12, 8);
    let mut modal = node(
        root,
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: false,
            revision: 0,
        },
        [12., 0., 10., 8.],
    );
    modal.children.push(node(
        root,
        NodeContent::Button {
            label: "close".into(),
            enabled: true,
        },
        [12., 0., 6., 1.],
    ));
    s.present(s.revision(), vec![scroll, modal]).unwrap();
    paint(&mut s);
    assert!(mouse(&mut s, MouseEventKind::Drag(MouseButton::Left), 12, 2).is_none());
    assert!(s.nodes().any(|(n, _)| n == id));
    s.present(s.revision(), vec![]).unwrap();
    let b = paint(&mut s);
    assert_eq!(b[(12, 8)].symbol(), " ");
}
#[test]
fn nested_scroll_clips_and_portals_do_not_inflate_ancestor_ranges() {
    let root = RootId::new();
    let mut s = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    s.set_scrollbars(true);
    let mut outer = node(root, NodeContent::Scroll, [0., 0., 20., 10.]);
    let mut inner = node(root, NodeContent::Scroll, [0., 0., 10., 6.]);
    inner.children.push(node(
        root,
        NodeContent::Text("long".into()),
        [0., 99., 5., 1.],
    ));
    outer.children.push(inner);
    let mut closed = node(
        root,
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        },
        [0., 0., 10., 10.],
    );
    closed.children.push(node(
        root,
        NodeContent::Text("hidden".into()),
        [0., 999., 5., 1.],
    ));
    outer.children.push(closed);
    s.present(0, vec![outer.clone()]).unwrap();
    let b = paint(&mut s);
    assert_eq!(b[(22, 2)].symbol(), " ");
    assert_eq!(b[(12, 2)].symbol(), "█");
    let e = mouse(&mut s, MouseEventKind::ScrollDown, 4, 3).unwrap();
    assert!(matches!(e.payload, EventPayload::Scroll { y: 3., .. }));
    // Shrink inner content. Its offset clamps and both tracks disappear.
    outer.children[0].children[0].descriptor.frame.y = 0.;
    s.present(s.revision(), vec![outer]).unwrap();
    let b = paint(&mut s);
    assert_eq!(b[(12, 2)].symbol(), " ");
    assert!(mouse(&mut s, MouseEventKind::ScrollDown, 4, 3).is_none());
}
