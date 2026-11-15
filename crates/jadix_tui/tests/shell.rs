#![cfg(feature = "shell")]
use jadix_core::presentation::{component::ComponentSchema, NodeContent};
use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
use jadix_tui::{
    crossterm::event::*,
    ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
    ShellApplication,
};
#[test]
fn tsx_runs_on_ratatui_and_keyboard_activation_reaches_the_same_js_callback() {
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
    let app = ScriptApplication::development(
        "tests.tui",
        include_str!("../../jadix_shell/tests/fixtures/application.js"),
        registry,
        Default::default(),
    )
    .unwrap();
    let mut app = ShellApplication::new(app, [8., 16.]).unwrap();
    let area = Rect::new(0, 0, 40, 8);
    app.publish(area, 0).unwrap();
    let mut buffer = Buffer::empty(area);
    app.surface_mut().render(area, &mut buffer);
    let button = app.application().node_at(&[1]).unwrap();
    app.surface_mut().focus_node(button).unwrap();
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    app.publish(area, 0).unwrap();
    assert!(app.layout_stats().incremental_sync);
    assert_eq!(app.layout_stats().sync_nodes, 1);
    assert_eq!(app.layout_stats().engine_created, 0);
    assert_eq!(
        app.application().roots()[0].children[0].descriptor.content,
        NodeContent::Text("Count: 1".into())
    );
}

#[test]
fn shell_exit_request_stops_host_dispatch_before_vm_dispose() {
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap()
    .into_iter()
    .chain(
        serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../jadix_components/schema/extended-v1.json"
        ))
        .unwrap(),
    ) {
        registry.register(schema).unwrap();
    }
    let script = ScriptApplication::development(
        "tests.tui",
        include_str!("../../jadix_shell/tests/fixtures/application.js"),
        registry,
        Default::default(),
    )
    .unwrap();
    let lifetime = script.lifetime();
    let mut app = ShellApplication::new(script, [8., 16.]).unwrap();
    let area = Rect::new(0, 0, 40, 8);
    app.publish(area, 0).unwrap();
    assert!(!app.exit_requested());
    assert!(!lifetime.is_disposed());

    app.request_exit();
    app.request_exit();
    assert!(app.exit_requested());
    assert!(!lifetime.is_disposed());
    assert!(app.publish(area, 1).is_err());
    assert!(app
        .handle(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )))
        .is_err());
    assert!(app.events(std::iter::empty()).is_err());

    app.dispose();
    app.dispose();
    assert!(lifetime.is_disposed());
}
#[test]
fn sdk_keyed_reorder_insert_remove_reaches_structural_taffy_and_matches_full_layout() {
    use jadix_core::presentation::{EventPayload, PlatformEvent};
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap()
    {
        registry.register(schema).unwrap();
    }
    let app = ScriptApplication::development(
        "tests.tui.structural",
        include_str!("../../jadix_shell/tests/fixtures/structural.js"),
        registry,
        Default::default(),
    )
    .unwrap();
    let mut app = ShellApplication::new(app, [8., 16.]).unwrap();
    let mut area = Rect::new(0, 0, 40, 12);
    app.publish(area, 0).unwrap();
    let button = app.application().node_at(&[0]).unwrap();
    let a = app.application().node_at(&[1]).unwrap();
    let b = app.application().node_at(&[2]).unwrap();
    for phase in 1..=10 {
        app.events([PlatformEvent {
            node: button,
            payload: EventPayload::Activate,
        }])
        .unwrap();
        assert_eq!(app.publish(area, 0).unwrap(), 1);
        assert!(app.layout_stats().incremental_sync);
        assert_eq!(
            app.layout_stats().engine_created,
            usize::from(phase % 2 == 1)
        );
        assert_eq!(
            app.application().node_at(&[1]),
            Some(if phase % 2 == 1 { b } else { a })
        );
        let mut fresh =
            jadix_tui::CellSurface::new(app.application().lifetime(), [8., 16.]).unwrap();
        fresh
            .present_auto(0, app.application().roots().to_vec(), area, 0)
            .unwrap();
        let mut left = Buffer::empty(area);
        let mut right = Buffer::empty(area);
        app.surface_mut().render(area, &mut left);
        (&mut fresh).render(area, &mut right);
        assert_eq!(left, right);
    }
    let revision = app.surface().revision();
    area.width = 25;
    assert_eq!(app.publish(area, 1).unwrap(), 0);
    assert!(!app.layout_stats().incremental_sync);
    assert_eq!(app.surface().revision(), revision);
}

