#![cfg(feature = "native-vm")]
use jadix_core::presentation::{
    component::ComponentSchema, EventPayload, NodeContent, PlatformEvent,
};
use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
fn registry() -> ComponentRegistry {
    let schemas: Vec<ComponentSchema> = serde_json::from_str(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap();
    let mut registry = ComponentRegistry::default();
    for schema in schemas.into_iter().chain(
        serde_json::from_str::<Vec<jadix_core::presentation::component::ComponentSchema>>(
            include_str!("../../jadix_components/schema/extended-v1.json"),
        )
        .unwrap(),
    ) {
        registry.register(schema).unwrap();
    }
    registry
}
#[test]
fn actual_sdk_tsx_bundle_roundtrips_host_events_and_keeps_keyed_identity() {
    let mut app = ScriptApplication::development(
        "tests.application",
        include_str!("fixtures/application.js"),
        registry(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(app.publish(|_| Ok(())).unwrap(), 1);
    let button = app.node_at(&[1]).unwrap();
    for count in 1..=20 {
        app.events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate,
        }])
        .unwrap();
        assert_eq!(app.publish(|_| Ok(())).unwrap(), 1);
        assert_eq!(app.node_at(&[1]), Some(button));
        assert_eq!(
            app.roots()[0].children[0].descriptor.content,
            NodeContent::Text(format!("Count: {count}"))
        );
    }
    let lifetime = app.lifetime();
    app.dispose();
    assert!(lifetime.is_disposed());
    assert!(app.publish(|_| panic!("disposed publisher")).is_err());
    assert!(app
        .events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate
        }])
        .is_err());
}
#[test]
fn failed_host_does_not_accept_bindings_and_failed_js_never_publishes() {
    let mut app = ScriptApplication::development(
        "tests.application",
        include_str!("fixtures/application.js"),
        registry(),
        Default::default(),
    )
    .unwrap();
    assert!(app.publish(|_| Err("publication failed".into())).is_err());
    assert!(app.roots().is_empty());
    assert!(app.node_at(&[1]).is_none());
    let source = format!(
        "{}\nthrow new Error('after mount')",
        include_str!("fixtures/application.js")
    );
    assert!(ScriptApplication::development(
        "tests.application",
        &source,
        registry(),
        Default::default()
    )
    .is_err());
}

#[test]
fn rejected_publication_retries_in_order_without_replaying_accepted_mounts() {
    let mut app = ScriptApplication::development(
        "tests.retry",
        include_str!("fixtures/application.js"),
        registry(),
        Default::default(),
    )
    .unwrap();
    assert!(app.publish_commits(|_| Err("busy".into())).is_err());
    assert!(app.roots().is_empty());
    let mut revisions = vec![];
    assert_eq!(
        app.publish_commits(|batch| {
            revisions.push((batch.previous, batch.revision));
            Ok(())
        })
        .unwrap(),
        1
    );
    let button = app.node_at(&[1]).unwrap();
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
            Ok(())
        })
        .is_err());
    assert_eq!(
        app.roots()[0].children[0].descriptor.content,
        NodeContent::Text("Count: 1".into())
    );
    assert_eq!(
        app.publish_commits(|batch| {
            revisions.push((batch.previous, batch.revision));
            Ok(())
        })
        .unwrap(),
        2
    );
    assert_eq!(revisions, [(0, 1), (1, 2), (2, 3), (3, 4)]);
    assert_eq!(app.node_at(&[1]), Some(button));
    assert_eq!(
        app.roots()[0].children[0].descriptor.content,
        NodeContent::Text("Count: 3".into())
    );
}

