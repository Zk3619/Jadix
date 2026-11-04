use jadix_native::{
    host::{HostError, NativeRoot, PlatformHost},
    layout::{LayoutError, MeasureRequest, NativeLayout},
    presentation::{capabilities::NodeKind, *},
    wire, BackendError, HeadlessRoot,
};

fn node(root: RootId, content: NodeContent, width: f32, height: f32) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
            style: presentation_style(width, height),
        },
        children: vec![],
    }
}

fn presentation_style(width: f32, height: f32) -> style::NodeStyle {
    style::NodeStyle {
        width: Some(width),
        height: Some(height),
        shrink: 0.0,
        ..Default::default()
    }
}

fn button(root: RootId, width: f32, height: f32) -> PresentationNode {
    node(
        root,
        NodeContent::Button {
            label: "Anchor".into(),
            enabled: true,
        },
        width,
        height,
    )
}

fn popover(
    root: RootId,
    anchor: NodeId,
    placement: PopoverPlacement,
    width: f32,
    height: f32,
) -> PresentationNode {
    node(
        root,
        NodeContent::Popover {
            anchor,
            placement,
            gap: 6,
            open: true,
            close_on_outside_click: true,
            revision: 1,
        },
        width,
        height,
    )
}

fn measure(request: MeasureRequest, _: &NodeDescriptor) -> Result<MeasureResult, LayoutError> {
    Ok(MeasureResult {
        key: request.key,
        width: 40.0_f64.min(request.key.max_width),
        height: 20.0_f64.min(request.key.max_height),
        baseline: None,
    })
}

fn scene(root: RootId, placement: PopoverPlacement) -> PresentationNode {
    let anchor = button(root, 80.0, 50.0);
    let panel = popover(root, anchor.id, placement, 40.0, 30.0);
    let mut container = node(root, NodeContent::Container, 400.0, 400.0);
    container.descriptor.style.padding = [100.0; 4];
    container.children = vec![anchor, panel, button(root, 20.0, 20.0)];
    container
}

#[test]
fn twelve_placements_use_anchor_frames_and_do_not_reserve_flow_space() {
    use PopoverPlacement::*;
    for (placement, expected) in [
        (TopStart, [100.0, 64.0]),
        (Top, [120.0, 64.0]),
        (TopEnd, [140.0, 64.0]),
        (BottomStart, [100.0, 156.0]),
        (Bottom, [120.0, 156.0]),
        (BottomEnd, [140.0, 156.0]),
        (LeftStart, [54.0, 100.0]),
        (Left, [54.0, 110.0]),
        (LeftEnd, [54.0, 120.0]),
        (RightStart, [186.0, 100.0]),
        (Right, [186.0, 110.0]),
        (RightEnd, [186.0, 120.0]),
    ] {
        let lifetime = RootLifetime::new(RootId::new());
        let mut nodes = vec![scene(lifetime.id(), placement)];
        let mut layout = NativeLayout::new(lifetime);
        layout
            .layout(&mut nodes, [400.0, 400.0], 1, measure)
            .unwrap();
        let frame = nodes[0].children[1].descriptor.frame;
        assert_eq!([frame.x, frame.y], expected, "{placement:?}");
        assert_eq!([frame.width, frame.height], [40.0, 30.0]);
        assert_eq!(nodes[0].children[2].descriptor.frame.y, 150.0);
    }
}

#[test]
fn placement_flips_to_roomier_side_and_clamps_cross_axis() {
    use PopoverPlacement::*;
    for (placement, padding, expected) in [
        (TopStart, [0.0, 0.0, 0.0, 0.0], [8.0, 56.0]),
        (LeftStart, [0.0, 0.0, 0.0, 0.0], [86.0, 8.0]),
        (BottomEnd, [340.0, 0.0, 0.0, 310.0], [350.0, 304.0]),
        (RightEnd, [340.0, 0.0, 0.0, 310.0], [264.0, 360.0]),
    ] {
        let lifetime = RootLifetime::new(RootId::new());
        let mut nodes = vec![scene(lifetime.id(), placement)];
        nodes[0].descriptor.style.padding = padding;
        NativeLayout::new(lifetime)
            .layout(&mut nodes, [400.0, 400.0], 1, measure)
            .unwrap();
        let frame = nodes[0].children[1].descriptor.frame;
        assert_eq!([frame.x, frame.y], expected, "{placement:?}");
    }
}

