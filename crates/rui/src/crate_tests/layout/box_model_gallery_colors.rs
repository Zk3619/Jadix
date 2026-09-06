//! Gallery WP-2.9 onion / B15 demo colors must stay visually distinct.
//!
//! Light zinc: `muted` ≈ `secondary` ≈ `accent` (oklch 0.97). Using those as
//! adjacent layers made padding and the bottom border disappear. Tokens here
//! must match `examples/gallery/src/pages/layout.rs` (`GAL_BM_*`).

use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::r#box;
use crate::style::{parse_color, set_theme_mode, ThemeMode};

const MARGIN: &str = "muted";
const PADDING: &str = "chart-1";
const CONTENT: &str = "chart-4";
const BORDER_T: &str = "destructive";
const BORDER_R: &str = "chart-2";
const BORDER_B: &str = "chart-5";
const BORDER_L: &str = "chart-3";

fn rgb_dist(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d0 = a[0] - b[0];
    let d1 = a[1] - b[1];
    let d2 = a[2] - b[2];
    (d0 * d0 + d1 * d1 + d2 * d2).sqrt()
}

fn assert_distinct(a_name: &str, b_name: &str) {
    let a = parse_color(a_name);
    let b = parse_color(b_name);
    assert!(
        rgb_dist(a, b) > 0.12,
        "{a_name} vs {b_name} must stay apart on the onion (dist={:.3}, {a:?} {b:?})",
        rgb_dist(a, b)
    );
}

fn color_close(inst: [f32; 4], token: [f32; 4]) -> bool {
    inst[3] > 0.5 && rgb_dist(inst, token) < 0.08
}

fn gal_box_model_onion() -> Element {
    let content = Element::from(
        r#box()
            .w("100%")
            .h("3rem")
            .bg(parse_color(CONTENT))
            .rounded("0.25rem"),
    );
    let padding = Element::from(
        r#box()
            .w("100%")
            .p("1rem")
            .bg(parse_color(PADDING))
            .child(content),
    );
    let border_box = Element::from(
        r#box()
            .w("100%")
            .border_style("solid")
            .border_top(parse_color(BORDER_T), 8.0)
            .border_right(parse_color(BORDER_R), 4.0)
            .border_bottom(parse_color(BORDER_B), 12.0)
            .border_left(parse_color(BORDER_L), 16.0)
            .child(padding),
    );
    Element::from(
        r#box()
            .w("22rem")
            .p("1.25rem")
            .bg(parse_color(MARGIN))
            .rounded("0.5rem")
            .child(border_box),
    )
}

#[test]
fn zinc_light_muted_secondary_accent_collide() {
    set_theme_mode(ThemeMode::Light);
    let muted = parse_color("muted");
    let secondary = parse_color("secondary");
    let accent = parse_color("accent");
    assert!(
        rgb_dist(muted, secondary) < 0.05 && rgb_dist(muted, accent) < 0.05,
        "test assumes zinc light muted≈secondary≈accent; if the theme diverges, drop this guard"
    );
}

#[test]
fn onion_teaching_tokens_are_pairwise_distinct() {
    set_theme_mode(ThemeMode::Light);
    let layers = [
        MARGIN, PADDING, CONTENT, BORDER_T, BORDER_R, BORDER_B, BORDER_L,
    ];
    for (i, a) in layers.iter().enumerate() {
        for b in layers.iter().skip(i + 1) {
            assert_distinct(a, b);
        }
    }
}

#[test]
fn onion_paint_emits_margin_padding_content_and_per_side_borders() {
    set_theme_mode(ThemeMode::Light);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let onion = gal_box_model_onion();
            let mut root = Element::new().w(400.0).h(200.0).p(8.0).child(onion);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
            let want = [
                ("margin", parse_color(MARGIN)),
                ("padding", parse_color(PADDING)),
                ("content", parse_color(CONTENT)),
                ("border-t", parse_color(BORDER_T)),
                ("border-r", parse_color(BORDER_R)),
                ("border-b", parse_color(BORDER_B)),
                ("border-l", parse_color(BORDER_L)),
            ];
            for (label, token) in want {
                assert!(
                    result.instances.iter().any(|i| color_close(i.color, token)),
                    "{label} token missing from paint instances (token={token:?})"
                );
            }
        });
    });
}
