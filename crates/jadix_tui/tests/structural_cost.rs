//! Complete host publication/layout/Buffer paint costs, including remaining N work.
use jadix_core::presentation::{style::NodeStyle, *};
use jadix_tui::{
    ratatui::{buffer::Buffer, layout::Rect as Area, widgets::Widget},
    CellSurface,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
struct CountingAllocator;
thread_local! {
    static BYTES: Cell<Option<usize>> = const { Cell::new(None) };
    static DESCRIPTIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        BYTES.with(|n| {
            if let Some(bytes) = n.get() {
                n.set(Some(bytes + layout.size()));
                if (2200..2240).contains(&layout.size()) {
                    DESCRIPTIONS.with(|v| v.set(v.get() + layout.size()));
                }
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
                if (2200..2240).contains(&size) {
                    DESCRIPTIONS.with(|v| v.set(v.get() + size));
                }
            }
        });
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
fn measured<T>(run: impl FnOnce() -> T) -> (T, usize, usize) {
    BYTES.with(|n| n.set(Some(0)));
    DESCRIPTIONS.with(|n| n.set(0));
    let result = run();
    let bytes = BYTES.with(|n| n.replace(None).unwrap());
    (result, bytes, DESCRIPTIONS.with(Cell::get))
}

fn descriptor(content: NodeContent) -> NodeDescriptor {
    NodeDescriptor {
        content,
        frame: Rect::default(),
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
        style: NodeStyle {
            width: Some(320.),
            height: Some(20.),
            shrink: 0.,
            ..Default::default()
        },
    }
}
#[test]
fn actual_structural_host_costs_and_full_snapshot_costs_produce_equal_cells() {
    for rows in [32, 256, 1024, 3072] {
        let life = RootLifetime::new(RootId::new());
        let root = NodeId::new(life.id());
        let mut container = descriptor(NodeContent::Container);
        container.style.height = None;
        let mut operations = vec![Operation::Create {
            node: root,
            parent: None,
            index: 0,
            descriptor: container.clone(),
        }];
        let mut forest = vec![PresentationNode {
            id: root,
            descriptor: container,
            children: vec![],
        }];
        for index in 0..rows {
            let id = NodeId::new(life.id());
            let mut d = descriptor(NodeContent::Text(format!("Record {index}: 0")));
            d.accessible_name = Some(format!(
                "Record {index}: {}",
                "Business description. ".repeat(100)
            ));
            operations.push(Operation::Create {
                node: id,
                parent: Some(root),
                index,
                descriptor: d.clone(),
            });
            forest[0].children.push(PresentationNode {
                id,
                descriptor: d,
                children: vec![],
            });
        }
        let mut structural = CellSurface::new(life.clone(), [8., 16.]).unwrap();
        let mut snapshot = CellSurface::new(life.clone(), [8., 16.]).unwrap();
        let area = Area::new(0, 0, 40, 12);
        structural
            .commit_auto(
                &CommitBatch {
                    root: life.id(),
                    previous: 0,
                    revision: 1,
                    operations,
                },
                area,
                0,
            )
            .unwrap();
        snapshot.present_auto(0, forest.clone(), area, 0).unwrap();
        for round in 0..6 {
            forest[0].children[0].descriptor.content =
                NodeContent::Text(format!("Record 0: {}", round + 1));
            let batch = CommitBatch {
                root: life.id(),
                previous: structural.revision(),
                revision: structural.revision() + 1,
                operations: vec![Operation::Update {
                    node: forest[0].children[0].id,
                    descriptor: forest[0].children[0].descriptor.clone(),
                }],
            };
            let mut images = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let start = std::time::Instant::now();
                let (buffer, bytes, copied_payload) = measured(|| {
                    let host = if mode == 0 {
                        &mut structural
                    } else {
                        &mut snapshot
                    };
                    if mode == 0 {
                        let stats = host.commit_auto(&batch, area, 0).unwrap();
                        assert_eq!(stats.sync_nodes, 1);
                    } else {
                        host.present_auto(host.revision(), forest.clone(), area, 0)
                            .unwrap();
                    }
                    let mut buffer = Buffer::empty(area);
                    (&mut *host).render(area, &mut buffer);
                    buffer
                });
                println!("host rows={rows} round={round} structural={} allocated={bytes} copied_payload={copied_payload} nanos={}",mode==0,start.elapsed().as_nanos());
                let visible: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
                assert!(visible.contains(&format!("Record 0: {}", round + 1)));
                if mode == 0 {
                    assert!(copied_payload <= 16 * 2240,
                        "a second business-description forest was copied: {copied_payload} vs fixed 35840");
                }
                images[mode] = Some(buffer);
            }
            assert_eq!(images[0], images[1], "rows={rows} round={round}");
        }
    }
}

#[cfg(feature = "shell")]
#[test]
fn actual_sdk_mouse_roundtrips_and_full_mounts_produce_equal_cells_across_tree_sizes() {
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    use jadix_tui::{
        crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
        ShellApplication,
    };
    fn application(rows: usize, sparse: bool) -> ShellApplication {
        let mut registry = ComponentRegistry::default();
        for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../jadix_components/schema/portable-v1.json"
        ))
        .unwrap()
        {
            registry.register(schema).unwrap();
        }
        let source = format!(
            "globalThis.scaleRows={rows};globalThis.leafPatches={sparse};globalThis.searchOffsets=true;\n{}",
            include_str!("../../jadix_shell/tests/fixtures/scaling.js")
        );
        ShellApplication::new(
            ScriptApplication::development(
                "tests.tui.roundtrip",
                &source,
                registry,
                jadix_shell::runtime::ResourceLimits {
                    turn_milliseconds: 1000,
                    memory_bytes: 128 * 1024 * 1024,
                    ..Default::default()
                },
            )
            .unwrap(),
            [8., 16.],
        )
        .unwrap()
    }
    for rows in [32, 256, 1024, 3072] {
        let area = Area::new(0, 0, 40, 12);
        let mut apps = [application(rows, true), application(rows, false)];
        for app in &mut apps {
            app.publish(area, 0).unwrap();
            let mut buffer = Buffer::empty(area);
            app.surface_mut().render(area, &mut buffer);
            assert_eq!(app.surface_mut().find("Row").unwrap().matches, rows);
        }
        for round in 0..6 {
            let mut images = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let app = &mut apps[mode];
                let start = std::time::Instant::now();
                let (buffer, bytes, _) = measured(|| {
                    for kind in [
                        MouseEventKind::Down(MouseButton::Left),
                        MouseEventKind::Up(MouseButton::Left),
                    ] {
                        app.handle(Event::Mouse(MouseEvent {
                            kind,
                            column: 2,
                            row: 0,
                            modifiers: KeyModifiers::NONE,
                        }))
                        .unwrap();
                    }
                    assert_eq!(app.publish(area, 0).unwrap(), 1);
                    let mut buffer = Buffer::empty(area);
                    app.surface_mut().render(area, &mut buffer);
                    buffer
                });
                println!(
                    "sdk rows={rows} round={round} sparse={} dense_search=true allocated={bytes} nanos={}",
                    mode == 0,
                    start.elapsed().as_nanos()
                );
                let visible: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
                assert!(
                    visible.contains(&format!("Row 0: {}", round + 1)),
                    "rows={rows} round={round} sparse={}",
                    mode == 0
                );
                if mode == 0 {
                    assert!(bytes<131072,"sparse real mouse/SDK/layout/render roundtrip still scales with the tree: rows={rows} bytes={bytes}");
                }
                images[mode] = Some(buffer);
            }
            assert_eq!(images[0], images[1], "rows={rows} round={round}");
        }
        assert_eq!(apps[0].application().work().full_mounts, 1);
        assert_eq!(apps[0].application().work().leaf_patches, 6);
        assert_eq!(apps[1].application().work().full_mounts, 7);
    }
}

