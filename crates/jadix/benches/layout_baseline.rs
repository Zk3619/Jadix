//! Layout baseline bench (`LayoutTree::build_and_layout` / `LayoutProfileMs`).
//!
//! Run from the workspace root:
//! ```text
//! cargo bench -p jadix --bench layout_baseline
//! cargo bench -p jadix --bench layout_baseline -- scroll
//! ```
//!
//! Optional structured spans (no effect on this harness's printed timings):
//! ```text
//! cargo bench -p jadix --bench layout_baseline --features tracing
//! ```

use std::time::Instant;

use jadix::layout::layout_tree::LayoutTree;
use jadix::tree::Element;

fn sample_tree(leaf_count: usize) -> Element {
    let mut root = Element::new().w(800.0).h(600.0).flex_col();
    for i in 0..leaf_count {
        root = root.child(Element::new().w(100.0).h(16.0).text(format!("item-{i}")));
    }
    root
}

fn scrolled_clipped_column(row_count: usize) -> Element {
    let content_h = row_count as f32 * 48.0;
    let mut col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w(320.0)
        .h(content_h)
        .min_h(content_h);
    for _ in 0..row_count {
        col = col.child(
            Element::new()
                .w(320.0)
                .h(48.0)
                .flex_shrink(0.0)
                .bg([0.2, 0.4, 0.9, 1.0]),
        );
    }
    Element::new()
        .w(320.0)
        .h(240.0)
        .overflow_y("hidden")
        .child(col)
}

fn bench_scroll_only(label: &str, iters: u32, row_count: usize, scroll_y: f32) {
    let mut layout_tree = LayoutTree::new();
    let mut root = scrolled_clipped_column(row_count);
    let _ = layout_tree.build_and_layout(&mut root, 320.0, 240.0, [-1.0, -1.0]);
    root.scroll_offset = [0.0, scroll_y];

    for _ in 0..5 {
        let _ =
            layout_tree.build_and_layout_fast(&mut root, 320.0, 240.0, [-1.0, -1.0], true, false);
    }

    let t0 = Instant::now();
    let mut last = None;
    for _ in 0..iters {
        let result =
            layout_tree.build_and_layout_fast(&mut root, 320.0, 240.0, [-1.0, -1.0], true, false);
        last = Some((result.prof.collect_ms, result.instances.len()));
    }
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let (collect_ms, inst) = last.expect("at least one iteration");
    println!(
        "{label}: {iters} iters in {elapsed_ms:.3}ms ({:.3}ms/iter); \
         last collect={collect_ms:.3}ms instances={inst}",
        elapsed_ms / f64::from(iters)
    );
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();

    if mode == "scroll" {
        bench_scroll_only("scroll_only_100rows_y800", 100, 100, 800.0);
        bench_scroll_only("scroll_only_100rows_y4000", 100, 100, 4000.0);
        return;
    }

    let mut layout_tree = LayoutTree::new();
    let mut root = sample_tree(32);

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