#[test]
fn resize_and_anchor_movement_reposition_retained_panels_without_remeasurement() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut nodes = vec![scene(lifetime.id(), PopoverPlacement::BottomStart)];
    let mut layout = NativeLayout::new(lifetime);
    layout
        .layout(&mut nodes, [400.0, 400.0], 1, measure)
        .unwrap();
    nodes[0].descriptor.style.padding = [20.0; 4];
    let stats = layout
        .layout(&mut nodes, [400.0, 400.0], 1, measure)
        .unwrap();
    assert_eq!(stats.measurements, 0);
    assert_eq!(nodes[0].children[1].descriptor.frame.x, 20.0);
    assert_eq!(nodes[0].children[1].descriptor.frame.y, 76.0);
    for viewport in [[30.0, 25.0], [0.0, 0.0], [8.0, 12.0], [200.0, 300.0]] {
        layout.layout(&mut nodes, viewport, 2, measure).unwrap();
        let frame = nodes[0].children[1].descriptor.frame;
        assert!(frame.is_valid());
        assert!(frame.x >= 0.0 && frame.y >= 0.0);
        assert!(frame.x + frame.width <= f64::from(viewport[0]));
        assert!(frame.y + frame.height <= f64::from(viewport[1]));
    }
}

#[test]
fn nested_anchor_dependencies_are_resolved_independently_of_tree_order() {
    let lifetime = RootLifetime::new(RootId::new());
    let anchor = button(lifetime.id(), 80.0, 50.0);
    let nested_anchor = button(lifetime.id(), 40.0, 30.0);
    let second = popover(
        lifetime.id(),
        nested_anchor.id,
        PopoverPlacement::RightStart,
        20.0,
        10.0,
    );
    let mut first = popover(
        lifetime.id(),
        anchor.id,
        PopoverPlacement::BottomStart,
        80.0,
        70.0,
    );
    first.descriptor.style.padding = [10.0; 4];
    first.children.push(nested_anchor);
    let mut container = node(lifetime.id(), NodeContent::Container, 400.0, 400.0);
    container.descriptor.style.padding = [100.0; 4];
    container.children = vec![second, anchor, first];
    let mut nodes = vec![container];
    NativeLayout::new(lifetime)
        .layout(&mut nodes, [400.0, 400.0], 1, measure)
        .unwrap();
    assert_eq!(nodes[0].children[2].descriptor.frame.y, 156.0);
    assert_eq!(nodes[0].children[2].children[0].descriptor.frame.x, 110.0);
    let second = nodes[0].children[0].descriptor.frame;
    assert_eq!([second.x, second.y], [156.0, 166.0]);
}

#[test]
fn anchor_removal_and_cross_root_updates_are_atomic() {
    let mut root = HeadlessRoot::new();
    let nodes = scene(root.id(), PopoverPlacement::BottomStart);
    root.reconcile(std::slice::from_ref(&nodes)).unwrap();
    let revision = root.revision();
    let mut missing = nodes.clone();
    missing.children.remove(0);
    assert_eq!(
        root.reconcile(&[missing]),
        Err(BackendError::InvalidAnchor(nodes.children[1].id))
    );
    assert_eq!(root.revision(), revision);
    assert!(root.node(nodes.children[0].id).is_some());
    let mut foreign = nodes.clone();
    if let NodeContent::Popover { anchor, .. } = &mut foreign.children[1].descriptor.content {
        *anchor = NodeId::new(RootId::new());
    }
    assert_eq!(
        root.reconcile(&[foreign]),
        Err(BackendError::InvalidAnchor(nodes.children[1].id))
    );
    assert_eq!(root.revision(), revision);
}