#[test]
fn changed_row_heights_do_not_copy_unchanged_business_descriptions() {
    let mut excess = vec![];
    for rows in [32, 256, 1024, 3072] {
        let life = RootLifetime::new(RootId::new());
        let root = NodeId::new(life.id());
        let mut container = descriptor(NodeContent::Container);
        container.style.height = None;
        let mut forest = vec![PresentationNode {
            id: root,
            descriptor: container.clone(),
            children: vec![],
        }];
        let mut operations = vec![Operation::Create {
            node: root,
            parent: None,
            index: 0,
            descriptor: container,
        }];
        for index in 0..rows {
            let id = NodeId::new(life.id());
            let mut d = descriptor(NodeContent::Text(format!("Record {index}: 0")));
            d.style.height = None;
            d.style.text_overflow = style::TextOverflow::Wrap;
            d.accessible_name = Some(format!(
                "Record {index}: {}",
                "Business description. ".repeat(100)
            ));
            operations.push(Operation::Create {
                node: id,
                parent: Some(root),
                index,
                descriptor: d.clone(),
            });
            forest[0].children.push(PresentationNode {
                id,
                descriptor: d,
                children: vec![],
            });
        }
        let mut structural = CellSurface::new(life.clone(), [8., 16.]).unwrap();
        let mut full = CellSurface::new(life.clone(), [8., 16.]).unwrap();
        let area = Area::new(3, 2, 40, 12);
        structural
            .commit_auto(
                &CommitBatch {
                    root: life.id(),
                    previous: 0,
                    revision: 1,
                    operations,
                },
                area,
                0,
            )
            .unwrap();
        full.present_auto(0, forest.clone(), area, 0).unwrap();
        structural.find("Record").unwrap();
        full.find("Record").unwrap();
        let first = forest[0].children[0].id;
        let second = forest[0].children[1].id;
        let initial_height = structural.frame(first).unwrap().height;
        for round in 0..6 {
            let text = if round % 2 == 0 {
                format!("Record 0: {}\nsecond 中文 🦀\nthird line", round + 1)
            } else {
                format!("Record 0: {}", round + 1)
            };
            forest[0].children[0].descriptor.content = NodeContent::Text(text);
            let update = CommitBatch {
                root: life.id(),
                previous: structural.revision(),
                revision: structural.revision() + 1,
                operations: vec![Operation::Update {
                    node: first,
                    descriptor: forest[0].children[0].descriptor.clone(),
                }],
            };
            let mut images = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let host = if mode == 0 {
                    &mut structural
                } else {
                    &mut full
                };
                let started = std::time::Instant::now();
                let ((buffer, stats), bytes, copied) = measured(|| {
                    let stats = if mode == 0 {
                        host.commit_auto(&update, area, 0).unwrap()
                    } else {
                        host.present_auto(host.revision(), forest.clone(), area, 0)
                            .unwrap()
                    };
                    let mut b = Buffer::empty(area);
                    host.render(area, &mut b);
                    (b, stats)
                });
                println!("geometry rows={rows} round={round} structural={} allocated={bytes} copied_payload={copied} geometry_reused={} nanos={}",mode==0,stats.geometry_reused,started.elapsed().as_nanos());
                assert!(
                    !stats.geometry_reused,
                    "height change must exercise actual geometry recomputation"
                );
                let frame = host.frame(first).unwrap();
                let next = host.frame(second).unwrap();
                assert_eq!(
                    frame.height,
                    initial_height * if round % 2 == 0 { 3. } else { 1. }
                );
                assert_eq!(next.y, frame.y + frame.height);
                assert_eq!(host.search_summary().matches, rows);
                if mode == 0 && copied > 16 * 2240 {
                    excess.push((rows, round, copied));
                }
                images[mode] = Some(buffer);
            }
            assert_eq!(images[0], images[1], "rows={rows} round={round}");
            assert_eq!(structural.search_matches(), full.search_matches());
        }
    }
    assert!(
        excess.is_empty(),
        "height update cloned unchanged business metadata: {excess:?}"
    );
}

