use base64::Engine;
use jadix_native::{
    presentation::*, BackendError, HeadlessRoot, ResourceKind, ResourceLimits, ResourceUsage,
};

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
        children: vec![],
    }
}

fn image(width: u32, height: u32, bytes: usize) -> NodeContent {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(width.to_be_bytes());
    png.extend(height.to_be_bytes());
    png.resize(bytes.max(33), 0);
    NodeContent::Image {
        source: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ),
        alt: "Budget probe".into(),
    }
}

#[test]
fn many_individually_legal_images_cannot_exceed_the_pixel_budget() {
    let mut root = HeadlessRoot::new();
    let nodes: Vec<_> = (0..5)
        .map(|_| node(root.id(), image(2048, 2048, 33)))
        .collect();
    assert!(nodes.iter().all(|node| node.descriptor.content.is_valid()));
    assert!(root.reconcile(&nodes).is_err());
    assert_eq!(root.revision(), 0);
    assert!(root.roots().is_empty());
}

#[test]
fn encoded_images_and_text_have_separate_aggregate_budgets() {
    let mut root = HeadlessRoot::new();
    let images: Vec<_> = (0..9)
        .map(|_| node(root.id(), image(1, 1, 750_000)))
        .collect();
    assert!(images.iter().all(|node| node.descriptor.content.is_valid()));
    assert!(root.reconcile(&images).is_err());
    let text: Vec<_> = (0..9)
        .map(|_| node(root.id(), NodeContent::Text("x".repeat(1024 * 1024))))
        .collect();
    assert!(root.reconcile(&text).is_err());
    assert_eq!(root.revision(), 0);
}

#[test]
fn aggregate_node_limit_rejects_a_large_shallow_tree() {
    let mut root = HeadlessRoot::new();
    let nodes: Vec<_> = (0..4097)
        .map(|_| node(root.id(), NodeContent::Container))
        .collect();
    assert!(root.reconcile(&nodes).is_err());
    assert_eq!(root.revision(), 0);
}

fn commit(root: &mut HeadlessRoot, operations: Vec<Operation>) -> Result<bool, BackendError> {
    root.commit(CommitBatch {
        root: root.id(),
        previous: root.revision(),
        revision: root.revision() + 1,
        operations,
    })
}

#[test]
fn failed_direct_commit_preserves_tree_focus_usage_and_revision() {
    let mut root = HeadlessRoot::with_limits(ResourceLimits {
        text_bytes: 12,
        ..Default::default()
    });
    let mut input = node(
        root.id(),
        NodeContent::Input {
            value: "中😀".into(),
            placeholder: "Name".into(),
            enabled: true,
            password: false,
            revision: 0,
        },
    );
    root.reconcile(std::slice::from_ref(&input)).unwrap();
    root.accept_event(&PlatformEvent {
        node: input.id,
        payload: EventPayload::Focus,
    })
    .unwrap();
    let usage = root.resource_usage();
    assert_eq!(usage.text_bytes, 11);
    let revision = root.revision();
    input.descriptor.accessible_name = Some("overflow".into());
    assert_eq!(
        commit(
            &mut root,
            vec![Operation::Update {
                node: input.id,
                descriptor: input.descriptor
            }]
        ),
        Err(BackendError::ResourceLimit(ResourceKind::TextBytes))
    );
    assert_eq!(root.revision(), revision);
    assert_eq!(root.resource_usage(), usage);
    assert_eq!(root.focused(), Some(input.id));
    assert!(root
        .node(input.id)
        .unwrap()
        .descriptor
        .accessible_name
        .is_none());
}

#[test]
fn atomic_replacement_uses_final_tree_budget_and_bounds_retired_identities() {
    let mut root = HeadlessRoot::with_limits(ResourceLimits {
        nodes: 1,
        retired_nodes: 1,
        text_bytes: 8,
        ..Default::default()
    });
    let original = node(root.id(), NodeContent::Text("original".into()));
    root.reconcile(std::slice::from_ref(&original)).unwrap();
    let replacement = node(root.id(), NodeContent::Text("next".into()));
    commit(
        &mut root,
        vec![
            Operation::Create {
                node: replacement.id,
                parent: None,
                index: 1,
                descriptor: replacement.descriptor.clone(),
            },
            Operation::Remove { node: original.id },
        ],
    )
    .unwrap();
    assert_eq!(root.roots(), &[replacement.id]);
    assert_eq!(root.resource_usage().nodes, 1);
    assert_eq!(root.resource_usage().retired_nodes, 1);
    let revision = root.revision();
    assert_eq!(
        commit(
            &mut root,
            vec![Operation::Remove {
                node: replacement.id
            }]
        ),
        Err(BackendError::ResourceLimit(ResourceKind::RetiredNodes))
    );
    assert_eq!(root.revision(), revision);
    assert_eq!(root.roots(), &[replacement.id]);
    assert_eq!(root.resource_limits().retired_nodes, 1);
    root.dispose();
    assert_eq!(root.resource_usage(), ResourceUsage::default());
}

