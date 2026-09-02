//! Gallery Edge Cases: bidi / RTL+emoji labels must soft-wrap inside a narrow card.

use crate::components::label::label;
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::r#box;
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::text::{glyphon_soft_wrap_line_count, invalidate_prepare_cache, shutdown_layout_text};
use glyphon::{Attrs, Buffer, Metrics, Shaping};

fn find_tid<'a>(node: &'a Element, id: &str) -> Option<&'a Element> {
    if node.template_id.as_deref() == Some(id) {
        return Some(node);
    }
    for c in &node.children {
        if let Some(hit) = find_tid(c, id) {
            return Some(hit);
        }
    }
    None
}

#[test]
fn gallery_bidi_labels_shrink_and_wrap_in_narrow_card() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            invalidate_prepare_cache();
            // Mirror TextCompatibilityShowcase nesting (card → column → labels).
            let bidi: Element = label("gal-bidi-sample")
                .w("100%")
                .leading(1.5)
                .text("Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789")
                .into();
            let rtl: Element = label("gal-rtl-emoji-mixed")
                .w("100%")
                .leading(1.5)
                .text("RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例")
                .into();
            let inner = Element::from(r#box().flex_col().w("100%").gap(8.0).child(bidi).child(rtl));
            let card = Element::from(r#box().flex_col().w("100%").min_w(0.0).p(12.0).child(inner));
            let mut root = Element::new().w("100%").h("100%").min_w(0.0).child(card);

            let viewport_w = 280.0_f32;
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, viewport_w, 600.0, [-1.0, -1.0]);

            for tid in ["gal-bidi-sample", "gal-rtl-emoji-mixed"] {
                let leaf = find_tid(&root, tid).unwrap_or_else(|| panic!("missing {tid}"));
                let w = leaf.layout_bounds[2];
                let h = leaf.layout_bounds[3];
                let text = leaf.text.as_deref().unwrap_or("");
                let size = leaf.text_style.size_resolved();
                let lh = leaf
                    .text_style
                    .line_height_scale
                    .map(|s| size * s)
                    .unwrap_or(size * 1.5);
                let runs = glyphon_soft_wrap_line_count(text, size, leaf.text_style.weight_resolved(), lh, w);
                println!("{tid}: lay={w:.1}x{h:.1} lh={lh:.1} glyphon_runs={runs}");
                assert!(
                    w <= viewport_w + 1.0,
                    "{tid} width {w} must fit viewport {viewport_w}"
                );
                assert!(
                    runs >= 2,
                    "{tid} must soft-wrap at laid-out width {w}, got {runs} runs"
                );
                assert!(
                    h + 0.5 >= lh * 2.0,
                    "{tid} height {h} should cover ≥2 lines (lh={lh})"
                );
            }

            shutdown_layout_text();
        });
    });
}

#[test]
fn gallery_bidi_labels_wrap_without_ancestor_min_w_zero() {
    // Reproduce the gallery nesting *without* min_w=0 on the inner column —
    // this used to let intrinsic text width expand past the card.
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            invalidate_prepare_cache();
            let bidi: Element = label("gal-bidi-no-min")
                .w("100%")
                .leading(1.5)
                .text("Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789")
                .into();
            // Intentionally omit min_w(0) on the inner column (gallery TextCompatibilityShowcase).
            let inner = Element::from(r#box().flex_col().w("100%").gap(8.0).child(bidi));
            let card = Element::from(r#box().flex_col().w("100%").min_w(0.0).p(12.0).child(inner));
            let mut root = Element::new().w("100%").h("100%").min_w(0.0).child(card);

            let viewport_w = 280.0_f32;
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, viewport_w, 400.0, [-1.0, -1.0]);
            let leaf = find_tid(&root, "gal-bidi-no-min").expect("label");
            let w = leaf.layout_bounds[2];
            let h = leaf.layout_bounds[3];
            let text = leaf.text.as_deref().unwrap_or("");
            let size = leaf.text_style.size_resolved();
            let lh = size * 1.5;
            let runs =
                glyphon_soft_wrap_line_count(text, size, leaf.text_style.weight_resolved(), lh, w.max(1.0));
            println!("no-ancestor-min_w: lay={w:.1}x{h:.1} runs={runs}");
            assert!(
                w <= viewport_w + 1.0,
                "even without inner min_w=0, label must not exceed viewport; w={w}"
            );
            assert!(runs >= 2, "expected wrap, runs={runs} at w={w}");
            shutdown_layout_text();
        });
    });
}

