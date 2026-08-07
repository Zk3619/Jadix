//! Layout baseline bench (`LayoutTree::build_and_layout` / `LayoutProfileMs`).
//!
//! Run from the workspace root:
//! ```text
//! cargo bench -p rui --bench layout_baseline
//! ```
//!
//! Optional structured spans (no effect on this harness's printed timings):
//! ```text
//! cargo bench -p rui --bench layout_baseline --features tracing
//! ```

use std::time::Instant;

use rui::layout::layout_tree::LayoutTree;
use rui::tree::Element;

fn sample_tree(leaf_count: usize) -> Element {
    let mut root = Element::new().w(800.0).h(600.0).flex_col();
    for i in 0..leaf_count {
        root = root.child(Element::new().w(100.0).h(16.0).text(format!("item-{i}")));
    }
    root
}

fn main() {
    let mut layout_tree = LayoutTree::new();
    let mut root = sample_tree(32);

    // Warmup so Taffy node reuse / font measure paths settle.
    for _ in 0..5 {
        let _ = layout_tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    }

    const ITERS: u32 = 50;
    let t0 = Instant::now();
    let mut last_prof = None;
    for _ in 0..ITERS {
        let result = layout_tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        last_prof = Some(result.prof);
    }
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let prof = last_prof.expect("at least one iteration");

    println!(
        "layout_baseline: {ITERS} iters in {elapsed_ms:.3}ms ({:.3}ms/iter); \
         last LayoutProfileMs: rebuild={:.3} taffy={:.3} collect={:.3}",
        elapsed_ms / f64::from(ITERS),
        prof.rebuild_ms,
        prof.taffy_compute_ms,
        prof.collect_ms,
    );
}
