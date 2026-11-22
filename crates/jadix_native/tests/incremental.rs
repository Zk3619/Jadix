use jadix_native::{
    host::{ApplyReceipt, NativeRoot, PlatformHost},
    layout::{LayoutError, MeasureRequest},
    presentation::*,
    HeadlessRoot,
};

fn node(root: RootId) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content: NodeContent::Button {
                label: "initial".into(),
                enabled: true,
            },
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: Default::default(),
        },
        children: vec![],
    }
}

#[test]
fn incremental_batch_replays_to_the_same_tree_and_idle_has_no_batch() {
    let mut source = HeadlessRoot::new();
    let mut replica = HeadlessRoot::with_lifetime(source.lifetime(), source.capabilities().clone());
    let mut roots = vec![node(source.id()), node(source.id())];
    let first = source.reconcile_batch(&roots).unwrap().unwrap();
    replica.commit(first).unwrap();
    assert!(source.reconcile_batch(&roots).unwrap().is_none());
    roots.swap(0, 1);
    roots[0].descriptor.content = NodeContent::Button {
        label: "changed".into(),
        enabled: false,
    };
    let changed = source.reconcile_batch(&roots).unwrap().unwrap();
    assert_eq!(changed.previous, 1);
    assert_eq!(changed.revision, 2);
    assert!(changed
        .operations
        .iter()
        .all(|operation| matches!(operation, Operation::Move { .. } | Operation::Update { .. })));
    replica.commit(changed).unwrap();
    assert_eq!(source.roots(), replica.roots());
    for root in &roots {
        assert_eq!(source.node(root.id), replica.node(root.id));
    }
}

struct IncrementalHost {
    batches: Vec<CommitBatch>,
    foreign_receipt: bool,
    disposed: bool,
}

#[test]
fn programmatic_focus_publishes_one_validated_operation_and_rejects_foreign_receipts() {
    let host = IncrementalHost {
        batches: vec![],
        foreign_receipt: false,
        disposed: false,
    };
    let mut root = NativeRoot::new(host);
    let button = node(root.model().id());
    let identifier = button.id;
    root.present(&mut [button], [320.0, 240.0], 0).unwrap();
    root.focus(Some(identifier)).unwrap();
    assert_eq!(root.model().focused(), Some(identifier));
    assert_eq!(
        root.host().batches.last().unwrap().operations,
        vec![Operation::Focus {
            node: Some(identifier)
        }]
    );
    root.focus(Some(identifier)).unwrap();
    assert_eq!(root.host().batches.len(), 2);
    assert!(root.focus(Some(NodeId::new(RootId::new()))).is_err());
    assert_eq!(root.host().batches.len(), 2);
    root.host_mut().foreign_receipt = true;
    assert!(root.focus(None).is_err());
    assert!(root.model().lifetime().is_disposed());
    assert!(root.host().disposed);
}

impl PlatformHost for IncrementalHost {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: 70.0,
            height: 24.0,
            baseline: None,
        })
    }

    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        panic!("incremental host must not use the snapshot path")
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        _: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        self.batches.push(batch.clone());
        let mut receipt = ApplyReceipt::for_batch(batch);
        if self.foreign_receipt {
            receipt.root = RootId::new();
        }
        Ok(receipt)
    }

    fn dispose(&mut self) {
        self.disposed = true;
    }
}

#[test]
fn native_root_publishes_only_incremental_changes_and_checks_receipts() {
    let host = IncrementalHost {
        batches: vec![],
        foreign_receipt: false,
        disposed: false,
    };
    let mut root = NativeRoot::new(host);
    let mut roots = vec![node(root.model().id())];
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    assert_eq!(root.host().batches.len(), 1);
    roots[0].descriptor.content = NodeContent::Button {
        label: "changed".into(),
        enabled: true,
    };
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    assert_eq!(root.host().batches.len(), 2);
    assert_eq!(root.host().batches[1].operations.len(), 1);
    let host = IncrementalHost {
        batches: vec![],
        foreign_receipt: true,
        disposed: false,
    };
    let mut failing = NativeRoot::new(host);
    let mut candidate = vec![node(failing.model().id())];
    assert!(failing.present(&mut candidate, [320.0, 240.0], 0).is_err());
    assert!(failing.host().disposed);
    assert!(failing.model().lifetime().is_disposed());
    assert!(!root.model().lifetime().is_disposed());
}

#[test]
fn inspector_tracks_dirty_measurement_and_lifetime_without_exporting_passwords() {
    let host = IncrementalHost {
        batches: vec![],
        foreign_receipt: false,
        disposed: false,
    };
    let mut root = NativeRoot::new(host);
    let mut roots = vec![node(root.model().id()), node(root.model().id())];
    roots[1].descriptor.content = NodeContent::Input {
        max_length: None,
        value: "secret-do-not-export".into(),
        placeholder: "password".into(),
        enabled: true,
        password: true,
        revision: 0,
    };
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    let report = root.inspection(false);
    assert!(report["layout"]["measurements"].as_u64().unwrap() > 0);
    assert_eq!(report["resources"]["nodes"], 2);
    assert_eq!(report["last_commit"]["creates"], 2);
    assert!(!report.to_string().contains("secret-do-not-export"));
    assert!(!root
        .inspection(true)
        .to_string()
        .contains("secret-do-not-export"));
    roots[0].descriptor.content = NodeContent::Button {
        label: "new-label".into(),
        enabled: true,
    };
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    let report = root.inspection(true);
    assert_eq!(report["last_commit"]["updates"], 1);
    assert_eq!(
        report["last_commit"]["dirty_nodes"],
        serde_json::json!([roots[0].id.token()])
    );
    assert!(report.to_string().contains("new-label"));
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    assert!(root.inspection(false)["last_commit"]["dirty_nodes"]
        .as_array()
        .unwrap()
        .is_empty());
    root.dispose();
    let report = root.inspection(false);
    assert_eq!(report["disposed"], true);
    assert!(report["nodes"].as_array().unwrap().is_empty());
    assert_eq!(report["layout"]["cached_nodes"], 0);
}
