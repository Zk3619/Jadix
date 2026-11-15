//! Complete host publication/layout/Buffer paint costs, including remaining N work.
#![cfg(feature = "shell")]
use jadix_core::presentation::NodeContent;
use jadix_tui::ratatui::{buffer::Buffer, layout::Rect as Area, widgets::Widget};
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

#[cfg(feature = "shell")]
#[test]
fn actual_sdk_editor_leaf_echo_undo_and_server_replacement_match_full_mounts_across_tree_sizes() {
    use jadix_core::presentation::component::ComponentSchema;
    use jadix_shell::{application::ScriptApplication, wire::ComponentRegistry};
    use jadix_tui::{
        crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        },
        ShellApplication,
    };
    fn app(rows: usize, partial: bool) -> ShellApplication {
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
        let source = format!(
            "globalThis.scaleRows={rows};globalThis.leafPatches={partial};\n{}",
            include_str!("../../jadix_shell/tests/fixtures/controlled-leaves.js")
        );
        ShellApplication::new(
            ScriptApplication::development(
                "tests.editorcost",
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
    let area = Area::new(0, 0, 40, 12);
    for rows in [32, 256, 1024, 3072] {
        let mut apps = [app(rows, true), app(rows, false)];
        for a in &mut apps {
            a.publish(area, 0).unwrap();
            let mut b = Buffer::empty(area);
            a.surface_mut().render(area, &mut b);
            a.surface_mut().find("Row").unwrap();
        }
        for round in 0..6 {
            let event = match round {
                0 => Event::Paste("中文 🦀".into()),
                3 => Event::Paste("-2".into()),
                1 | 4 => Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL)),
                _ => Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
            };
            let mut images = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let a = &mut apps[mode];
                let start = std::time::Instant::now();
                let (image, allocated, _) = measured(|| {
                    a.handle(event.clone()).unwrap();
                    assert_eq!(a.publish(area, 0).unwrap(), 1);
                    let mut b = Buffer::empty(area);
                    a.surface_mut().render(area, &mut b);
                    b
                });
                println!("editor rows={rows} round={round} partial={} allocated={allocated} sync={} reused={} nanos={}",mode==0,a.layout_stats().sync_nodes,a.layout_stats().geometry_reused,start.elapsed().as_nanos());
                if mode == 0 {
                    assert_eq!(a.layout_stats().sync_nodes, 1);
                }
                assert_eq!(a.surface().search_summary().matches, rows);
                images[mode] = Some(image);
            }
            assert_eq!(images[0], images[1], "rows={rows} round={round}");
        }
        for round in 0..6 {
            let mut images = [None, None];
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                let a = &mut apps[mode];
                let start = std::time::Instant::now();
                let (image, allocated, _) = measured(|| {
                    a.handle(Event::Key(KeyEvent::new(
                        KeyCode::Char('a'),
                        KeyModifiers::CONTROL,
                    )))
                    .unwrap();
                    a.handle(Event::Paste(
                        if round % 2 == 0 { "aaaaa" } else { "bbbbb" }.into(),
                    ))
                    .unwrap();
                    a.publish(area, 0).unwrap();
                    let mut b = Buffer::empty(area);
                    a.surface_mut().render(area, &mut b);
                    b
                });
                println!("stableeditor rows={rows} round={round} partial={} allocated={allocated} sync={} reused={} nanos={}",mode==0,a.layout_stats().sync_nodes,a.layout_stats().geometry_reused,start.elapsed().as_nanos());
                if mode == 0 && round > 0 {
                    assert!(a.layout_stats().geometry_reused);
                }
                images[mode] = Some(image);
            }
            assert_eq!(images[0], images[1]);
        }
        for event in [
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Event::Paste("多行\n🙂".into()),
            Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 4,
                modifiers: KeyModifiers::NONE,
            }),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: 2,
                row: 4,
                modifiers: KeyModifiers::NONE,
            }),
        ] {
            let mut images = vec![];
            for a in &mut apps {
                a.handle(event.clone()).unwrap();
                a.publish(area, 0).unwrap();
                let mut b = Buffer::empty(area);
                a.surface_mut().render(area, &mut b);
                images.push(b);
            }
            assert_eq!(
                images[0], images[1],
                "secondary editor and server replacement"
            );
        }
        for a in &apps {
            assert!(
                matches!(&a.application().roots()[0].children[0].descriptor.content,NodeContent::Input {value,..} if value=="SERVER 中文 🦀")
            );
        }
        assert_eq!(apps[0].application().work().full_mounts, 1);
        assert!(apps[0].application().work().leaf_patches >= 10);
        assert_eq!(apps[1].application().work().leaf_patches, 0);
    }
}
