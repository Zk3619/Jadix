use jadix_core::presentation::{
    style::{Axis, NodeStyle},
    *,
};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect as Cells, widgets::Widget},
    CellSurface,
};

fn node(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            style: NodeStyle {
                foreground: [255; 4],
                shrink: 0.,
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn paint(view: &mut CellSurface, area: Cells) -> Buffer {
    let mut buffer = Buffer::empty(Cells::new(0, 0, 120, 80));
    view.render(area, &mut buffer);
    buffer
}

#[test]
fn automatic_popover_tracks_scrolling_anchor_without_a_new_presentation() {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [8., 16.]).unwrap();
    let mut scroll = node(root, NodeContent::Scroll);
    scroll.descriptor.style.width = Some(320.);
    scroll.descriptor.style.height = Some(160.);
    let mut spacer = node(root, NodeContent::Text("spacer".into()));
    spacer.descriptor.style.height = Some(96.);
    let anchor = node(
        root,
        NodeContent::Button {
            label: "Anchor".into(),
            enabled: true,
        },
    );
    let aid = anchor.id;
    let mut filler = node(root, NodeContent::Text("filler".into()));
    filler.descriptor.style.height = Some(600.);
    let mut popup = node(
        root,
        NodeContent::Popover {
            anchor: aid,
            placement: PopoverPlacement::BottomStart,
            gap: 0,
            open: true,
            close_on_outside_click: true,
            revision: 0,
        },
    );
    popup.descriptor.style.width = Some(160.);
    popup.descriptor.style.height = Some(48.);
    popup.children.push(node(
        root,
        NodeContent::Button {
            label: "Popup".into(),
            enabled: true,
        },
    ));
    let sid = scroll.id;
    scroll.children = vec![spacer, anchor, filler, popup];
    let area = Cells::new(3, 2, 70, 25);
    view.present_auto(0, vec![scroll], area, 0).unwrap();
    let locate = |buffer: &Buffer| {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .find(|(x, y)| buffer[(*x, *y)].symbol() == "P")
            .unwrap()
    };
    let before = locate(&paint(&mut view, area));
    let revision = view.revision();
    let event = view
        .handle(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: before.0,
            row: before.1,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap()
        .unwrap();
    assert_eq!(event.node, sid);
    assert!(matches!(event.payload,EventPayload::Scroll{y,..} if y>0.));
    let after = locate(&paint(&mut view, area));
    assert!(after.1 < before.1, "{before:?} -> {after:?}");
    assert_eq!(view.revision(), revision);
}
fn key(c: KeyCode) -> Event {
    Event::Key(KeyEvent::new(c, KeyModifiers::NONE))
}
fn click(x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn shared_layout_measures_graphemes_reflows_retains_history_and_rejects_bad_candidates() {
    let root = RootId::new();
    let life = RootLifetime::new(root);
    let mut view = CellSurface::new(life, [8., 16.]).unwrap();
    let mut panel = node(root, NodeContent::Container);
    panel.descriptor.style.padding = [16., 8., 16., 8.];
    panel.descriptor.style.gap = 16.;
    let input = node(
        root,
        NodeContent::Textarea {
            max_length: None,
            value: "中文x".into(),
            placeholder: String::new(),
            enabled: true,
            revision: 0,
        },
    );
    let input_id = input.id;
    panel.children.push(input);
    panel.children.push(node(
        root,
        NodeContent::Button {
            label: "Go".into(),
            enabled: true,
        },
    ));
    let area = Cells::new(3, 2, 20, 12);
    let stats = view.present_auto(0, vec![panel.clone()], area, 0).unwrap();
    assert!(stats.measurements > 0);
    let frame = view.frame(input_id).unwrap();
    assert_eq!((frame.x, frame.y, frame.height), (8., 16., 16.));
    let buffer = paint(&mut view, area);
    assert_eq!(buffer[(4, 3)].symbol(), "中");
    view.handle(key(KeyCode::End)).unwrap();
    view.handle(key(KeyCode::Char('!'))).unwrap();
    let stats = view
        .present_auto(view.revision(), vec![panel.clone()], area, 0)
        .unwrap();
    assert_eq!(stats.measurements, 0);
    assert_eq!(view.field(input_id).unwrap().document().text(), "中文x!");
    assert!(view.field(input_id).unwrap().can_undo());
    let narrow = Cells::new(3, 2, 5, 12);
    view.present_auto(view.revision(), vec![panel.clone()], narrow, 0)
        .unwrap();
    assert!(view.frame(input_id).unwrap().height >= 32.);
    let before = view.frame(input_id);
    let revision = view.revision();
    panel.descriptor.style.padding[0] = f32::NAN;
    assert!(view.present_auto(revision, vec![panel], area, 0).is_err());
    assert_eq!(view.frame(input_id), before);
    assert_eq!(view.revision(), revision);
    view.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(view.field(input_id).unwrap().document().text(), "中文x");
}

#[test]
fn automatic_scroll_and_unicode_search_reveal_offscreen_rows_without_changing_focus_or_selection() {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [8., 16.]).unwrap();
    let mut scroll = node(root, NodeContent::Scroll);
    scroll.descriptor.style.height = Some(80.);
    for i in 0..100 {
        scroll
            .children
            .push(node(root, NodeContent::Text(format!("row {i:03} 中文"))));
    }
    let target = scroll.children[90].id;
    let field = node(
        root,
        NodeContent::Input {
            max_length: None,
            value: "private".into(),
            placeholder: String::new(),
            password: true,
            enabled: true,
            revision: 0,
        },
    );
    let area = Cells::new(2, 1, 30, 10);
    view.present_auto(0, vec![scroll, field], area, 0).unwrap();
    paint(&mut view, area);
    let focus = view.focused();
    let selection = view.field(focus.unwrap()).unwrap().selection();
    assert_eq!(view.find("private").unwrap().matches, 0);
    let result = view.find("row 090").unwrap();
    assert_eq!(result.matches, 1);
    assert_eq!(view.search_matches()[0].node, target);
    let b = paint(&mut view, area);
    assert!(b.content.iter().any(|cell| cell.symbol() == "9"
        && matches!(cell.bg, jadix_tui::ratatui::style::Color::Rgb(251, 146, 60))));
    assert_eq!(view.focused(), focus);
    assert_eq!(view.field(focus.unwrap()).unwrap().selection(), selection);
    assert_eq!(view.find_next(true).unwrap().current, Some(0));
    assert_eq!(view.find("中文").unwrap().matches, 100);
    assert_eq!(view.find_next(true).unwrap().current, Some(99));
    view.clear_search();
    assert_eq!(view.search_summary().matches, 0);
}

#[test]
fn search_is_bounded_grapheme_safe_scoped_and_refreshed_after_controlled_updates() {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [8., 16.]).unwrap();
    let text = node(root, NodeContent::Text("e\u{301} 中文 ".repeat(5000)));
    let id = text.id;
    let area = Cells::new(0, 0, 30, 10);
    view.present_auto(0, vec![text.clone()], area, 0).unwrap();
    assert_eq!(view.find("e").unwrap().matches, 0);
    assert_eq!(view.find("\u{301}").unwrap().matches, 0);
    let result = view.find("e\u{301}").unwrap();
    assert_eq!(result.matches, 4096);
    assert!(result.truncated);
    assert!(view.find(&"x".repeat(513)).is_err());
    assert_eq!(view.search_summary(), result);
    let mut next = text;
    next.descriptor.content = NodeContent::Text("中文".into());
    view.present_auto(view.revision(), vec![next], area, 0)
        .unwrap();
    paint(&mut view, area);
    assert_eq!(view.search_summary().matches, 0);
    assert_eq!(view.frame(id).unwrap().height, 16.);
    let mut modal = node(
        root,
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 1,
        },
    );
    modal.descriptor.style.width = Some(96.);
    modal
        .children
        .push(node(root, NodeContent::Text("only dialog".into())));
    let outside = node(root, NodeContent::Text("outside".into()));
    view.present_auto(view.revision(), vec![outside, modal], area, 0)
        .unwrap();
    assert_eq!(view.find("outside").unwrap().matches, 0);
    assert_eq!(view.find("dialog").unwrap().matches, 1);
}

