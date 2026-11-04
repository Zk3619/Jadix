use jadix_native::{layout::*, presentation::*, HeadlessRoot};

fn node(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
            style: Default::default(),
        },
        children: Vec::new(),
    }
}

fn measure(
    request: MeasureRequest,
    descriptor: &NodeDescriptor,
) -> Result<MeasureResult, LayoutError> {
    let text = match &descriptor.content {
        NodeContent::Text(text) | NodeContent::Button { label: text, .. } => text,
        NodeContent::Input { value, .. } => value,
        _ => panic!("containers must not be platform measured"),
    };
    Ok(MeasureResult {
        key: request.key,
        width: (text.chars().count() as f64 * f64::from(descriptor.style.font_size))
            .min(request.key.max_width),
        height: f64::from(descriptor.style.font_size).min(request.key.max_height),
        baseline: Some((f64::from(descriptor.style.font_size) * 0.75).min(request.key.max_height)),
    })
}

#[test]
fn measurement_drives_taffy_frames_and_retained_commits() {
    let mut backend = HeadlessRoot::new();
    let lifetime = backend.lifetime();
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut root = node(lifetime.id(), NodeContent::Container);
    root.descriptor.style.width = Some(200.0);
    root.descriptor.style.padding = [5.0; 4];
    root.descriptor.style.gap = 3.0;
    root.children = vec![
        node(lifetime.id(), NodeContent::Text("中文".into())),
        node(
            lifetime.id(),
            NodeContent::Button {
                label: "OK".into(),
                enabled: true,
            },
        ),
    ];
    let mut roots = vec![root];
    let stats = layout
        .layout(&mut roots, [300.0, 300.0], 1, measure)
        .unwrap();
    assert_eq!(stats.nodes, 3);
    assert!(stats.measurements >= 2);
    assert_eq!(roots[0].children[0].descriptor.frame.y, 5.0);
    assert_eq!(roots[0].children[1].descriptor.frame.y, 24.0);
    assert_eq!(roots[0].descriptor.frame.height, 45.0);
    assert!(backend.reconcile(&roots).unwrap());
    let stats = layout
        .layout(&mut roots, [300.0, 300.0], 1, |_, _| {
            panic!("unchanged content must hit cache")
        })
        .unwrap();
    assert_eq!(stats.measurements, 0);
    assert!(stats.cache_hits >= 2);
    assert!(!backend.reconcile(&roots).unwrap());
}

#[test]
fn palette_changes_reuse_text_metrics_but_font_changes_still_remeasure() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut roots = vec![node(
        lifetime.id(),
        NodeContent::Text("retained metrics".into()),
    )];
    layout
        .layout(&mut roots, [320.0, 240.0], 0, measure)
        .unwrap();
    let before = roots[0].descriptor.frame;
    roots[0].descriptor.style.foreground = [230, 231, 232, 255];
    roots[0].descriptor.style.background = [10, 20, 30, 255];
    let stats = layout
        .layout(&mut roots, [320.0, 240.0], 0, |_, _| {
            panic!("palette change must reuse measurement")
        })
        .unwrap();
    assert_eq!(stats.measurements, 0);
    assert_eq!(roots[0].descriptor.frame, before);
    roots[0].descriptor.style.font_size = 24.0;
    assert!(
        layout
            .layout(&mut roots, [320.0, 240.0], 0, measure)
            .unwrap()
            .measurements
            > 0
    );
}

#[test]
fn dialog_is_a_viewport_portal_and_hidden_children_are_not_measured() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut panel = node(
        lifetime.id(),
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        },
    );
    panel.descriptor.style.width = Some(200.0);
    panel.descriptor.style.height = Some(120.0);
    panel.descriptor.style.padding = [12.0; 4];
    panel
        .children
        .push(node(lifetime.id(), NodeContent::Text("Modal".into())));
    let mut container = node(lifetime.id(), NodeContent::Container);
    let mut before = node(lifetime.id(), NodeContent::Container);
    before.descriptor.style.height = Some(44.0);
    let mut after = node(lifetime.id(), NodeContent::Container);
    after.descriptor.style.height = Some(30.0);
    container.children = vec![before, panel, after];
    let mut roots = vec![container];
    layout
        .layout(&mut roots, [360.0, 640.0], 1, |_, _| {
            panic!("closed dialog must not measure content")
        })
        .unwrap();
    assert_eq!(roots[0].descriptor.frame.height, 74.0);
    assert_eq!(
        roots[0].children[1].children[0].descriptor.frame,
        Rect::default()
    );
    roots[0].children[1].descriptor.content = NodeContent::Dialog {
        open: true,
        close_on_overlay_click: true,
        revision: 1,
    };
    layout
        .layout(&mut roots, [360.0, 640.0], 1, measure)
        .unwrap();
    assert_eq!(roots[0].descriptor.frame.height, 74.0);
    assert_eq!(roots[0].children[2].descriptor.frame.y, 44.0);
    let frame = roots[0].children[1].descriptor.frame;
    assert_eq!(
        frame,
        Rect {
            x: 80.0,
            y: 260.0,
            width: 200.0,
            height: 120.0
        }
    );
    assert_eq!(roots[0].children[1].children[0].descriptor.frame.x, 92.0);
    layout
        .layout(&mut roots, [180.0, 100.0], 2, measure)
        .unwrap();
    let frame = roots[0].children[1].descriptor.frame;
    assert_eq!(frame.width, 148.0);
    assert_eq!(frame.height, 68.0);
    assert!(frame.x >= 0.0 && frame.y >= 0.0);
}