#[test]
fn unchanged_sdk_render_accepts_bindings_without_advancing_host_revision() {
    let (prefix, suffix) = include_str!("fixtures/application.js")
        .rsplit_once("  render();")
        .unwrap();
    let source = format!("{prefix}  render(); render();{suffix}");
    let mut app =
        ScriptApplication::development("tests.noop", &source, registry(), Default::default())
            .unwrap();
    let mut host_calls = 0;
    assert_eq!(
        app.publish_commits(|_| {
            host_calls += 1;
            Ok(())
        })
        .unwrap(),
        2
    );
    assert_eq!(host_calls, 1);
    app.events([PlatformEvent {
        node: app.node_at(&[1]).unwrap(),
        payload: EventPayload::Activate,
    }])
    .unwrap();
    app.publish_commits(|batch| {
        assert_eq!((batch.previous, batch.revision), (1, 2));
        Ok(())
    })
    .unwrap();
}

#[test]
fn sparse_leaf_packets_are_atomic_ordered_bounded_and_keep_event_bindings() {
    let source = format!(
        "globalThis.scaleRows=1024;globalThis.leafPatches=true;\n{}",
        include_str!("fixtures/scaling.js")
    );
    let mut app = ScriptApplication::development(
        "tests.sparse",
        &source,
        registry(),
        jadix_shell::runtime::ResourceLimits {
            turn_milliseconds: 500,
            memory_bytes: 128 * 1024 * 1024,
            ..Default::default()
        },
    )
    .unwrap();
    app.publish(|_| Ok(())).unwrap();
    let button = app.node_at(&[0]).unwrap();
    let before = app.work();
    for _ in 0..3 {
        app.events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate,
        }])
        .unwrap();
    }
    let mut seen = 0;
    assert!(app
        .publish(|_| {
            seen += 1;
            if seen == 2 {
                Err("busy".into())
            } else {
                Ok(())
            }
        })
        .is_err());
    assert_eq!(
        app.roots()[0].children[1].descriptor.content,
        NodeContent::Text("Row 0: 1".into())
    );
    assert_eq!(app.publish(|_| Ok(())).unwrap(), 2);
    assert_eq!(
        app.roots()[0].children[1].descriptor.content,
        NodeContent::Text("Row 0: 3".into())
    );
    assert_eq!(app.node_at(&[0]), Some(button));
    let work = app.work();
    assert_eq!(work.full_mounts, before.full_mounts);
    assert_eq!(work.leaf_patches - before.leaf_patches, 3);
    assert_eq!(work.event_lookups - before.event_lookups, 3);
    assert!(
        work.message_bytes - before.message_bytes < 1024,
        "no full tree crosses the bridge"
    );
    for patches in [
        r#"[{"path":[0],"props":{"text":"partial"}},{"path":[2],"props":{"text":"not an editor"}}]"#,
        r#"[{"path":[0],"props":{"text":"duplicate"}},{"path":[0],"props":{"text":"duplicate"}}]"#,
        r#"[{"path":[9999],"props":{"text":"missing"}}]"#,
        r#"[{"path":[0],"props":{"text":"wrong schema","unknown":true}}]"#,
    ] {
        let source=format!("{}\nglobalThis.__jadixDispatch=()=>globalThis.__jadixHost(JSON.stringify({{type:'patch',patches:{patches}}}));",include_str!("fixtures/application.js"));
        let mut app = ScriptApplication::development(
            "tests.bad-patch",
            &source,
            registry(),
            Default::default(),
        )
        .unwrap();
        app.publish(|_| Ok(())).unwrap();
        let old = app.roots().to_vec();
        app.events([PlatformEvent {
            node: app.node_at(&[1]).unwrap(),
            payload: EventPayload::Activate,
        }])
        .unwrap();
        assert!(app
            .publish(|_| panic!("invalid patch must not reach host"))
            .is_err());
        assert_eq!(app.roots(), old);
    }
}