#[cfg(feature = "shell")]
#[test]
fn actual_sdk_height_changes_match_full_mount_geometry_search_and_cells() {
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    use jadix_tui::{
        crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
        ShellApplication,
    };
    fn app(rows: usize, sparse: bool) -> ShellApplication {
        let mut registry = ComponentRegistry::default();
        for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../jadix_components/schema/portable-v1.json"
        ))
        .unwrap()
        {
            registry.register(schema).unwrap();
        }
        let source=format!("globalThis.scaleRows={rows};globalThis.leafPatches={sparse};globalThis.growingRows=true;\n{}",include_str!("../../jadix_shell/tests/fixtures/scaling.js"));
        ShellApplication::new(
            ScriptApplication::development(
                "tests.tui.geometry",
                &source,
                registry,
                jadix_shell::runtime::ResourceLimits {
                    turn_milliseconds: 1000,
                    memory_bytes: 128 * 1024 * 1024,
                    ..Default::default()
                },
            )
            .unwrap(),
            [8., 16.],
        )
        .unwrap()
    }
    for rows in [32, 256, 1024, 3072] {
        let area = Area::new(0, 0, 40, 12);
        let mut apps = [app(rows, true), app(rows, false)];
        for a in &mut apps {
            a.publish(area, 0).unwrap();
            a.surface_mut().find("Row").unwrap();
        }
        for round in 0..6 {
            let mut images = [None, None];
            let mut frames = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let a = &mut apps[mode];
                let start = std::time::Instant::now();
                let (image, bytes, copied) = measured(|| {
                    for kind in [
                        MouseEventKind::Down(MouseButton::Left),
                        MouseEventKind::Up(MouseButton::Left),
                    ] {
                        a.handle(Event::Mouse(MouseEvent {
                            kind,
                            column: 2,
                            row: 0,
                            modifiers: KeyModifiers::NONE,
                        }))
                        .unwrap();
                    }
                    assert_eq!(a.publish(area, 0).unwrap(), 1);
                    let mut b = Buffer::empty(area);
                    a.surface_mut().render(area, &mut b);
                    b
                });
                println!("sdkgeometry rows={rows} round={round} sparse={} allocated={bytes} copied_payload={copied} nanos={}",mode==0,start.elapsed().as_nanos());
                assert!(!a.layout_stats().geometry_reused);
                assert_eq!(a.surface().search_summary().matches, rows);
                let first = a.application().node_at(&[1]).unwrap();
                let second = a.application().node_at(&[2]).unwrap();
                let f = a.surface().frame(first).unwrap();
                let next = a.surface().frame(second).unwrap();
                assert_eq!(f.height, if round % 2 == 0 { 48. } else { 16. });
                assert_eq!(next.y, f.y + f.height);
                frames[mode] = Some((f, next));
                let visible: String = image.content.iter().map(|c| c.symbol()).collect();
                assert!(visible.contains(&format!("Row 0: {}", round + 1)));
                assert_eq!(visible.contains("third line"), round % 2 == 0);
                images[mode] = Some(image);
            }
            assert_eq!(images[0], images[1]);
            assert_eq!(frames[0], frames[1]);
        }
        assert_eq!(apps[0].application().work().full_mounts, 1);
        assert_eq!(apps[0].application().work().leaf_patches, 6);
        assert_eq!(apps[1].application().work().full_mounts, 7);
    }
}

