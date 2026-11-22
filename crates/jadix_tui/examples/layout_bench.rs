//! Retained versus rebuilt Taffy on identical shared presentation updates.
use jadix_core::presentation::{style::NodeStyle, *};
use jadix_layout::presentation::*;
use std::time::Instant;
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
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn measure(r: MeasureRequest, d: &NodeDescriptor) -> Result<MeasureResult, LayoutError> {
    let text = match &d.content {
        NodeContent::Text(s) => s.len(),
        _ => 0,
    };
    Ok(MeasureResult {
        key: r.key,
        width: (text as f64).min(r.key.max_width),
        height: 1_f64.min(r.key.max_height),
        baseline: None,
    })
}
fn main() -> Result<(), LayoutError> {
    let mut results = vec![];
    for count in [500, 5000] {
        for workload in ["unchanged", "text", "style", "reorder"] {
            for retained in [false, true] {
                let life = RootLifetime::new(RootId::new());
                let mut layout = PresentationLayout::new(life.clone());
                let mut root = node(life.id(), NodeContent::Container);
                root.descriptor.style.width = Some(80.);
                root.children = (0..count)
                    .map(|i| node(life.id(), NodeContent::Text(format!("row {i}"))))
                    .collect();
                let mut roots = vec![root];
                let started = Instant::now();
                layout.layout(&mut roots, [80., 24.], 0, measure)?;
                let initial = started.elapsed().as_nanos();
                let mut samples = vec![];
                let mut created = 0;
                let mut measurements = 0;
                let mut reused = 0;
                for step in 0..90 {
                    let start = Instant::now();
                    match workload {
                        "text" => {
                            roots[0].children[step % 24].descriptor.content =
                                NodeContent::Text(format!("updated {step}"))
                        }
                        "style" => {
                            roots[0].children[step % 24].descriptor.style.height =
                                Some(if (step / 24) % 2 == 0 { 2. } else { 1. })
                        }
                        "reorder" => roots[0].children.rotate_left(1),
                        _ => {}
                    }
                    if !retained {
                        layout.release_layout_cache();
                    }
                    let stats = layout.layout(&mut roots, [80., 24.], 0, measure)?;
                    if step >= 10 {
                        samples.push(start.elapsed().as_nanos());
                        created += stats.engine_created;
                        measurements += stats.measurements;
                        reused += stats.engine_reused;
                    }
                }
                samples.sort_unstable();
                results.push(serde_json::json!({"nodes":count,"workload":workload,"retained":retained,"median_ns":samples[40],"p95_ns":samples[75],"initial_ns":initial,"samples":80,"engine_created_total":created,"engine_reused_total":reused,"host_measurements_total":measurements,"scope":"mutation + shared layout sync/compute/frame collection; bounded host measurement cache in both; no app projection/paint/ANSI"}));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"results":results})
    );
    Ok(())
}