#[test]
fn controlled_editor_leaf_packets_keep_full_mount_revision_preview_and_retry_contract() {
    fn app(partial: bool) -> ScriptApplication {
        ScriptApplication::development(
            "tests.editorleaves",
            &format!(
                "globalThis.leafPatches={partial};\n{}",
                include_str!("fixtures/controlled-leaves.js")
            ),
            registry(),
            Default::default(),
        )
        .unwrap()
    }
    let mut apps = [app(false), app(true)];
    for a in &mut apps {
        a.publish(|_| Ok(())).unwrap();
        assert!(matches!(&a.roots()[0].children[0].descriptor.content,
            NodeContent::Input { value, revision: 0, .. } if value == "draft"));
        assert!(matches!(&a.roots()[0].children[1].descriptor.content,
            NodeContent::Textarea { value, revision: 0, .. } if value == "notes"));
    }
    for (path, value, revision, composing) in [
        (0, "中文 🦀 draft", 1, None),
        (0, "preview 中文", 2, Some([0, 7])),
        (0, "preview 中文", 3, None),
        (1, "notes\n中文 🦀", 1, None),
    ] {
        for a in &mut apps {
            let id = a.node_at(&[path]).unwrap();
            a.events([PlatformEvent {
                node: id,
                payload: EventPayload::Edit {
                    value: value.into(),
                    revision,
                    selection_utf16: [0, 0],
                    composing_utf16: composing,
                },
            }])
            .unwrap();
            let before = a.roots().to_vec();
            assert!(a.publish(|_| Err("busy".into())).is_err());
            assert_eq!(a.roots(), before);
            assert_eq!(a.publish(|_| Ok(())).unwrap(), 1);
            assert_eq!(a.node_at(&[path]), Some(id));
        }
        for i in [0, 1] {
            assert_eq!(
                apps[0].roots()[0].children[i].descriptor,
                apps[1].roots()[0].children[i].descriptor
            );
        }
    }
    for a in &mut apps {
        a.events([PlatformEvent {
            node: a.node_at(&[2]).unwrap(),
            payload: EventPayload::Activate,
        }])
        .unwrap();
        a.publish(|_| Ok(())).unwrap();
    }
    assert_eq!(
        apps[0].roots()[0].children[0].descriptor,
        apps[1].roots()[0].children[0].descriptor
    );
    assert!(
        matches!(&apps[1].roots()[0].children[0].descriptor.content, NodeContent::Input { value,revision:4,.. } if value=="SERVER 中文 🦀")
    );
    assert_eq!(apps[1].work().full_mounts, 1);
    assert_eq!(apps[1].work().leaf_patches, 5);
    assert_eq!(apps[0].work().full_mounts, 6);
}

#[test]
fn controlled_leaf_invalid_second_record_or_schema_never_partially_publishes() {
    for patches in [
        r#"[{"path":[0],"props":{"value":"first"}},{"path":[1],"props":{"value":"second","maxLength":-1}}]"#,
        r#"[{"path":[0],"props":{"value":"first"}},{"path":[1],"props":{"value":"second","maxLength":1.5}}]"#,
        r#"[{"path":[0],"props":{"value":"first","style":{}}}]"#,
        r#"[{"path":[0],"props":{"value":"first"}},{"path":[0],"props":{"value":"duplicate"}}]"#,
    ] {
        let source=format!("{}\nglobalThis.__jadixDispatch=()=>globalThis.__jadixHost(JSON.stringify({{type:'patch',patches:{patches}}}));",include_str!("fixtures/controlled-leaves.js"));
        let mut app = ScriptApplication::development(
            "tests.badeditorleaf",
            &source,
            registry(),
            Default::default(),
        )
        .unwrap();
        app.publish(|_| Ok(())).unwrap();
        let before = app.roots().to_vec();
        app.events([PlatformEvent {
            node: app.node_at(&[2]).unwrap(),
            payload: EventPayload::Activate,
        }])
        .unwrap();
        assert!(app
            .publish(|_| panic!("invalid editor patch reached host"))
            .is_err());
        assert_eq!(app.roots(), before);
        assert_eq!(app.work().full_mounts, 1);
        assert_eq!(app.work().leaf_patches, 0);
    }
}