#[test]
fn anchor_descendants_and_cyclic_overlay_dependencies_are_rejected_before_measurement() {
    let lifetime = RootLifetime::new(RootId::new());
    let anchor = button(lifetime.id(), 20.0, 20.0);
    let mut first = popover(
        lifetime.id(),
        anchor.id,
        PopoverPlacement::BottomStart,
        60.0,
        60.0,
    );
    first.children.push(anchor);
    let mut nodes = vec![first];
    let mut layout = NativeLayout::new(lifetime.clone());
    assert_eq!(
        layout.layout(&mut nodes, [400.0, 400.0], 1, |_, _| panic!(
            "invalid tree was measured"
        )),
        Err(LayoutError::InvalidTree)
    );
    let anchor_first = button(lifetime.id(), 20.0, 20.0);
    let anchor_second = button(lifetime.id(), 20.0, 20.0);
    let mut first = popover(
        lifetime.id(),
        anchor_second.id,
        PopoverPlacement::BottomStart,
        60.0,
        60.0,
    );
    let mut second = popover(
        lifetime.id(),
        anchor_first.id,
        PopoverPlacement::BottomStart,
        60.0,
        60.0,
    );
    first.children.push(anchor_first);
    second.children.push(anchor_second);
    assert_eq!(
        layout.layout(&mut [first, second], [400.0, 400.0], 1, |_, _| panic!(
            "cyclic tree was measured"
        )),
        Err(LayoutError::InvalidTree)
    );
}

#[test]
fn nonmodal_panel_preserves_background_events_and_rejects_stale_dismissals() {
    let mut root = HeadlessRoot::new();
    let mut nodes = scene(root.id(), PopoverPlacement::BottomStart);
    let child = button(root.id(), 20.0, 20.0);
    nodes.children[1].children.push(child.clone());
    if let NodeContent::Popover {
        close_on_outside_click,
        ..
    } = &mut nodes.children[1].descriptor.content
    {
        *close_on_outside_click = false;
    }
    root.reconcile(std::slice::from_ref(&nodes)).unwrap();
    root.accept_event(&PlatformEvent {
        node: nodes.children[0].id,
        payload: EventPayload::Focus,
    })
    .unwrap();
    root.accept_event(&PlatformEvent {
        node: child.id,
        payload: EventPayload::Activate,
    })
    .unwrap();
    let mut event = PlatformEvent {
        node: nodes.children[1].id,
        payload: EventPayload::Dismiss {
            backdrop: true,
            revision: 2,
        },
    };
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    event.payload = EventPayload::Dismiss {
        backdrop: false,
        revision: 2,
    };
    root.accept_event(&event).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    if let NodeContent::Popover { open, revision, .. } = &mut nodes.children[1].descriptor.content {
        *open = false;
        *revision = 2;
    }
    root.reconcile(&[nodes]).unwrap();
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: child.id,
            payload: EventPayload::Activate
        }),
        Err(BackendError::InvalidEvent)
    );
    root.dispose();
    assert_eq!(root.accept_event(&event), Err(BackendError::Disposed));
}

#[test]
fn hidden_anchor_cannot_leave_a_visible_orphan_panel() {
    let mut root = HeadlessRoot::new();
    let anchor = button(root.id(), 20.0, 20.0);
    let mut dialog = node(
        root.id(),
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        },
        100.0,
        100.0,
    );
    dialog.children.push(anchor.clone());
    let mut panel = popover(
        root.id(),
        anchor.id,
        PopoverPlacement::BottomStart,
        40.0,
        30.0,
    );
    assert_eq!(
        root.reconcile(&[dialog.clone(), panel.clone()]),
        Err(BackendError::InvalidAnchor(panel.id))
    );
    if let NodeContent::Popover { open, .. } = &mut panel.descriptor.content {
        *open = false;
    }
    root.reconcile(&[dialog, panel]).unwrap();
}

#[test]
fn wire_retains_anchor_placement_and_full_revision_precision() {
    let root = RootId::new();
    let mut nodes = scene(root, PopoverPlacement::LeftEnd);
    if let NodeContent::Popover { revision, .. } = &mut nodes.children[1].descriptor.content {
        *revision = u64::MAX;
    }
    let value: serde_json::Value =
        serde_json::from_str(&wire::snapshot(root, 1, std::slice::from_ref(&nodes))).unwrap();
    assert_eq!(value["nodes"][2]["anchor"], nodes.children[0].id.token());
    assert_eq!(value["nodes"][2]["placement"], "left-end");
    assert_eq!(value["nodes"][2]["gap"], 6);
    assert_eq!(value["nodes"][2]["editRevision"], u64::MAX.to_string());
    assert_eq!(value["nodes"][2]["kind"], "popover");
}

