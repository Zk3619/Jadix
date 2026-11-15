//! Reproducible CPU cell text timings; run without concurrent builds.
use jadix_core::presentation::{
    style::{NodeStyle, TextOverflow},
    NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect as Frame, RootId, RootLifetime,
};
use jadix_tui::{
    ratatui::{buffer::Buffer, layout::Rect, widgets::Widget},
    CellSurface,
};
use std::time::Instant;
fn measure(label: &str, text: String, width: u16, ellipsis: bool) -> serde_json::Value {
    let root = RootId::new();
    let lifetime = RootLifetime::new(root);
    let id = NodeId::new(root);
    let mut surface = CellSurface::new(lifetime, [1., 1.]).unwrap();
    let style = NodeStyle {
        text_overflow: if ellipsis {
            TextOverflow::Ellipsis
        } else {
            TextOverflow::Wrap
        },
        max_lines: ellipsis.then_some(1),
        ..Default::default()
    };
    surface
        .present(
            0,
            vec![PresentationNode {
                id,
                descriptor: NodeDescriptor {
                    content: NodeContent::Text(text.clone()),
                    style,
                    frame: Frame {
                        x: 0.,
                        y: 0.,
                        width: f64::from(width),
                        height: 4.,
                    },
                    accessible_description: None,
                    accessible_name: None,
                    accessible_role: None,
                    accessible_selected: None,
                    accessible_expanded: None,
                    accessibility_relations: Default::default(),
                },
                children: vec![],
            }],
        )
        .unwrap();
    let area = Rect::new(0, 0, width, 4);
    let mut buffer = Buffer::empty(area);
    let mut samples = vec![];
    for i in 0..120 {
        buffer.reset();
        let start = Instant::now();
        surface.render(area, &mut buffer);
        if i >= 20 {
            samples.push(start.elapsed().as_nanos() as u64);
        }
    }
    samples.sort_unstable();
    serde_json::json!({"case":label,"utf8_bytes":text.len(),"width":width,"samples":samples.len(),"median_us":samples[50] as f64/1000.,"p95_us":samples[95] as f64/1000.,"p99_us":samples[99] as f64/1000.})
}
fn main() {
    println!(
        "{}",
        serde_json::json!({"scope":"CPU CellSurface paint including Unicode layout; no nsh comparison or process RSS claim","cases":[measure("ascii-4096", "x".repeat(4096),4096,false),measure("alternating-ltr-rtl-4096", "aא".repeat(2048),4096,false),measure("wrapped-bidi", "English אבג 中文 😀 ".repeat(256),80,false),measure("ellipsis-mixed", "English אבג 中文 😀 ".repeat(256),80,true)]})
    );
}