#[test]
fn gallery_bidi_labels_wrap_inside_scroll_area_like_root() {
    use crate::components::scroll_area::scroll_area;

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            invalidate_prepare_cache();
            let bidi: Element = label("gal-bidi-scroll")
                .w("100%")
                .leading(1.5)
                .text("Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789")
                .into();
            let rtl: Element = label("gal-rtl-scroll")
                .w("100%")
                .leading(1.5)
                .text("RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例")
                .into();
            let inner = Element::from(r#box().flex_col().w("100%").gap(8.0).child(bidi).child(rtl));
            let card = Element::from(
                r#box()
                    .flex_col()
                    .w("100%")
                    .min_w(0.0)
                    .p(12.0)
                    .border([0.0, 0.0, 0.0, 1.0], 1.0)
                    .child(inner),
            );
            let body = Element::from(r#box().flex_col().w("100%").p(16.0).child(card));
            // Match gallery App: ScrollArea grow=1, overflow_x=auto, overflow_y=auto.
            let mut root = Element::from(
                scroll_area("root-scroll-bidi")
                    .w(320.0)
                    .h(480.0)
                    .min_w(0.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(body),
            );

            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 320.0, 480.0, [-1.0, -1.0]);

            for tid in ["gal-bidi-scroll", "gal-rtl-scroll"] {
                let leaf = find_tid(&root, tid).unwrap_or_else(|| panic!("missing {tid}"));
                let w = leaf.layout_bounds[2];
                let h = leaf.layout_bounds[3];
                let text = leaf.text.as_deref().unwrap_or("");
                let size = leaf.text_style.size_resolved();
                let lh = size * 1.5;
                let runs = glyphon_soft_wrap_line_count(
                    text,
                    size,
                    leaf.text_style.weight_resolved(),
                    lh,
                    w.max(1.0),
                );
                println!(
                    "{tid}: lay={w:.1}x{h:.1} runs={runs} leaf={:?}",
                    leaf.layout_bounds
                );
                assert!(
                    w <= 320.0 + 1.0,
                    "{tid} must fit scroll viewport width, got {w}"
                );
                assert!(
                    runs >= 2,
                    "{tid} must soft-wrap inside scroll content, runs={runs} w={w}"
                );
                assert!(h + 0.5 >= lh * 2.0, "{tid} h={h} lh={lh}");
            }
            shutdown_layout_text();
        });
    });
}

#[test]
fn shaped_buffer_set_size_reflows_gallery_bidi_line() {
    // Mirrors prepare_text cache-hit path: reshape width without set_text.
    invalidate_prepare_cache();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    font_loader::ensure_coverage_for_text(&mut fs, text);
    let size = 14.0f32;
    let lh = size * 1.5;
    let family = best_font_family(&fs);
    let mut buffer = Buffer::new(&mut fs, Metrics::new(size, lh));
    buffer.set_size(Some(10_000.0), Some(lh * 4.0));
    buffer.set_text(
        text,
        &Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(500)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut fs, false);
    assert_eq!(buffer.layout_runs().count(), 1);

    let avail = 280.0f32;
    buffer.set_size(Some(avail), Some(lh * 8.0));
    buffer.shape_until_scroll(&mut fs, false);
    let runs: Vec<f32> = buffer.layout_runs().map(|r| r.line_w).collect();
    assert!(runs.len() >= 2, "set_size must soft-wrap; runs={runs:?}");
    assert!(
        runs.iter().all(|w| *w <= avail + 1.0),
        "wrapped line_w must fit avail; runs={runs:?}"
    );
    shutdown_layout_text();
}
