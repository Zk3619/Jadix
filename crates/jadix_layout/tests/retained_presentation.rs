use jadix_core::presentation::{style::NodeStyle, *};
use jadix_layout::presentation::*;
fn node(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            style: NodeStyle {
                shrink: 0.,
                ..Default::default()
            },
            frame: Rect::default(),
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
fn measure(r: MeasureRequest, d: &NodeDescriptor) -> Result<MeasureResult, LayoutError> {
    let text = match &d.content {
        NodeContent::Text(s) | NodeContent::Button { label: s, .. } => s.as_str(),
        _ => "",
    };
    Ok(MeasureResult {
        key: r.key,
        width: (text.chars().count() as f64 * f64::from(d.style.font_size) * 0.5)
            .min(r.key.max_width),
        height: f64::from(d.style.font_size).min(r.key.max_height),
        baseline: None,
    })
}
fn equivalent(
    layout: &mut PresentationLayout,
    roots: &mut [PresentationNode],
    life: &RootLifetime,
    viewport: [f32; 2],
    env: u64,
) -> LayoutStats {
    let mut fresh = roots.to_vec();
    let expected = PresentationLayout::new(life.clone())
        .layout(&mut fresh, viewport, env, measure)
        .unwrap();
    let actual = layout.layout(roots, viewport, env, measure).unwrap();
    assert_eq!(roots, fresh);
    assert_eq!(actual.nodes, expected.nodes);
    actual
}
#[test]
fn stable_nodes_keep_engine_caches_and_structural_changes_match_a_fresh_engine() {
    let life = RootLifetime::new(RootId::new());
    let root = life.id();
    let mut layout = PresentationLayout::new(life.clone());
    let mut a = node(root, NodeContent::Container);
    a.descriptor.style.width = Some(180.);
    a.descriptor.style.gap = 4.;
    a.children = (0..40)
        .map(|i| node(root, NodeContent::Text(format!("row {i}"))))
        .collect();
    let b = node(root, NodeContent::Container);
    let mut roots = vec![a, b];
    let initial = equivalent(&mut layout, &mut roots, &life, [400., 500.], 0);
    assert_eq!(initial.engine_created, 43);
    let next = layout
        .layout(&mut roots, [400., 500.], 0, |_, _| {
            panic!("stable layout must use Taffy cache")
        })
        .unwrap();
    assert_eq!(next.engine_created, 0);
    assert_eq!(next.engine_reused, 43);
    assert_eq!(next.engine_updated, 0);
    assert_eq!(next.children_updated, 0);
    assert_eq!(next.measurements, 0);
    roots[0].children[4].descriptor.content =
        NodeContent::Text("longer content that changes width".into());
    let changed = equivalent(&mut layout, &mut roots, &life, [400., 500.], 0);
    assert_eq!(changed.engine_created, 0);
    assert_eq!(changed.engine_updated, 1);
    assert!(changed.measurements > 0);
    let moved = roots[0].children.remove(2);
    roots[1].children.push(moved);
    roots[0].children.reverse();
    roots[0]
        .children
        .insert(3, node(root, NodeContent::Text("new".into())));
    roots[0].children.pop();
    let structural = equivalent(&mut layout, &mut roots, &life, [400., 500.], 0);
    assert_eq!(structural.engine_created, 1);
    assert_eq!(structural.engine_removed, 1);
    assert!(structural.children_updated >= 2);
    roots[0].descriptor.style.width = Some(120.);
    roots[0].children[0].descriptor.style.font_size = 21.;
    equivalent(&mut layout, &mut roots, &life, [240., 320.], 0);
    let env = equivalent(&mut layout, &mut roots, &life, [240., 320.], 1);
    assert!(env.engine_created > 0);
    layout.release_layout_cache();
    let reset = equivalent(&mut layout, &mut roots, &life, [240., 320.], 1);
    assert!(reset.engine_created > 0);
    assert_eq!(reset.measurements, 0);
}
#[test]
fn panels_anchor_changes_and_open_close_preserve_fresh_engine_geometry() {
    let life = RootLifetime::new(RootId::new());
    let root = life.id();
    let mut layout = PresentationLayout::new(life.clone());
    let mut container = node(root, NodeContent::Container);
    container.descriptor.style.width = Some(220.);
    container.descriptor.style.padding = [12.; 4];
    let anchor = node(
        root,
        NodeContent::Button {
            label: "Anchor".into(),
            enabled: true,
        },
    );
    let aid = anchor.id;
    let mut popup = node(
        root,
        NodeContent::Popover {
            anchor: aid,
            placement: PopoverPlacement::BottomStart,
            gap: 8,
            open: true,
            close_on_outside_click: true,
            revision: 0,
        },
    );
    popup
        .children
        .push(node(root, NodeContent::Text("panel text".into())));
    popup.descriptor.style.width = Some(140.);
    let nested_anchor = node(
        root,
        NodeContent::Button {
            label: "Nested".into(),
            enabled: true,
        },
    );
    let nid = nested_anchor.id;
    popup.children.push(nested_anchor);
    let mut nested = node(
        root,
        NodeContent::Popover {
            anchor: nid,
            placement: PopoverPlacement::RightStart,
            gap: 4,
            open: true,
            close_on_outside_click: true,
            revision: 0,
        },
    );
    nested.descriptor.style.width = Some(80.);
    nested
        .children
        .push(node(root, NodeContent::Text("nested panel".into())));
    container.children = vec![popup, anchor, nested];
    let mut roots = vec![container];
    equivalent(&mut layout, &mut roots, &life, [400., 300.], 0);
    let stable = equivalent(&mut layout, &mut roots, &life, [400., 300.], 0);
    assert_eq!(stable.engine_created, 0);
    assert_eq!(stable.children_updated, 0);
    assert_eq!(stable.engine_updated, 0);
    roots[0].children[0].children[0].descriptor.content =
        NodeContent::Text("new panel contents".into());
    equivalent(&mut layout, &mut roots, &life, [280., 200.], 0);
    if let NodeContent::Popover { open, .. } = &mut roots[0].children[0].descriptor.content {
        *open = false;
    }
    if let NodeContent::Popover { open, .. } = &mut roots[0].children[2].descriptor.content {
        *open = false;
    }
    let closed = equivalent(&mut layout, &mut roots, &life, [280., 200.], 0);
    assert!(closed.engine_removed >= 4);
    if let NodeContent::Popover { open, .. } = &mut roots[0].children[0].descriptor.content {
        *open = true;
    }
    if let NodeContent::Popover { open, .. } = &mut roots[0].children[2].descriptor.content {
        *open = true;
    }
    equivalent(&mut layout, &mut roots, &life, [500., 400.], 0);
}
#[test]
fn failed_measure_and_invalid_reparent_never_publish_partial_frames_and_retry_rebuilds() {
    let life = RootLifetime::new(RootId::new());
    let root = life.id();
    let mut layout = PresentationLayout::new(life.clone());
    let mut roots = vec![node(root, NodeContent::Text("before".into()))];
    equivalent(&mut layout, &mut roots, &life, [200., 100.], 0);
    let before = roots[0].descriptor.frame;
    roots[0].descriptor.content = NodeContent::Text("after".into());
    let failure = layout.layout(&mut roots, [200., 100.], 0, |r, _| {
        let mut bad = measure(r, &roots_placeholder(root))?;
        bad.key.environment_revision = 9;
        Ok(bad)
    });
    assert!(matches!(failure, Err(LayoutError::InvalidMeasurement(_))));
    assert_eq!(roots[0].descriptor.frame, before);
    let retry = equivalent(&mut layout, &mut roots, &life, [200., 100.], 0);
    assert!(retry.engine_created > 0);
    // Previously A→B, next B→A: detach changed parents before reattachment.
    let mut a = node(root, NodeContent::Container);
    let b = node(root, NodeContent::Container);
    a.children.push(b);
    roots = vec![a];
    equivalent(&mut layout, &mut roots, &life, [200., 100.], 0);
    let mut a = roots.pop().unwrap();
    let mut b = a.children.pop().unwrap();
    b.children.push(a);
    roots.push(b);
    equivalent(&mut layout, &mut roots, &life, [200., 100.], 0);
    life.dispose();
    assert_eq!(
        layout.layout(&mut roots, [200., 100.], 0, measure),
        Err(LayoutError::Disposed)
    );
}
fn roots_placeholder(root: RootId) -> NodeDescriptor {
    node(root, NodeContent::Text("unused".into())).descriptor
}