#[test]
fn image_metadata_keeps_aspect_ratio_and_failed_decode_geometry() {
    use base64::Engine;
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let source = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPQqPjwHwAE7AKQVV27NwAAAABJRU5ErkJggg==";
    let mut image = node(
        lifetime.id(),
        NodeContent::Image {
            source: source.into(),
            alt: "Pixel".into(),
        },
    );
    image.descriptor.style.width = Some(24.0);
    let mut roots = vec![image];
    layout
        .layout(&mut roots, [100.0, 100.0], 1, |_, _| {
            panic!("image size comes from bounded resource metadata")
        })
        .unwrap();
    let original = roots[0].descriptor.frame;
    assert_eq!([original.width, original.height], [24.0, 24.0]);
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(source.split_once(',').unwrap().1)
        .unwrap();
    bytes[25] = 99;
    roots[0].descriptor.content = NodeContent::Image {
        source: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ),
        alt: "Unavailable".into(),
    };
    layout
        .layout(&mut roots, [100.0, 100.0], 1, |_, _| {
            panic!("decode success cannot change resource geometry")
        })
        .unwrap();
    assert_eq!(roots[0].descriptor.frame, original);
}

#[test]
fn content_font_constraints_and_environment_invalidate_measurements() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut roots = vec![node(lifetime.id(), NodeContent::Text("hello".into()))];
    layout
        .layout(&mut roots, [100.0, 100.0], 1, measure)
        .unwrap();
    for change in 0..4 {
        match change {
            0 => roots[0].descriptor.content = NodeContent::Text("updated".into()),
            1 => roots[0].descriptor.style.font_size = 20.0,
            _ => {}
        }
        let stats = layout
            .layout(
                &mut roots,
                [if change == 2 { 80.0 } else { 100.0 }, 100.0],
                if change == 3 { 2 } else { 1 },
                measure,
            )
            .unwrap();
        assert!(stats.measurements > 0, "change {change} must remeasure");
    }
}

#[test]
fn stale_result_does_not_publish_partial_geometry() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut roots = vec![node(lifetime.id(), NodeContent::Text("hello".into()))];
    layout
        .layout(&mut roots, [100.0, 100.0], 1, measure)
        .unwrap();
    let previous = roots[0].descriptor.frame;
    roots[0].descriptor.content = NodeContent::Text("new".into());
    let error = layout.layout(&mut roots, [100.0, 100.0], 2, |request, descriptor| {
        let mut result = measure(request, descriptor)?;
        result.key.environment_revision = 1;
        Ok(result)
    });
    assert_eq!(error, Err(LayoutError::InvalidMeasurement(roots[0].id)));
    assert_eq!(roots[0].descriptor.frame, previous);
    layout
        .layout(&mut roots, [100.0, 100.0], 2, measure)
        .unwrap();
}

#[test]
fn malformed_trees_and_styles_fail_before_platform_calls() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut root = node(lifetime.id(), NodeContent::Text("hello".into()));
    root.children
        .push(node(lifetime.id(), NodeContent::Container));
    let mut roots = vec![root];
    assert_eq!(
        layout.layout(&mut roots, [100.0; 2], 1, |_, _| panic!()),
        Err(LayoutError::InvalidTree)
    );
    roots[0].children.clear();
    roots[0].descriptor.style.font_size = f32::NAN;
    assert_eq!(
        layout.layout(&mut roots, [100.0; 2], 1, |_, _| panic!()),
        Err(LayoutError::InvalidStyle)
    );
    assert_eq!(
        layout.layout(&mut roots, [f32::INFINITY; 2], 1, |_, _| panic!()),
        Err(LayoutError::InvalidViewport)
    );
}

#[test]
fn removed_nodes_release_cache_and_revocation_blocks_measurement_publication() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut layout = NativeLayout::new(lifetime.clone());
    let mut roots = vec![node(lifetime.id(), NodeContent::Text("hello".into()))];
    layout.layout(&mut roots, [100.0; 2], 1, measure).unwrap();
    assert_eq!(layout.cached_nodes(), 1);
    roots.clear();
    layout.layout(&mut roots, [100.0; 2], 1, measure).unwrap();
    assert_eq!(layout.cached_nodes(), 0);
    roots.push(node(lifetime.id(), NodeContent::Text("late".into())));
    assert_eq!(
        layout.layout(&mut roots, [100.0; 2], 1, |request, descriptor| {
            lifetime.dispose();
            measure(request, descriptor)
        }),
        Err(LayoutError::Disposed)
    );
    assert_eq!(roots[0].descriptor.frame, Rect::default());
    assert_eq!(layout.cached_nodes(), 0);
}