#[test]
fn unchanged_script_render_does_not_swallow_viewport_or_environment_invalidation() {
    use jadix_core::presentation::{EventPayload, PlatformEvent};
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap()
    {
        registry.register(schema).unwrap();
    }
    let source = include_str!("../../jadix_shell/tests/fixtures/structural.js")
        .replace("phase++;", "phase += 0;");
    let script =
        ScriptApplication::development("tests.resize.noop", &source, registry, Default::default())
            .unwrap();
    let mut app = ShellApplication::new(script, [8., 16.]).unwrap();
    app.publish(Rect::new(0, 0, 40, 12), 0).unwrap();
    let root = app.application().roots()[0].id;
    let button = app.application().node_at(&[0]).unwrap();
    let revision = app.surface().revision();
    app.events([PlatformEvent {
        node: button,
        payload: EventPayload::Activate,
    }])
    .unwrap();
    assert_eq!(app.publish(Rect::new(3, 2, 20, 10), 1).unwrap(), 1);
    assert_eq!(app.surface().revision(), revision);
    assert!(!app.layout_stats().incremental_sync);
    assert_eq!(app.surface().frame(root).unwrap().width, 160.);
}

#[test]
fn sdk_max_length_updates_shared_descriptors_and_terminal_editing() {
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap()
    .into_iter()
    .chain(
        serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../jadix_components/schema/extended-v1.json"
        ))
        .unwrap(),
    ) {
        registry.register(schema).unwrap();
    }
    let app = ScriptApplication::development(
        "tests.max-length",
        include_str!("../../jadix_shell/tests/fixtures/max-length.js"),
        registry,
        Default::default(),
    )
    .unwrap();
    let mut app = ShellApplication::new(app, [8., 16.]).unwrap();
    let area = Rect::new(0, 0, 40, 16);
    app.publish(area, 0).unwrap();
    let input = app.application().node_at(&[0]).unwrap();
    let notes = app.application().node_at(&[1]).unwrap();
    let button = app.application().node_at(&[2]).unwrap();
    app.surface_mut().focus_node(input).unwrap();
    app.handle(Event::Paste("ab中🦀x".into())).unwrap();
    app.publish(area, 0).unwrap();
    assert_eq!(
        app.surface().field(input).unwrap().document().text(),
        "ab中🦀"
    );
    assert_eq!(
        app.application().roots()[0].children[0]
            .descriptor
            .content
            .input_max_length(),
        Some(5)
    );
    app.surface_mut().focus_node(notes).unwrap();
    app.handle(Event::Paste("中🦀xy".into())).unwrap();
    app.publish(area, 0).unwrap();
    assert_eq!(
        app.surface().field(notes).unwrap().document().text(),
        "中🦀x"
    );
    app.surface_mut().focus_node(button).unwrap();
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    app.publish(area, 0).unwrap();
    assert_eq!(
        app.application().roots()[0].children[0]
            .descriptor
            .content
            .input_max_length(),
        Some(2)
    );
    app.surface_mut().focus_node(input).unwrap();
    app.handle(Event::Paste("Z".into())).unwrap();
    assert_eq!(
        app.surface().field(input).unwrap().document().text(),
        "ab中🦀"
    );
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    app.handle(Event::Paste("👩‍💻".into())).unwrap();
    assert_eq!(
        app.surface().field(input).unwrap().document().text(),
        "ab中🦀"
    );
    app.handle(Event::Paste("🦀x".into())).unwrap();
    app.publish(area, 0).unwrap();
    assert_eq!(app.surface().field(input).unwrap().document().text(), "🦀");
}