#[test]
fn long_lived_virtual_root_compresses_retirement_and_rejects_old_identities() {
    let mut root = HeadlessRoot::with_limits(ResourceLimits {
        nodes: 1,
        retired_nodes: 100_000,
        ..Default::default()
    });
    let original = node(root.id(), NodeContent::Text("original".into()));
    root.reconcile(std::slice::from_ref(&original)).unwrap();
    for index in 0..70_000 {
        let next = node(root.id(), NodeContent::Text(index.to_string()));
        root.reconcile(&[next]).unwrap();
    }
    assert_eq!(root.resource_usage().nodes, 1);
    assert_eq!(root.resource_usage().retired_nodes, 70_000);
    assert!(root.resource_usage().retired_ranges < 1000);
    let revision = root.revision();
    assert_eq!(
        root.reconcile(&[original.clone()]),
        Err(BackendError::ReusedNode(original.id))
    );
    assert_eq!(root.revision(), revision);
}

#[test]
fn fragmented_retirement_fails_atomically_at_its_storage_budget() {
    let mut root = HeadlessRoot::with_limits(ResourceLimits {
        retired_nodes: 100,
        retired_ranges: 1,
        ..Default::default()
    });
    let first = node(root.id(), NodeContent::Text("first".into()));
    let still_live = node(root.id(), NodeContent::Text("middle".into()));
    let last = node(root.id(), NodeContent::Text("last".into()));
    root.reconcile(&[first, still_live.clone(), last.clone()])
        .unwrap();
    root.reconcile(&[still_live.clone(), last]).unwrap();
    let revision = root.revision();
    assert_eq!(
        root.reconcile(&[still_live]),
        Err(BackendError::ResourceLimit(ResourceKind::RetiredRanges))
    );
    assert_eq!(root.revision(), revision);
    assert_eq!(root.resource_usage().retired_nodes, 1);
}

#[test]
fn hidden_dialog_resources_are_retained_and_counted_per_node() {
    let mut root = HeadlessRoot::with_limits(ResourceLimits {
        image_pixels: 8,
        ..Default::default()
    });
    let mut dialog = node(
        root.id(),
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: true,
            revision: 0,
        },
    );
    dialog.children = (0..3).map(|_| node(root.id(), image(2, 2, 33))).collect();
    assert_eq!(
        root.reconcile(std::slice::from_ref(&dialog)),
        Err(BackendError::ResourceLimit(ResourceKind::ImagePixels))
    );
    dialog.children.pop();
    root.reconcile(std::slice::from_ref(&dialog)).unwrap();
    assert_eq!(root.resource_usage().image_pixels, 8);
    assert_eq!(root.resource_usage().nodes, 3);
    assert_eq!(root.resource_usage().text_bytes, 24);
}

#[test]
fn host_rejects_over_budget_trees_before_measurement_and_keeps_serving() {
    use jadix_native::{host::*, layout::*};
    use std::{cell::Cell, rc::Rc};
    struct Host {
        measurements: Rc<Cell<usize>>,
        publications: Rc<Cell<usize>>,
    }
    impl PlatformHost for Host {
        fn measure(
            &mut self,
            request: MeasureRequest,
            _: &NodeDescriptor,
        ) -> Result<MeasureResult, LayoutError> {
            self.measurements.set(self.measurements.get() + 1);
            Ok(MeasureResult {
                key: request.key,
                width: 20.0,
                height: 20.0,
                baseline: None,
            })
        }
        fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
            self.publications.set(self.publications.get() + 1);
            Ok(())
        }
        fn dispose(&mut self) {}
    }
    let measurements = Rc::new(Cell::new(0));
    let publications = Rc::new(Cell::new(0));
    let mut root = NativeRoot::with_limits(
        Host {
            measurements: measurements.clone(),
            publications: publications.clone(),
        },
        ResourceLimits {
            text_bytes: 8,
            ..Default::default()
        },
    );
    let mut roots = vec![node(root.model().id(), NodeContent::Text("valid".into()))];
    root.present(&mut roots, [320.0, 480.0], 1).unwrap();
    let measured = measurements.get();
    let revision = root.model().revision();
    roots[0].descriptor.content = NodeContent::Text("over budget".into());
    assert!(matches!(
        root.present(&mut roots, [320.0, 480.0], 2),
        Err(HostError::Backend(BackendError::ResourceLimit(
            ResourceKind::TextBytes
        )))
    ));
    assert_eq!(measurements.get(), measured);
    assert_eq!(publications.get(), 1);
    assert_eq!(root.model().revision(), revision);
    assert_eq!(root.model().resource_usage().text_bytes, 5);
    roots[0].descriptor.content = NodeContent::Text("recovery".into());
    root.present(&mut roots, [320.0, 480.0], 3).unwrap();
    assert_eq!(publications.get(), 2);
}