struct UnsupportedHost;
#[test]
fn viewport_constraints_bound_explicit_minimums_and_oversized_padding_fails_closed() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut nodes = vec![scene(lifetime.id(), PopoverPlacement::BottomStart)];
    let panel = &mut nodes[0].children[1];
    panel.descriptor.style.min_width = 1000.0;
    panel.descriptor.style.min_height = 1000.0;
    let mut layout = NativeLayout::new(lifetime);
    layout
        .layout(&mut nodes, [100.0, 80.0], 1, measure)
        .unwrap();
    let frame = nodes[0].children[1].descriptor.frame;
    assert!(frame.width <= 84.0 && frame.height <= 64.0);
    nodes[0].children[1].descriptor.style.padding = [100.0; 4];
    assert_eq!(
        layout.layout(&mut nodes, [100.0, 80.0], 2, measure),
        Err(LayoutError::InvalidStyle)
    );
    assert_eq!(nodes[0].children[1].descriptor.frame, frame);
}

#[test]
fn closed_popover_skips_measurement_and_resets_descendant_frames() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut nodes = vec![scene(lifetime.id(), PopoverPlacement::BottomStart)];
    nodes[0].children[1]
        .children
        .push(button(lifetime.id(), 20.0, 20.0));
    let mut layout = NativeLayout::new(lifetime);
    layout
        .layout(&mut nodes, [400.0, 400.0], 1, measure)
        .unwrap();
    assert!(nodes[0].children[1].children[0].descriptor.frame.height > 0.0);
    if let NodeContent::Popover { open, .. } = &mut nodes[0].children[1].descriptor.content {
        *open = false;
    }
    nodes[0].children[1].children[0].descriptor.content =
        NodeContent::Text("hidden changed content".into());
    layout
        .layout(&mut nodes, [400.0, 400.0], 1, |_, _| {
            panic!("closed panel must not measure descendants")
        })
        .unwrap();
    assert_eq!(nodes[0].children[1].descriptor.frame, Rect::default());
    assert_eq!(
        nodes[0].children[1].children[0].descriptor.frame,
        Rect::default()
    );
}

#[test]
fn enclosing_modal_keeps_its_event_boundary_when_popover_opens() {
    let mut root = HeadlessRoot::new();
    let background = button(root.id(), 20.0, 20.0);
    let mut contents = scene(root.id(), PopoverPlacement::BottomStart);
    let action = button(root.id(), 20.0, 20.0);
    contents.children[1].children.push(action.clone());
    let mut dialog = node(
        root.id(),
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 1,
        },
        400.0,
        400.0,
    );
    dialog.children.push(contents);
    root.reconcile(&[background.clone(), dialog]).unwrap();
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: background.id,
            payload: EventPayload::Activate
        }),
        Err(BackendError::InvalidEvent)
    );
    root.accept_event(&PlatformEvent {
        node: action.id,
        payload: EventPayload::Focus,
    })
    .unwrap();
    assert_eq!(root.focused(), Some(action.id));
}

#[test]
fn invalid_layout_preserves_previously_published_geometry() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut nodes = vec![scene(lifetime.id(), PopoverPlacement::BottomStart)];
    let mut layout = NativeLayout::new(lifetime);
    layout
        .layout(&mut nodes, [400.0, 400.0], 1, measure)
        .unwrap();
    let previous = nodes[0].children[1].descriptor.frame;
    if let NodeContent::Popover { anchor, .. } = &mut nodes[0].children[1].descriptor.content {
        *anchor = NodeId::new(RootId::new());
    }
    assert_eq!(
        layout.layout(&mut nodes, [200.0, 200.0], 2, |_, _| panic!(
            "invalid anchor was measured"
        )),
        Err(LayoutError::InvalidTree)
    );
    assert_eq!(nodes[0].children[1].descriptor.frame, previous);
}

impl PlatformHost for UnsupportedHost {
    fn measure(
        &mut self,
        _: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        panic!("unsupported content was measured")
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        panic!("unsupported content was published")
    }
    fn dispose(&mut self) {}
}

#[test]
fn platform_hosts_fail_closed_until_they_explicitly_advertise_popover() {
    let mut root = NativeRoot::new(UnsupportedHost);
    let mut nodes = vec![scene(root.model().id(), PopoverPlacement::BottomStart)];
    assert!(matches!(
        root.present(&mut nodes, [400.0, 400.0], 1),
        Err(HostError::Backend(BackendError::Unsupported(
            NodeKind::Popover
        )))
    ));
    assert_eq!(root.model().revision(), 0);
    assert!(root.model().roots().is_empty());
}