#[test]
fn sparse_sdk_leaf_packets_match_full_terminal_output_on_large_business_trees() {
    use jadix_core::presentation::{EventPayload, PlatformEvent};
    for rows in [32, 256, 1024, 3072] {
        let mut registry = ComponentRegistry::default();
        for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
            "../../jadix_components/schema/portable-v1.json"
        ))
        .unwrap()
        {
            registry.register(schema).unwrap();
        }
        let source = format!(
            "globalThis.scaleRows={rows};globalThis.leafPatches=true;\n{}",
            include_str!("../../jadix_shell/tests/fixtures/scaling.js")
        );
        let script = ScriptApplication::development(
            "tests.tui.sparse",
            &source,
            registry,
            jadix_shell::runtime::ResourceLimits {
                turn_milliseconds: 1000,
                memory_bytes: 128 * 1024 * 1024,
                ..Default::default()
            },
        )
        .unwrap();
        let mut app = ShellApplication::new(script, [8., 16.]).unwrap();
        let area = Rect::new(0, 0, 40, 8);
        app.publish(area, 0).unwrap();
        let button = app.application().node_at(&[0]).unwrap();
        let text = app.application().node_at(&[1]).unwrap();
        for count in 1..=3 {
            app.events([PlatformEvent {
                node: button,
                payload: EventPayload::Activate,
            }])
            .unwrap();
            assert_eq!(app.publish(area, 0).unwrap(), 1);
            assert_eq!(app.application().node_at(&[1]), Some(text));
            assert_eq!(
                app.application().roots()[0].children[1].descriptor.content,
                NodeContent::Text(format!("Row 0: {count}"))
            );
            assert_eq!(app.layout_stats().sync_nodes, 1);
            let mut reference =
                jadix_tui::CellSurface::new(app.application().lifetime(), [8., 16.]).unwrap();
            reference
                .present_auto(0, app.application().roots().to_vec(), area, 0)
                .unwrap();
            let mut actual = Buffer::empty(area);
            let mut expected = Buffer::empty(area);
            app.surface_mut().render(area, &mut actual);
            (&mut reference).render(area, &mut expected);
            assert_eq!(actual, expected, "rows={rows} count={count}");
        }
        let revision = app.surface().revision();
        assert_eq!(app.publish(area, 0).unwrap(), 0);
        assert_eq!(app.surface().revision(), revision);
        assert_eq!(app.application().work().full_mounts, 1);
        assert_eq!(app.application().work().leaf_patches, 3);
    }
}

#[test]
fn sdk_resize_without_commits_clamps_scroll_and_preserves_local_editor_history() {
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap()
    {
        registry.register(schema).unwrap();
    }
    let script = ScriptApplication::development(
        "tests.resize.scroll",
        include_str!("../../jadix_shell/tests/fixtures/resize-scroll.js"),
        registry,
        Default::default(),
    )
    .unwrap();
    let mut app = ShellApplication::new(script, [8., 16.]).unwrap();
    let narrow = Rect::new(3, 2, 12, 10);
    let wide = Rect::new(3, 2, 100, 10);
    app.publish(narrow, 0).unwrap();
    let scroll = app.application().node_at(&[0]).unwrap();
    let input = app.application().node_at(&[1]).unwrap();
    let mut full = jadix_tui::CellSurface::new(app.application().lifetime(), [8., 16.]).unwrap();
    full.present_auto(0, app.application().roots().to_vec(), narrow, 0)
        .unwrap();
    app.surface_mut().set_scrollbars(true);
    full.set_scrollbars(true);
    for host in [app.surface_mut(), &mut full] {
        host.focus_node(input).unwrap();
        host.handle(Event::Paste("中文 🦀 ".into())).unwrap();
        assert!(host.scroll_to(scroll, [0., 1e6]).unwrap());
    }
    let checkpoint = app
        .surface()
        .field(input)
        .unwrap()
        .checkpoint_bytes()
        .unwrap();
    let revision = app.surface().revision();
    for (round, (area, environment)) in [(wide, 0), (narrow, 1), (wide, 1), (wide, 2)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            app.publish(area, environment).unwrap(),
            0,
            "resize sends no SDK commit"
        );
        full.present_auto(
            full.revision(),
            app.application().roots().to_vec(),
            area,
            environment,
        )
        .unwrap();
        let mut actual = Buffer::empty(area);
        let mut expected = Buffer::empty(area);
        app.surface_mut().render(area, &mut actual);
        (&mut full).render(area, &mut expected);
        assert_eq!(
            actual, expected,
            "formal SDK resize reference round={round}"
        );
        assert_eq!(app.surface().revision(), revision);
        assert_eq!(app.surface().focused(), Some(input));
        assert_eq!(
            app.surface()
                .field(input)
                .unwrap()
                .checkpoint_bytes()
                .unwrap(),
            checkpoint
        );
        // At wide width all content fits: no obsolete offset can remain.
        if area == wide {
            assert!(!app.surface_mut().scroll_to(scroll, [0., 0.]).unwrap());
        } else {
            for host in [app.surface_mut(), &mut full] {
                assert!(host.scroll_to(scroll, [0., 1e6]).unwrap());
            }
        }
    }
    let undo = Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert_eq!(
        app.surface_mut().handle(undo.clone()).unwrap(),
        full.handle(undo).unwrap()
    );
    assert_eq!(
        app.surface().field(input).unwrap().document().text(),
        "draft"
    );
}