/// Run explicitly: isolate SDK construction from the actual shared host admission.
#[test]
#[ignore = "startup budget diagnosis; no concurrent builds"]
fn sdk_startup_phases() {
    let fixture = include_str!("fixtures/counter-leaves.js");
    assert_eq!(fixture.matches("  app.mount(").count(), 1);
    let mut records = vec![];
    for rows in [0, 256, 1024, 3072] {
        for build_only in [true, false] {
            for turn_milliseconds in [20, 1000] {
                for round in 0..3 {
                    let mut source = format!("globalThis.businessRows={rows};\n{fixture}");
                    if build_only {
                        source = source.replace("  app.mount(", "  globalThis.built = (");
                    }
                    let registry = registry();
                    let started = std::time::Instant::now();
                    let result = ScriptApplication::development(
                        "tests.startup.phases",
                        &source,
                        registry,
                        jadix_shell::runtime::ResourceLimits {
                            load_milliseconds: turn_milliseconds,
                            turn_milliseconds,
                            ..Default::default()
                        },
                    );
                    let nanos = started.elapsed().as_nanos() as u64;
                    let (ok, error, mounts) = match result {
                        Ok(mut app) => {
                            let mounts = app.publish(|_| Ok(())).unwrap();
                            if !build_only {
                                assert_eq!(app.roots()[0].children.len(), rows + 3);
                            }
                            app.dispose();
                            (true, None, mounts)
                        }
                        Err(e) => (false, Some(e.to_string()), 0),
                    };
                    records.push(serde_json::json!({"rows":rows,"build_only":build_only,
                        "turn_ms":turn_milliseconds,"round":round,"ok":ok,"error":error,"mounts":mounts,"load_ns":nanos}));
                }
            }
        }
    }
    if let Ok(path) = std::env::var("JADIX_STARTUP_REPORT") {
        std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    }
    assert_eq!(records.len(), 48);
}

#[test]
fn startup_and_interaction_budgets_are_independent() {
    let source = format!(
        r#"{}
        const startupBegin = Date.now();
        while (Date.now() - startupBegin < 40) {{}}
        const dispatch = globalThis.__jadixDispatch;
        globalThis.__jadixDispatch = (key, value) => {{
            const begin = Date.now(); while (Date.now() - begin < 40) {{}}
            dispatch(key, value);
        }};"#,
        include_str!("fixtures/application.js")
    );
    let mut app = ScriptApplication::development(
        "tests.startup.budgets",
        &source,
        registry(),
        jadix_shell::runtime::ResourceLimits {
            turn_milliseconds: 5,
            ..Default::default()
        },
    )
    .expect("bounded startup has its own budget; interactions retain 5 ms");
    assert_eq!(app.publish(|_| Ok(())).unwrap(), 1);
    let before = app.roots().to_vec();
    let button = app.node_at(&[1]).unwrap();
    assert!(matches!(
        app.events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate
        }]),
        Err(jadix_shell::application::ApplicationError::Vm(
            jadix_shell::runtime::native::VmError::Timeout
        ))
    ));
    assert_eq!(app.roots(), before);
    assert!(app
        .publish(|_| panic!("faulted event must not publish"))
        .is_err());
    app.dispose();
}

#[test]
fn startup_source_and_initial_jobs_keep_independent_bounds() {
    use jadix_shell::{
        application::ApplicationError,
        runtime::{native::VmError, ResourceLimits},
    };
    assert!(matches!(
        ScriptApplication::development(
            "tests.startup.infinite",
            "while(true){}",
            registry(),
            ResourceLimits {
                load_milliseconds: 1,
                ..Default::default()
            }
        ),
        Err(ApplicationError::Vm(VmError::Timeout))
    ));
    assert!(matches!(
        ScriptApplication::development(
            "tests.startup.jobs",
            "function again(){Promise.resolve().then(again)} again();",
            registry(),
            ResourceLimits {
                jobs_per_turn: 8,
                ..Default::default()
            }
        ),
        Err(ApplicationError::Vm(VmError::JobLimit))
    ));
    assert!(matches!(
        ScriptApplication::development(
            "tests.startup.slowjob",
            "Promise.resolve().then(()=>{const b=Date.now();while(Date.now()-b<40){}})",
            registry(),
            ResourceLimits {
                turn_milliseconds: 5,
                ..Default::default()
            }
        ),
        Err(ApplicationError::Vm(VmError::Timeout))
    ));
}