#[test]
fn viewport_resize_borrows_large_accepted_descriptors_and_matches_full_publication() {
    for rows in [32, 256, 1024, 3072] {
        let life = RootLifetime::new(RootId::new());
        let root = NodeId::new(life.id());
        let mut parent = descriptor(NodeContent::Container);
        parent.style.height = None;
        parent.style.width = None;
        let mut forest = vec![PresentationNode {
            id: root,
            descriptor: parent.clone(),
            children: vec![],
        }];
        let mut operations = vec![Operation::Create {
            node: root,
            parent: None,
            index: 0,
            descriptor: parent,
        }];
        for index in 0..rows {
            let id = NodeId::new(life.id());
            let mut d = descriptor(NodeContent::Text(format!(
                "Record {index}: wrap 中文 words"
            )));
            d.style.width = None;
            d.style.height = None;
            d.style.text_overflow = jadix_core::presentation::style::TextOverflow::Wrap;
            d.accessible_name = Some("Business description. ".repeat(100));
            operations.push(Operation::Create {
                node: id,
                parent: Some(root),
                index,
                descriptor: d.clone(),
            });
            forest[0].children.push(PresentationNode {
                id,
                descriptor: d,
                children: vec![],
            });
        }
        let mut warm = CellSurface::new(life.clone(), [8., 16.]).unwrap();
        let mut full = CellSurface::new(life.clone(), [8., 16.]).unwrap();
        warm.commit_auto(
            &CommitBatch {
                root: life.id(),
                previous: 0,
                revision: 1,
                operations,
            },
            Area::new(3, 2, 40, 12),
            0,
        )
        .unwrap();
        full.present_auto(full.revision(), forest.clone(), Area::new(3, 2, 40, 12), 0)
            .unwrap();
        for round in 0..6 {
            let area = Area::new(3, 2, if round % 2 == 0 { 12 } else { 100 }, 12);
            let start = std::time::Instant::now();
            let ((stats, actual), bytes, copied) = measured(|| {
                let stats = warm.relayout_structural(area, 0).unwrap();
                let mut b = Buffer::empty(area);
                warm.render(area, &mut b);
                (stats, b)
            });
            println!("resize rows={rows} round={round} incremental=true allocated={bytes} copied_payload={copied} sync={} nanos={}", stats.sync_nodes, start.elapsed().as_nanos());
            assert_eq!(stats.sync_nodes, 1);
            assert!(stats.incremental_sync);
            assert_eq!(copied, 0, "accepted business descriptions stay borrowed");
            let start = std::time::Instant::now();
            let (expected, bytes, copied) = measured(|| {
                full.present_auto(full.revision(), forest.clone(), area, 0)
                    .unwrap();
                let mut b = Buffer::empty(area);
                full.render(area, &mut b);
                b
            });
            println!("resize rows={rows} round={round} incremental=false allocated={bytes} copied_payload={copied} sync={} nanos={}", rows+2, start.elapsed().as_nanos());
            assert_eq!(actual, expected, "rows={rows} round={round}");
            for node in &forest[0].children {
                assert_eq!(warm.frame(node.id), full.frame(node.id));
            }
            assert_eq!(warm.revision(), 1);
        }
    }
}