#[test]
fn portals_follow_anchor_dependencies_escape_scroll_clips_and_dismiss_without_background_activation(
) {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [8., 16.]).unwrap();
    let anchor = node(
        root,
        NodeContent::Button {
            label: "Anchor".into(),
            enabled: true,
        },
    );
    let anchor_id = anchor.id;
    let mut popover = node(
        root,
        NodeContent::Popover {
            anchor: anchor_id,
            placement: PopoverPlacement::BottomStart,
            gap: 4,
            open: true,
            close_on_outside_click: true,
            revision: 7,
        },
    );
    let popover_id = popover.id;
    popover.descriptor.style.width = Some(128.);
    popover.descriptor.style.padding = [16.; 4];
    let action = node(
        root,
        NodeContent::Button {
            label: "Action".into(),
            enabled: true,
        },
    );
    let action_id = action.id;
    popover.children.push(action);
    let mut scroll = node(root, NodeContent::Scroll);
    scroll.descriptor.style.height = Some(16.);
    // Portal occurs before its external anchor in tree order.
    scroll.children = vec![popover];
    let area = Cells::new(4, 3, 40, 15);
    view.present_auto(0, vec![scroll, anchor], area, 0).unwrap();
    paint(&mut view, area);
    let frame = view.frame(action_id).unwrap();
    let x = area.x + (frame.x / 8.) as u16;
    let y = area.y + (frame.y / 16.) as u16;
    assert_eq!(view.handle(click(x, y)).unwrap().unwrap().node, action_id);
    let dismissed = view.handle(click(area.x, area.y)).unwrap().unwrap();
    assert_eq!(dismissed.node, popover_id);
    assert_eq!(
        dismissed.payload,
        EventPayload::Dismiss {
            backdrop: true,
            revision: 8
        }
    );
    assert_eq!(view.focused(), Some(action_id));
}

