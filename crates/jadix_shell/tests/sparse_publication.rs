#![cfg(feature = "native-vm")]
//! Count actual allocator traffic inside publication, excluding VM startup/events.
use jadix_core::presentation::{
    component::ComponentSchema, EventPayload, NodeContent, Operation, PlatformEvent,
};
use jadix_shell::{
    application::ScriptApplication, runtime::ResourceLimits, wire::ComponentRegistry,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

struct CountingAllocator;
thread_local! { static BYTES: Cell<Option<usize>> = const { Cell::new(None) }; }
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        BYTES.with(|n| {
            if let Some(bytes) = n.get() {
                n.set(Some(bytes + layout.size()));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        BYTES.with(|n| {
            if let Some(bytes) = n.get() {
                n.set(Some(bytes + size));
            }
        });
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
fn measured<T>(run: impl FnOnce() -> T) -> (T, usize) {
    BYTES.with(|n| n.set(Some(0)));
    let result = run();
    let bytes = BYTES.with(|n| n.replace(None).unwrap());
    (result, bytes)
}
fn app(rows: usize) -> ScriptApplication {
    app_bundle(rows, include_str!("fixtures/scaling.js"))
}
fn app_bundle(rows: usize, bundle: &str) -> ScriptApplication {
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap()
    {
        registry.register(schema).unwrap();
    }
    ScriptApplication::development(
        "tests.sparse-publication",
        &format!(
            "globalThis.scaleRows={rows};globalThis.leafPatches=true;\n{}",
            bundle
        ),
        registry,
        ResourceLimits {
            turn_milliseconds: 1000,
            memory_bytes: 128 * 1024 * 1024,
            ..Default::default()
        },
    )
    .unwrap()
}
#[test]
fn structural_leaf_publication_allocations_are_independent_of_sibling_count() {
    let mut costs = vec![];
    for rows in [32, 256, 1024, 3072] {
        let mut app = app(rows);
        app.publish_commits(|_| Ok(())).unwrap();
        let (idle, idle_bytes) =
            measured(|| app.publish_commits(|_| panic!("idle must not publish")));
        assert_eq!(idle.unwrap(), 0);
        app.events([PlatformEvent {
            node: app.node_at(&[0]).unwrap(),
            payload: EventPayload::Activate,
        }])
        .unwrap();
        let mut operations = vec![];
        let (result, patch_bytes) = measured(|| {
            app.publish_commits(|batch| {
                operations.extend(batch.operations.clone());
                Ok(())
            })
        });
        assert_eq!(result.unwrap(), 1);
        assert_eq!(operations.len(), 1);
        assert!(
            matches!(&operations[0], Operation::Update { descriptor, .. } if descriptor.content == NodeContent::Text("Row 0: 1".into()))
        );
        println!("rows={rows} idle_bytes={idle_bytes} patch_bytes={patch_bytes}");
        costs.push((idle_bytes, patch_bytes));
    }
    assert!(
        costs.iter().all(|(idle, _)| *idle == 0),
        "idle publication cloned a complete snapshot: {costs:?}"
    );
    assert!(
        costs
            .iter()
            .all(|(_, bytes)| *bytes <= costs[0].1 * 2 + 8192),
        "leaf publication cost grows with untouched siblings: {costs:?}"
    );
}

#[test]
fn sdk_to_native_leaf_commit_does_not_allocate_for_untouched_siblings() {
    let mut costs = vec![];
    for rows in [32, 256, 1024, 3072] {
        let mut app = app(rows);
        let mut host = jadix_native::HeadlessRoot::with_lifetime(
            app.lifetime(),
            jadix_native::presentation::capabilities::BackendCapabilities::headless(),
        );
        app.publish_commits(|batch| {
            host.commit(batch.clone())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        let changed = app.node_at(&[1]).unwrap();
        app.events([PlatformEvent {
            node: app.node_at(&[0]).unwrap(),
            payload: EventPayload::Activate,
        }])
        .unwrap();
        let (result, bytes) = measured(|| {
            app.publish_commits(|batch| {
                host.commit(batch.clone())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
        });
        assert_eq!(result.unwrap(), 1);
        assert_eq!(
            host.node(changed).unwrap().descriptor.content,
            NodeContent::Text("Row 0: 1".into())
        );
        println!("native rows={rows} bytes={bytes}");
        costs.push(bytes);
    }
    assert!(
        costs.iter().all(|bytes| *bytes <= costs[0] * 2 + 8192),
        "native staging still visits untouched siblings: {costs:?}"
    );
}

#[test]
fn sparse_commit_rejection_restores_only_pending_leaves_and_keeps_revisions() {
    let mut app = app(32);
    let mut host = jadix_native::HeadlessRoot::with_lifetime(
        app.lifetime(),
        jadix_native::presentation::capabilities::BackendCapabilities::headless(),
    );
    let mut revisions = vec![];
    app.publish_commits(|batch| {
        revisions.push((batch.previous, batch.revision));
        host.commit(batch.clone())
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .unwrap();
    let button = app.node_at(&[0]).unwrap();
    let text = app.node_at(&[1]).unwrap();
    for _ in 0..3 {
        app.events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate,
        }])
        .unwrap();
    }
    let mut attempts = 0;
    assert!(app
        .publish_commits(|batch| {
            attempts += 1;
            if attempts == 2 {
                return Err("busy".into());
            }
            revisions.push((batch.previous, batch.revision));
            host.commit(batch.clone())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .is_err());
    assert_eq!(
        app.roots()[0].children[1].descriptor.content,
        NodeContent::Text("Row 0: 1".into())
    );
    assert_eq!(
        host.node(text).unwrap().descriptor.content,
        app.roots()[0].children[1].descriptor.content
    );
    assert_eq!(
        app.publish_commits(|batch| {
            revisions.push((batch.previous, batch.revision));
            host.commit(batch.clone())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap(),
        2
    );
    assert_eq!(revisions, [(0, 1), (1, 2), (2, 3), (3, 4)]);
    assert_eq!(app.node_at(&[0]), Some(button));
    assert_eq!(
        host.node(text).unwrap().descriptor.content,
        NodeContent::Text("Row 0: 3".into())
    );
}

#[test]
fn unchanged_sparse_packets_accept_without_consuming_host_revisions() {
    let bundle = include_str!("fixtures/scaling.js").replace("count++;", "count = 1;");
    let mut app = app_bundle(32, &bundle);
    app.publish_commits(|_| Ok(())).unwrap();
    for _ in 0..3 {
        app.events([PlatformEvent {
            node: app.node_at(&[0]).unwrap(),
            payload: EventPayload::Activate,
        }])
        .unwrap();
    }
    let mut revisions = vec![];
    assert_eq!(
        app.publish_commits(|batch| {
            revisions.push((batch.previous, batch.revision));
            Ok(())
        })
        .unwrap(),
        3
    );
    assert_eq!(revisions, [(1, 2)]);
    assert_eq!(app.work().leaf_patches, 3);
}

#[test]
fn complete_protocol_roundtrip_matches_full_mount_output_with_bounded_sparse_allocation() {
    let mut sparse_costs = vec![];
    for rows in [32, 256, 1024, 3072] {
        let mut sparse = app(rows);
        let dense_bundle = include_str!("fixtures/scaling.js")
            .replace("if (globalThis.leafPatches)", "if (false)");
        let mut dense = app_bundle(rows, &dense_bundle);
        let mut sparse_host = jadix_native::HeadlessRoot::with_lifetime(
            sparse.lifetime(),
            jadix_native::presentation::capabilities::BackendCapabilities::headless(),
        );
        let mut dense_host = jadix_native::HeadlessRoot::with_lifetime(
            dense.lifetime(),
            jadix_native::presentation::capabilities::BackendCapabilities::headless(),
        );
        sparse
            .publish_commits(|batch| {
                sparse_host
                    .commit(batch.clone())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        dense
            .publish_commits(|batch| {
                dense_host
                    .commit(batch.clone())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        let mut bytes = [vec![], vec![]];
        for round in 0..6 {
            // Alternate order so the same path never always benefits from warmup.
            for index in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let (app, host) = if index == 0 {
                    (&mut sparse, &mut sparse_host)
                } else {
                    (&mut dense, &mut dense_host)
                };
                let event = PlatformEvent {
                    node: app.node_at(&[0]).unwrap(),
                    payload: EventPayload::Activate,
                };
                let start = std::time::Instant::now();
                let (result, allocated) = measured(|| {
                    host.accept_event(&event).unwrap();
                    app.events([event]).unwrap();
                    app.publish_commits(|batch| {
                        host.commit(batch.clone())
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    })
                });
                assert_eq!(result.unwrap(), 1);
                println!(
                    "roundtrip rows={rows} round={round} sparse={} bytes={allocated} nanos={}",
                    index == 0,
                    start.elapsed().as_nanos()
                );
                bytes[index].push(allocated);
            }
            // Compare every actual host descriptor outside the measured region.
            for path in std::iter::once(vec![]).chain((0..=rows).map(|i| vec![i])) {
                assert_eq!(
                    sparse_host
                        .node(sparse.node_at(&path).unwrap())
                        .unwrap()
                        .descriptor,
                    dense_host
                        .node(dense.node_at(&path).unwrap())
                        .unwrap()
                        .descriptor
                );
            }
        }
        let maximum = *bytes[0].iter().max().unwrap();
        assert!(
            *bytes[1].iter().min().unwrap() > maximum,
            "full mount must cost more for identical output"
        );
        sparse_costs.push(maximum);
    }
    assert!(
        sparse_costs
            .iter()
            .all(|bytes| *bytes < sparse_costs[0] * 2 + 8192),
        "protocol roundtrip allocation scales with siblings: {sparse_costs:?}"
    );
}
