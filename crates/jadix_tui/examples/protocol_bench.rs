//! Includes complete transaction staging, validation and paint/diff in both modes.
use jadix_core::presentation::{style::NodeStyle, *};
use jadix_tui::{
    ratatui::{backend::TestBackend, layout::Rect as Cells, Terminal},
    CellSurface,
};
use std::time::Instant;
fn descriptor(value: String, index: usize) -> NodeDescriptor {
    NodeDescriptor {
        content: NodeContent::Text(value),
        frame: Rect {
            x: 0.,
            y: index as f64,
            width: 80.,
            height: 1.,
        },
        style: NodeStyle {
            foreground: [255; 4],
            ..Default::default()
        },
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
    }
}
fn main() -> std::io::Result<()> {
    let mut results = vec![];
    for count in [500usize, 5000] {
        for incremental in [false, true] {
            let root = RootId::new();
            let ids: Vec<_> = (0..count).map(|_| NodeId::new(root)).collect();
            let mut tree: Vec<_> = ids
                .iter()
                .enumerate()
                .map(|(i, id)| PresentationNode {
                    id: *id,
                    descriptor: descriptor(format!("row {i}"), i),
                    children: vec![],
                })
                .collect();
            let mut surface = CellSurface::new(RootLifetime::new(root), [1., 1.])?;
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            let init = Instant::now();
            surface.present(0, tree.clone())?;
            terminal
                .draw(|f| f.render_widget(&mut surface, f.area()))
                .unwrap();
            let initial_ns = init.elapsed().as_nanos();
            let mut samples = vec![];
            for step in 0..140 {
                let index = step % 24;
                let text = format!("update {step}");
                let start = Instant::now();
                let next = descriptor(text, index);
                if incremental {
                    let batch = CommitBatch {
                        root,
                        previous: surface.revision(),
                        revision: surface.revision() + 1,
                        operations: vec![Operation::Update {
                            node: ids[index],
                            descriptor: next,
                        }],
                    };
                    surface.apply_batch(&batch)?;
                } else {
                    tree[index].descriptor = next;
                    surface.present(surface.revision(), tree.clone())?;
                }
                terminal
                    .draw(|f| f.render_widget(&mut surface, f.area()))
                    .unwrap();
                if step >= 20 {
                    samples.push(start.elapsed().as_nanos());
                }
            }
            samples.sort_unstable();
            assert!(!surface.needs_frame());
            assert_eq!(terminal.size().unwrap(), Cells::new(0, 0, 80, 24).as_size());
            results.push(serde_json::json!({"nodes":count,"incremental_payload":incremental,"initial_present_paint_ns":initial_ns,"median_ns":samples[60],"p95_ns":samples[113],"samples":120,
                "scope":"descriptor update + transaction staging/validation + paint + TestBackend diff; no ANSI/TTY; no auto-layout", "fine_grained_layout":false}));
        }
    }
    println!(
        "{}",
        serde_json::json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"results":results})
    );
    Ok(())
}