#[test]
fn shared_flex_row_layout_is_relative_to_embedding_origin() {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [8., 16.]).unwrap();
    let mut row = node(root, NodeContent::Container);
    row.descriptor.style.axis = Axis::Row;
    row.descriptor.style.gap = 8.;
    let a = node(root, NodeContent::Text("aa".into()));
    let b = node(root, NodeContent::Text("中文".into()));
    let id = b.id;
    row.children = vec![a, b];
    let area = Cells::new(5, 4, 30, 10);
    view.present_auto(0, vec![row], area, 0).unwrap();
    assert_eq!(view.frame(id).unwrap().x, 24.);
    assert_eq!(paint(&mut view, area)[(8, 4)].symbol(), "中");
}

#[test]
fn textarea_vertical_motion_uses_wrapped_graphemes_and_keeps_preferred_column() {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    let mut input = node(
        root,
        NodeContent::Textarea {
            max_length: None,
            value: "abcdef\nx\n12345678".into(),
            placeholder: String::new(),
            enabled: true,
            revision: 0,
        },
    );
    let id = input.id;
    input.descriptor.frame = Rect {
        x: 0.,
        y: 0.,
        width: 4.,
        height: 2.,
    };
    view.present(0, vec![input.clone()]).unwrap();
    paint(&mut view, Cells::new(0, 0, 4, 2));
    view.handle(key(KeyCode::Right)).unwrap();
    view.handle(key(KeyCode::Right)).unwrap();
    assert_eq!(view.field(id).unwrap().caret(), 2);
    view.handle(key(KeyCode::Down)).unwrap();
    assert_eq!(view.field(id).unwrap().caret(), 6);
    view.handle(key(KeyCode::Down)).unwrap();
    assert_eq!(view.field(id).unwrap().caret(), 8);
    view.handle(key(KeyCode::Down)).unwrap();
    assert_eq!(view.field(id).unwrap().caret(), 11);
    view.handle(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)))
        .unwrap();
    assert_eq!(view.field(id).unwrap().selection(), 8..11);
    view.present(view.revision(), vec![input]).unwrap();
    view.handle(key(KeyCode::Up)).unwrap();
    assert_eq!(view.field(id).unwrap().caret(), 6);
    // Search into the field must survive resize without moving document caret.
    view.find("5678").unwrap();
    let before = view.field(id).unwrap().caret();
    let b = paint(&mut view, Cells::new(0, 0, 4, 2));
    assert!(b.content.iter().any(|cell| cell.symbol() == "5"));
    assert_eq!(view.field(id).unwrap().caret(), before);
}