#[cfg(feature = "shell")]
#[test]
fn actual_sdk_resize_matches_complete_publication_without_new_sdk_mounts() {
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    use jadix_tui::ShellApplication;
    fn app(rows: usize) -> ScriptApplication {
        let mut registry = ComponentRegistry::default();
        for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../jadix_components/schema/portable-v1.json"
        ))
        .unwrap()
        {
            registry.register(schema).unwrap();
        }
        let source = format!(
            "globalThis.scaleRows={rows};globalThis.growingRows=true;\n{}",
            include_str!("../../jadix_shell/tests/fixtures/scaling.js")
        );
        ScriptApplication::development(
            "tests.tui.resizecost",
            &source,
            registry,
            jadix_shell::runtime::ResourceLimits {
                turn_milliseconds: 1000,
                memory_bytes: 128 * 1024 * 1024,
                ..Default::default()
            },
        )
        .unwrap()
    }
    for rows in [32, 256, 1024, 3072] {
        let mut warm = ShellApplication::new(app(rows), [8., 16.]).unwrap();
        let mut reference = app(rows);
        let mut full = CellSurface::new(reference.lifetime(), [8., 16.]).unwrap();
        let initial = Area::new(0, 0, 40, 12);
        warm.publish(initial, 0).unwrap();
        reference
            .publish(|nodes| {
                full.present_auto(full.revision(), nodes.to_vec(), initial, 0)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        for round in 0..6 {
            let area = Area::new(0, 0, if round % 2 == 0 { 12 } else { 100 }, 12);
            let start = std::time::Instant::now();
            let (actual, bytes, _) = measured(|| {
                assert_eq!(warm.publish(area, 0).unwrap(), 0);
                let mut b = Buffer::empty(area);
                warm.surface_mut().render(area, &mut b);
                b
            });
            println!("sdkresize rows={rows} round={round} incremental=true allocated={bytes} sync={} nanos={}",warm.layout_stats().sync_nodes,start.elapsed().as_nanos());
            assert_eq!(warm.layout_stats().sync_nodes, 1);
            let start = std::time::Instant::now();
            let (expected, bytes, _) = measured(|| {
                full.present_auto(full.revision(), reference.roots().to_vec(), area, 0)
                    .unwrap();
                let mut b = Buffer::empty(area);
                full.render(area, &mut b);
                b
            });
            println!(
                "sdkresize rows={rows} round={round} incremental=false allocated={bytes} nanos={}",
                start.elapsed().as_nanos()
            );
            assert_eq!(actual, expected, "rows={rows} round={round}");
        }
        assert_eq!(warm.application().work().full_mounts, 1);
        assert_eq!(reference.work().full_mounts, 1);
        assert_eq!(warm.surface().revision(), 1);
    }
}

/// Run separately so allocator observations include real SDK/VM publication.
#[cfg(feature = "shell")]
#[test]
#[ignore = "allocation/timing probe; run explicitly without concurrent builds"]
fn initial_sdk_mount_phases() {
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    use jadix_tui::ShellApplication;
    use std::time::Instant;
    let area = Area::new(0, 0, 80, 24);
    let mut records = vec![];
    for rows in [0, 256, 1024, 3072] {
        for round in 0..6 {
            let mut registry = ComponentRegistry::default();
            for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
                "../../jadix_components/schema/portable-v1.json"
            ))
            .unwrap()
            {
                registry.register(schema).unwrap();
            }
            let source = format!(
                "globalThis.businessRows={rows};\n{}",
                include_str!("../../jadix_shell/tests/fixtures/counter-leaves.js")
            );
            let start = Instant::now();
            let (script, load_bytes, _) = measured(|| {
                ScriptApplication::development(
                    "tests.tui.mount",
                    &source,
                    registry,
                    jadix_shell::runtime::ResourceLimits {
                        turn_milliseconds: 1000,
                        memory_bytes: 128 * 1024 * 1024,
                        ..Default::default()
                    },
                )
                .unwrap()
            });
            let load_ns = start.elapsed().as_nanos() as u64;
            let mut app = ShellApplication::new(script, [8., 16.]).unwrap();
            let start = Instant::now();
            let (_, publish_bytes, _) = measured(|| app.publish(area, 0).unwrap());
            let publish_ns = start.elapsed().as_nanos() as u64;
            let start = Instant::now();
            let (image, paint_bytes, _) = measured(|| {
                let mut image = Buffer::empty(area);
                app.surface_mut().render(area, &mut image);
                image
            });
            let paint_ns = start.elapsed().as_nanos() as u64;
            let lines: Vec<String> = image
                .content
                .chunks(80)
                .map(|row| row.iter().map(|c| c.symbol()).collect())
                .collect();
            assert_eq!(lines[0].trim_end(), "Counter 0");
            assert!(lines[1].contains("Increment"));
            for n in 0..rows.min(21) {
                assert_eq!(lines[n + 3].trim_end(), format!("Row {n}: business"));
            }
            assert_eq!(app.surface().nodes().count(), rows + 4);
            assert_eq!(app.application().work().full_mounts, 1);
            records.push(serde_json::json!({
                "rows":rows,"round":round,"load_bytes":load_bytes,"load_ns":load_ns,
                "publish_bytes":publish_bytes,"publish_ns":publish_ns,
                "paint_bytes":paint_bytes,"paint_ns":paint_ns,"lines":lines,
                "nodes":app.surface().nodes().count(),
                "cells":image.content.iter().map(|c|serde_json::json!([c.symbol(),format!("{:?}",c.fg),format!("{:?}",c.bg),c.modifier.bits()])).collect::<Vec<_>>()
            }));
            println!("mount rows={rows} round={round} load={load_bytes} publish={publish_bytes} paint={paint_bytes} load_ns={load_ns} publish_ns={publish_ns} paint_ns={paint_ns}");
            app.dispose();
        }
    }
    if let Ok(path) = std::env::var("JADIX_MOUNT_REPORT") {
        std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    }
}

