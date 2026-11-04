use jadix_components::prelude::*;
use jadix_components::{authoring::native::ElementProjection, presentation::*};
use jadix_native::{layout::*, HeadlessRoot};

mod page;
#[cfg(test)]
mod tests;

fn main() {
    let (owner, (mut element, mut builder)) = create_root(|| {
        let count = create_signal(0usize);
        let value = create_signal(String::new());
        (page::page(count, value), page::builder_page(count, value))
    });
    let mut backend = HeadlessRoot::new();
    let lifetime = backend.lifetime();
    let projection = ElementProjection::with_lifetime(lifetime.clone());
    let mut layout = NativeLayout::new(lifetime);
    let mut nodes = vec![projection.snapshot(&mut element).unwrap()];
    assert_eq!(
        projection.snapshot(&mut builder).unwrap().children.len(),
        nodes[0].children.len()
    );
    let stats = layout
        .layout(&mut nodes, [320.0, 480.0], 1, |request, descriptor| {
            let width = match &descriptor.content {
                NodeContent::Text(text) => text.chars().count() as f64 * 8.0,
                _ => 100.0,
            };
            Ok(MeasureResult {
                key: request.key,
                width: width.min(request.key.max_width),
                height: 28.0f64.min(request.key.max_height),
                baseline: None,
            })
        })
        .unwrap();
    assert!(backend.reconcile(&nodes).unwrap());
    println!(
        "portable: {} nodes; {} measurements; theme {:?}",
        stats.nodes,
        stats.measurements,
        jadix_components::style::get_theme().mode
    );
    backend.dispose();
    drop(owner);
}