#[test]
fn checkbox_switch_and_select_leaf_updates_match_full_mount_without_layout_rebuild() {
    use jadix_core::presentation::{EventPayload, PlatformEvent};
    fn assert_same_tree(
        a: &[jadix_core::presentation::PresentationNode],
        b: &[jadix_core::presentation::PresentationNode],
    ) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.descriptor.content, b.descriptor.content);
            assert_eq!(a.descriptor.style, b.descriptor.style);
            assert_eq!(a.descriptor.frame, b.descriptor.frame);
            assert_same_tree(&a.children, &b.children);
        }
    }
    let make = |partial| {
        let schemas: Vec<ComponentSchema> = serde_json::from_str(include_str!(
            "../../jadix_components/schema/portable-v1.json"
        ))
        .unwrap();
        let mut registry = ComponentRegistry::default();
        for schema in schemas.into_iter().chain(
            serde_json::from_str::<Vec<ComponentSchema>>(include_str!(
                "../../jadix_components/schema/extended-v1.json"
            ))
            .unwrap(),
        ) {
            registry.register(schema).unwrap();
        }
        let source = format!(
            "globalThis.leafPatches={partial};globalThis.scaleRows=1024;\n{}",
            include_str!("../../jadix_shell/tests/fixtures/control-leaves.js")
        );
        let script = ScriptApplication::development(
            "tests.tui.control-leaves",
            &source,
            registry,
            Default::default(),
        )
        .unwrap();
        ShellApplication::new(script, [8., 16.]).unwrap()
    };
    let (mut partial, mut full) = (make(true), make(false));
    let area = Rect::new(0, 0, 40, 12);
    assert_eq!(partial.publish(area, 0).unwrap(), 1);
    assert_eq!(full.publish(area, 0).unwrap(), 1);
    assert_same_tree(partial.application().roots(), full.application().roots());
    let ids: Vec<_> = (0..3)
        .map(|i| partial.application().node_at(&[i]).unwrap())
        .collect();
    let full_ids: Vec<_> = (0..3)
        .map(|i| full.application().node_at(&[i]).unwrap())
        .collect();
    for (index, (node, payload)) in [
        (
            ids[0],
            EventPayload::Toggle {
                checked: true,
                revision: 0,
            },
        ),
        (
            ids[1],
            EventPayload::Toggle {
                checked: true,
                revision: 0,
            },
        ),
        (
            ids[2],
            EventPayload::Select {
                value: "two".into(),
                revision: 0,
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let event = PlatformEvent { node, payload };
        partial.events([event.clone()]).unwrap();
        full.events([PlatformEvent {
            node: full_ids[index],
            payload: event.payload,
        }])
        .unwrap();
        assert_eq!(partial.publish(area, 0).unwrap(), 1);
        full.publish(area, 0).unwrap();
        assert_same_tree(partial.application().roots(), full.application().roots());
        assert!(partial.layout_stats().incremental_sync);
        assert_eq!(partial.layout_stats().engine_created, 0);
        assert_eq!(partial.layout_stats().sync_nodes, 1);
        let mut left = Buffer::empty(area);
        let mut right = Buffer::empty(area);
        partial.surface_mut().render(area, &mut left);
        full.surface_mut().render(area, &mut right);
        assert_eq!(left, right);
    }
    assert!(matches!(
        partial.application().roots()[0].children[0]
            .descriptor
            .content,
        NodeContent::Checkbox { checked: true, .. }
    ));
    assert!(matches!(
        partial.application().roots()[0].children[1]
            .descriptor
            .content,
        NodeContent::Switch { checked: true, .. }
    ));
    assert!(
        matches!(&partial.application().roots()[0].children[2].descriptor.content,NodeContent::Select{value,..} if value=="two")
    );
}