/// Common snapshot projection used before GPU/native host-specific work.
#[cfg(feature = "shell")]
#[test]
#[ignore = "allocation/timing probe; run explicitly without concurrent builds"]
fn initial_shared_snapshot_payload() {
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    use std::time::Instant;
    let mut records = vec![];
    for children in [1, 4, 16] {
        for round in 0..6 {
            let mut registry = ComponentRegistry::default();
            for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
                "../../jadix_components/schema/portable-v1.json"
            ))
            .unwrap()
            {
                registry.register(schema).unwrap();
            }
            let source = format!(
                r#"const answer=JSON.parse(__jadixHost(JSON.stringify({{
                    type:"mount",tree:{{component:"Box",props:{{}},children:
                    Array.from({{length:{children}}},()=>({{component:"Text",props:{{text:"汉🦀".repeat(8192)}}}}))}}
                }})));if(!answer.ok)throw new Error(answer.error);"#
            );
            let mut app = ScriptApplication::development(
                "tests.snapshot.payload",
                &source,
                registry,
                jadix_shell::runtime::ResourceLimits {
                    turn_milliseconds: 1000,
                    memory_bytes: 128 * 1024 * 1024,
                    ..Default::default()
                },
            )
            .unwrap();
            let start = Instant::now();
            let (accepted, projection_bytes, _) = measured(|| app.publish(|_| Ok(())).unwrap());
            let projection_ns = start.elapsed().as_nanos() as u64;
            assert_eq!(accepted, 1);
            assert_eq!(app.roots()[0].children.len(), children);
            for node in &app.roots()[0].children {
                assert!(
                    matches!(&node.descriptor.content, NodeContent::Text(text) if text == &"汉🦀".repeat(8192))
                );
            }
            records.push(serde_json::json!({"children":children,"round":round,
                "payload_bytes":children*7*8192,"projection_bytes":projection_bytes,
                "projection_ns":projection_ns,"work":app.work()}));
            println!("snapshot children={children} round={round} payload={} allocated={projection_bytes} nanos={projection_ns}",children*7*8192);
            app.dispose();
        }
    }
    if let Ok(path) = std::env::var("JADIX_MOUNT_REPORT") {
        std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    }
}