#[test]
fn default_startup_mounts_large_sdk_tree_and_keeps_sparse_event_updates() {
    let source = format!(
        "globalThis.businessRows=3072;\n{}",
        include_str!("fixtures/counter-leaves.js")
    );
    let mut app = ScriptApplication::development(
        "tests.startup.large",
        &source,
        registry(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(app.publish(|_| Ok(())).unwrap(), 1);
    assert_eq!(app.roots()[0].children.len(), 3075);
    for n in 0..3072 {
        assert_eq!(
            app.roots()[0].children[n + 3].descriptor.content,
            NodeContent::Text(format!("Row {n}: business"))
        );
    }
    let button = app.node_at(&[1]).unwrap();
    for count in 1..=12 {
        app.events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate,
        }])
        .unwrap();
        assert_eq!(app.publish(|_| Ok(())).unwrap(), 1);
        assert_eq!(
            app.roots()[0].children[0].descriptor.content,
            NodeContent::Text(format!("Counter {count}"))
        );
        assert_eq!(app.node_at(&[1]), Some(button));
    }
    assert_eq!(app.work().full_mounts, 1);
    assert_eq!(app.work().leaf_patches, 12);
    app.dispose();
}

/// Both SDK variants execute in the same runtime and under the same budgets.
#[test]
#[ignore = "controlled startup ABBAAB measurement; requires frozen reference bundle"]
fn sdk_validation_same_budget_abba() {
    let reference = std::fs::read_to_string(std::env::var("JADIX_SDK_REFERENCE").unwrap()).unwrap();
    let candidate = include_str!("fixtures/counter-leaves.js");
    let mut records = vec![];
    for rows in [0, 256, 1024, 3072] {
        let mut expected = None;
        for (index, kind) in "ABBAAB".chars().enumerate() {
            let source = format!(
                "globalThis.businessRows={rows};\n{}",
                if kind == 'A' { &reference } else { candidate }
            );
            let registry = registry();
            let started = std::time::Instant::now();
            let mut app = ScriptApplication::development(
                "tests.sdk.abba",
                &source,
                registry,
                jadix_shell::runtime::ResourceLimits {
                    load_milliseconds: 1000,
                    turn_milliseconds: 20,
                    ..Default::default()
                },
            )
            .unwrap();
            let load_ns = started.elapsed().as_nanos() as u64;
            assert_eq!(app.publish(|_| Ok(())).unwrap(), 1);
            let root = &app.roots()[0];
            assert_eq!(root.children.len(), rows + 3);
            let descriptors: Vec<_> = std::iter::once(&root.descriptor)
                .chain(root.children.iter().map(|n| &n.descriptor))
                .cloned()
                .collect();
            if let Some(expected) = &expected {
                assert_eq!(&descriptors, expected);
            } else {
                expected = Some(descriptors);
            }
            for n in 0..rows {
                assert_eq!(
                    root.children[n + 3].descriptor.content,
                    NodeContent::Text(format!("Row {n}: business"))
                );
            }
            let content: Vec<_> = root
                .children
                .iter()
                .map(|n| format!("{:?}", n.descriptor.content))
                .collect();
            records.push(serde_json::json!({"rows":rows,"index":index,"kind":kind.to_string(),"load_ns":load_ns,
                "content":content,"descriptors_equal":true,"nodes":rows+4,"work":app.work()}));
            app.dispose();
        }
    }
    assert_eq!(records.len(), 24);
    std::fs::write(
        std::env::var("JADIX_SDK_ABBA_REPORT").unwrap(),
        serde_json::to_vec(&records).unwrap(),
    )
    .unwrap();
}
