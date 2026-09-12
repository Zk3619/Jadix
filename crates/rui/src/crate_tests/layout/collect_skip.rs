//! Paint-only collect can replay a fingerprint display list for clean sibling subtrees
//! (GPUI `Entity::cached` analog — skip collect, not omit pixels).

use crate::app::frame_stats::{last_skipped_siblings, reset_collect_skip_counters};
use crate::components::input::{focus_id_for_input, input, input_value_signal_key};
use crate::components::input_adornment::input_adornment;
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::core::reactivity::use_signal;
use crate::core::tree::{sync_reactive_elements_subtree, Element, LayoutStatus, LayoutTree};
use crate::layout::collect_cache::collect_skip_allowed_for_frame;
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::primitives::r#box::hstack;
use crate::renderer::font_loader;
use crate::style::sync_class_variants_subtree;
use crate::text::install_shared_font_system;

fn two_column_page() -> Element {
    let col = |bg: [f32; 4], label: &str| {
        Element::new()
            .flex_col()
            .flex_shrink(0.0)
            .w(80.0)
            .h(80.0)
            .child(Element::new().w(80.0).h(40.0).flex_shrink(0.0).bg(bg))
            .child(Element::new().w(80.0).h(40.0).flex_shrink(0.0).text(label))
    };
    Element::new()
        .flex_row()
        .w(160.0)
        .h(80.0)
        .child(col([1.0, 0.0, 0.0, 1.0], "left"))
        .child(col([0.0, 0.0, 1.0, 1.0], "right"))
}

#[test]
fn paint_only_skips_clean_sibling_subtree() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    assert!(
        first.instances.len() >= 2,
        "both columns must paint on the first collect"
    );
    assert_eq!(tree.last_collect_skip_counts(), (0, 0));

    // Full rebuild does not clone the DL. One allowed collect warms `prev`.
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert_eq!(
        warmed.instances.len(),
        first.instances.len(),
        "warm collect must still emit both columns"
    );

    root.children[0].children[1].text = Some("LEFT".into());
    let second = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let (skipped, _patched) = tree.last_collect_skip_counts();
    assert!(
        skipped >= 1,
        "clean right column must replay last-frame ops (skipped={skipped})"
    );
    assert_eq!(
        second.instances.len(),
        first.instances.len(),
        "skip replays ops; FullClear must not drop sibling pixels"
    );
    assert!(last_skipped_siblings() >= 1);

    let third = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "replayed subtrees must stay in the fingerprint map"
    );
    assert_eq!(third.instances.len(), first.instances.len());
}

#[test]
fn paint_only_text_change_must_emit_new_glyphs_not_replay_old() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);

    root.children[0].children[1].text = Some("LEFT".into());
    let second = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        second.text_nodes.iter().any(|t| t.text.contains("LEFT")),
        "StyleChanged bind_text must recollect new glyphs, not replay the previous string; got {:?}",
        second
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !second.text_nodes.iter().any(|t| t.text == "left"),
        "stale caption must not remain in the display list after a paint-only text patch"
    );
}

/// Sidebar Header / flex-1 Content / Footer: maximize grows the column but the footer
/// box size is unchanged. Skip geom used to key only parent origin + size, so the
/// footer display list stayed at the pre-maximize Y (Gallery Send after maximize).
#[test]
fn flex_column_resize_must_not_replay_footer_at_old_y() {
    reset_collect_skip_counters();
    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .h("100%")
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("HeaderChrome")),
        )
        .child(
            Element::new()
                .w("100%")
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .min_h(0.0)
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(Element::new().text("NavItem")),
        )
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("FooterChrome")),
        );

    let mut tree = LayoutTree::new();
    let short = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);

    let footer_short = short
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer text at 200h");
    assert!(
        footer_short.y + footer_short.height > 150.0,
        "footer must sit near the bottom at 200h, got y={}",
        footer_short.y
    );

    let tall = tree.build_and_layout_fast(&mut root, 200.0, 400.0, [-1.0, -1.0], false, false);
    let footer_tall = tall
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer text at 400h");
    assert!(
        footer_tall.y > footer_short.y + 80.0,
        "maximize must recollect Footer at the new bottom, not replay the 200h Y\n\
         short_y={} tall_y={}",
        footer_short.y,
        footer_tall.y
    );
    assert!(
        footer_tall.y + footer_tall.height > 350.0,
        "footer must sit near the 400h bottom, got y={}",
        footer_tall.y
    );

    let restored = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let footer_restored = restored
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer text at restored 200h");
    assert!(
        (footer_restored.y - footer_short.y).abs() < 2.0,
        "restore/minimize must recollect Footer at the short bottom, not keep the 400h Y\n\
         short_y={} restored_y={}",
        footer_short.y,
        footer_restored.y
    );
}

/// AppState live resize is `Clean` + `scroll_only`. Viewport change must still
/// recompute flex locations (maximize flake / minimize-stuck).
#[test]
fn scroll_only_resize_must_reflow_flex_footer() {
    reset_collect_skip_counters();
    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .h("100%")
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("HeaderChrome")),
        )
        .child(
            Element::new()
                .w("100%")
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .min_h(0.0)
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(Element::new().text("NavItem")),
        )
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("FooterChrome")),
        );

    let mut tree = LayoutTree::new();
    let short = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    let footer_short = short
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer at 200h");

    let tall = tree.build_and_layout_fast(&mut root, 200.0, 400.0, [-1.0, -1.0], true, false);
    let footer_tall = tall
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer at 400h scroll_only");
    assert!(
        footer_tall.y > footer_short.y + 80.0,
        "scroll_only maximize must still compute Footer Y\nshort_y={} tall_y={}",
        footer_short.y,
        footer_tall.y
    );

    let restored = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], true, false);
    let footer_restored = restored
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer at restored scroll_only");
    assert!(
        (footer_restored.y - footer_short.y).abs() < 2.0,
        "scroll_only restore must compute Footer back to the short bottom\n\
         short_y={} restored_y={}",
        footer_short.y,
        footer_restored.y
    );
}

#[test]
fn collect_skip_is_paint_only() {
    use LayoutStatus::*;
    assert!(collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        false,
        false,
        false
    ));
    assert!(collect_skip_allowed_for_frame(
        Clean, false, false, false, false
    ));
    assert!(
        !collect_skip_allowed_for_frame(LayoutChanged, false, false, false, false),
        "maximize / settle must recollect; cached ops are absolute"
    );
    assert!(!collect_skip_allowed_for_frame(
        StructureChanged,
        false,
        false,
        false,
        false
    ));
    assert!(
        !collect_skip_allowed_for_frame(StyleChanged, true, false, false, false),
        "smooth scroll must not fingerprint-replay"
    );
    assert!(!collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        true,
        false,
        false
    ));
    assert!(!collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        false,
        true,
        false
    ));
    assert!(!collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        false,
        false,
        true
    ));
}

#[test]
fn paint_only_scroll_only_replays_clean_siblings() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    let second = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "idle ViewportsOnly collect is paint-only; fingerprint skip must replay clean siblings"
    );
    assert_eq!(warmed.instances.len(), first.instances.len());
    assert_eq!(second.instances.len(), first.instances.len());
}

#[test]
fn skip_disallowed_frame_drops_stale_absolute_dls() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(tree.last_collect_skip_counts().0 >= 1);

    tree.set_collect_skip_allowed(false);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert_eq!(tree.last_collect_skip_counts().0, 0);

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "skip-off must drop prev so the next paint-only frame cannot replay pre-layout DLs"
    );
}

const SPIN_BG: [f32; 4] = [0.91, 0.12, 0.88, 1.0];

fn header_spinner_page() -> Element {
    Element::new()
        .flex_col()
        .w(200.0)
        .h("100%")
        .child(
            Element::new().w("100%").h(40.0).flex_shrink(0.0).child(
                Element::new()
                    .w(16.0)
                    .h(16.0)
                    .flex_shrink(0.0)
                    .bg(SPIN_BG)
                    .child(Element::new().w(1.0).h(1.0)),
            ),
        )
        .child(
            Element::new()
                .w("100%")
                .flex_grow(1.0)
                .min_h(0.0)
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(Element::new().text("NavItem")),
        )
}

fn spinner_instance_transform(result: &crate::layout::layout_tree::RenderResult) -> [f32; 6] {
    result
        .instances
        .iter()
        .find(|i| {
            (i.color[0] - SPIN_BG[0]).abs() < 0.02
                && (i.color[1] - SPIN_BG[1]).abs() < 0.02
                && (i.color[2] - SPIN_BG[2]).abs() < 0.02
                && i.rect_size[0] >= 8.0
                && i.rect_size[0] <= 24.0
        })
        .map(|i| i.transform)
        .expect("spinner instance")
}

fn transform_is_identity(m: &[f32; 6]) -> bool {
    let id = crate::style::MAT2D_IDENTITY;
    m.iter()
        .zip(id.iter())
        .all(|(a, b)| (*a - *b).abs() <= 1e-4)
}

/// LayoutTree-level: even if skip is wrongly left on across a grow, viewport
/// invalidate plus geom origin must not replay the rest-pose spinner.
/// keeps the same box; hashing only origin/size replayed the rest-pose DL and froze
/// `animate-spin` after the window grew.
#[test]
fn collect_skip_must_not_freeze_rotate_after_maximize() {
    reset_collect_skip_counters();
    let mut root = header_spinner_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let rest = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    assert!(
        transform_is_identity(&spinner_instance_transform(&rest)),
        "warm collect is rest pose"
    );

    root.children[0].children[0].style.rotate_deg = 45.0;
    let maxed = tree.build_and_layout_fast(&mut root, 200.0, 400.0, [-1.0, -1.0], false, false);
    let xf = spinner_instance_transform(&maxed);
    assert!(
        !transform_is_identity(&xf),
        "maximize LayoutChanged must recollect the 45° spin, not replay the rest-pose DL\n\
         transform={xf:?}"
    );
}

#[test]
fn collect_skip_must_not_replay_rotated_sibling() {
    reset_collect_skip_counters();
    let mut root = header_spinner_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);

    root.children[0].children[0].style.rotate_deg = 90.0;
    let spun = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let xf = spinner_instance_transform(&spun);
    assert!(
        !transform_is_identity(&xf),
        "paint-hash skip must bust when rotate_deg changes, got {xf:?}"
    );
}

#[test]
fn viewport_resize_drops_skip_cache() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "warm skip cache must replay a clean sibling"
    );

    let _ = tree.build_and_layout_fast(&mut root, 160.0, 240.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "maximize must drop the fingerprint DL; replaying absolute glyphs stacks the old Y"
    );
}

#[test]
fn collect_skip_must_not_replay_shimmer_box() {
    reset_collect_skip_counters();
    let mut box_el = Element::new()
        .w(200.0)
        .h(40.0)
        .bg([0.9, 0.9, 0.9, 1.0])
        .child(Element::new().text("shimmer-box overlay on container"));
    crate::style::apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1000");
    let mut root = Element::new().w(200.0).h(80.0).child(box_el).child(
        Element::new()
            .w(200.0)
            .h(40.0)
            .child(Element::new().text("next-section")),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], false, false);
    let second = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        root.children[0].collect_skip_live,
        "shimmer-box subtree must be marked live so skip cannot freeze the sheen"
    );
    assert_eq!(
        second
            .text_nodes
            .iter()
            .filter(|n| n.text.contains("shimmer-box overlay"))
            .count(),
        1,
        "label must paint once, not a skip-ghost plus a live collect"
    );
}

#[test]
fn maximize_must_not_ghost_shimmer_label() {
    reset_collect_skip_counters();
    let mut box_el = Element::new()
        .w(200.0)
        .h(40.0)
        .bg([0.9, 0.9, 0.9, 1.0])
        .child(Element::new().text("shimmer-box overlay on container"));
    crate::style::apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1000");
    let mut root = Element::new().flex_col().w(200.0).child(box_el).child(
        Element::new()
            .w(200.0)
            .h(40.0)
            .child(Element::new().text("1c-msg) Message")),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], false, false);
    let grown = tree.build_and_layout_fast(&mut root, 200.0, 240.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "viewport grow must drop skip cache"
    );
    let labels: Vec<_> = grown
        .text_nodes
        .iter()
        .filter(|n| n.text.contains("shimmer-box overlay"))
        .collect();
    assert_eq!(
        labels.len(),
        1,
        "maximize must not leave a skip-ghost copy of the shimmer label"
    );
    let msg_y = grown
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-msg"))
        .map(|n| n.y)
        .expect("message heading");
    assert!(
        labels[0].y + 8.0 < msg_y,
        "shimmer label y={} must stay above the next section y={msg_y}, not overlay it",
        labels[0].y
    );
}

#[test]
fn collect_skip_must_not_replay_animate_spin() {
    reset_collect_skip_counters();
    let mut spin = Element::new()
        .w(16.0)
        .h(16.0)
        .flex_shrink(0.0)
        .bg(SPIN_BG)
        .child(Element::new().w(1.0).h(1.0));
    crate::style::apply_class_list(&mut spin, "animate-spin");
    let mut root = Element::new().w(80.0).h(40.0).child(
        Element::new()
            .w(80.0)
            .h(40.0)
            .child(spin)
            .child(Element::new().text("idle")),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 40.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 40.0, [-1.0, -1.0], false, false);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 40.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "animate-spin subtrees must recollect; skip is why maximize froze until scroll"
    );
}

#[test]
fn css_spin_must_keep_rotating_after_maximize_skip() {
    reset_collect_skip_counters();
    crate::style::utility::set_frame_dt(1.0 / 30.0);
    let mut spin = Element::new()
        .w(16.0)
        .h(16.0)
        .flex_shrink(0.0)
        .bg(SPIN_BG)
        .child(Element::new().w(1.0).h(1.0));
    crate::style::apply_class_list(&mut spin, "animate-spin");
    let mut root = Element::new().w(80.0).h("100%").child(spin);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 80.0, [-1.0, -1.0], false, false);

    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        80.0,
        240.0,
        &mut tree,
        &mut impact,
    );
    let a = tree.build_and_layout_fast(&mut root, 80.0, 240.0, [-1.0, -1.0], false, false);
    let xf_a = spinner_instance_transform(&a);

    crate::style::sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        80.0,
        240.0,
        &mut tree,
        &mut impact,
    );
    let b = tree.build_and_layout_fast(&mut root, 80.0, 240.0, [-1.0, -1.0], false, false);
    let xf_b = spinner_instance_transform(&b);
    assert!(
        xf_a.iter()
            .zip(xf_b.iter())
            .any(|(x, y)| (*x - *y).abs() > 1e-4),
        "CSS spin must advance on post-maximize skip frames, got {xf_a:?} then {xf_b:?}"
    );
}

/// Gallery Kbd Search: focus is paint-only. If the Input subtree's fingerprint
/// ignores `input_caret`, collect-skip replays the unfocused DL (no hairline)
/// while dest-reset wipes the box — caret gone, inner fill looks nested.
#[test]
fn focused_kbd_input_must_not_replay_unfocused_chrome() {
    reset_collect_skip_counters();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "skip-kbd-search";
    use_signal(&input_value_signal_key(id), String::new).set(String::new());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut root = Element::new()
        .w(400.0)
        .h(80.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(
            hstack().w("100%").h(36.0).px(12.0).items("center").child(
                input_adornment()
                    .child(input(id).placeholder("Search…").plain_chrome(true))
                    .addon("inline-end", Element::new().text("⌘K")),
            ),
        );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let unfocused = tree.build_and_layout_fast(&mut root, 400.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        unfocused.input_chrome_instances.is_empty(),
        "unfocused field must not emit a caret hairline"
    );

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 80.0, [0.0, 0.0]);
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        400.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    fn tree_has_input_caret(el: &Element) -> bool {
        el.text_style.input_caret.is_some() || el.children.iter().any(tree_has_input_caret)
    }
    fn tree_has_paint_sync(el: &Element) -> bool {
        el.input_paint_sync.is_some() || el.children.iter().any(tree_has_paint_sync)
    }
    assert!(
        tree_has_paint_sync(&root),
        "shell must contain the Search Input paint-sync node"
    );
    assert!(
        tree_has_input_caret(&root),
        "sync must install input_caret on the focused field before collect"
    );
    let focused = tree.build_and_layout_fast(&mut root, 400.0, 80.0, [-1.0, -1.0], false, false);

    let has_caret_draw = focused
        .text_nodes
        .iter()
        .any(|n| n.style.input_caret.is_some());
    assert!(
        has_caret_draw,
        "focus collect must keep input_caret on text nodes (skip must not replay the unfocused leaf)"
    );
    assert!(
        !focused.input_chrome_instances.is_empty(),
        "focused Kbd Input must collect caret chrome on a skip-allowed StyleChanged frame; \
         fingerprint skip replayed the unfocused DL (no hairline)"
    );
    assert!(
        focused.node_paints.iter().any(|p| p.chrome.0 < p.chrome.1),
        "some collect slot must own the caret chrome range so node-present can redraw it"
    );
}

/// Portal overlay_split is a walk-time cursor, not part of the cached DL.
/// Skipping an ancestor of `__rui_portal_layer` splices rects/glyphs but leaves
/// `overlay_split` unmarked → finish_collect parks the split at `len` (empty
/// overlay suffix). Present then dest-resets the dialog hole with no overlay
/// redraw: first frame looks right, later frames lose scrim + panel fill.
fn page_under_portal_overlay() -> Element {
    let page = Element::new()
        .w(400.0)
        .h(300.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(Element::new().text("SheetSlide · Builder"));
    let mut overlay = Element::new()
        .w(400.0)
        .h(300.0)
        .bg([0.0, 0.0, 0.0, 0.28])
        .backdrop_blur(2.0)
        .child(
            Element::new()
                .w(200.0)
                .h(80.0)
                .bg([1.0, 1.0, 1.0, 1.0])
                .rounded(12.0)
                .child(Element::new().text("ViewportModal · RSX")),
        );
    overlay.set_attr("rui_portal", "1");
    overlay.set_attr(ATTR_PORTAL_KEY, "vm-scrim");
    Element::new().w(400.0).h(300.0).child(page).child(overlay)
}

fn assert_portal_overlay_split_keeps_chrome(result: &crate::layout::layout_tree::RenderResult) {
    assert!(
        result.overlay_split.instance_start > 0
            && result.overlay_split.instance_start < result.instances.len(),
        "portal layer must split after page instances (not 0 = all overlay, not len = empty overlay); \
         start={} n={}",
        result.overlay_split.instance_start,
        result.instances.len()
    );
    let split_t = result.overlay_split.text_start.min(result.text_nodes.len());
    assert!(
        result.text_nodes[..split_t]
            .iter()
            .any(|t| t.text.contains("SheetSlide")),
        "page label must collect before the overlay split; texts={:?}",
        result
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    let overlay_inst = &result.instances[result.overlay_split.instance_start..];
    assert!(
        overlay_inst
            .iter()
            .any(|i| i.rect_size[0] > 300.0 && i.color[3] > 0.05),
        "scrim missing in overlay suffix; overlay={:?}",
        overlay_inst
            .iter()
            .map(|i| (i.rect_size, i.color[3]))
            .collect::<Vec<_>>()
    );
    assert!(
        overlay_inst
            .iter()
            .any(|i| i.rect_size[0] > 100.0 && i.rect_size[0] < 280.0 && i.color[3] > 0.5),
        "opaque panel fill missing in overlay suffix; overlay={:?}",
        overlay_inst
            .iter()
            .map(|i| (i.rect_size, i.color[3]))
            .collect::<Vec<_>>()
    );
}

#[test]
fn paint_only_skip_must_not_drop_portal_overlay_split() {
    reset_collect_skip_counters();
    let mut root = page_under_portal_overlay();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
    assert_portal_overlay_split_keeps_chrome(&first);

    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 400.0, 300.0, [-1.0, -1.0], false, false);
    assert_portal_overlay_split_keeps_chrome(&warmed);

    let skipped = tree.build_and_layout_fast(&mut root, 400.0, 300.0, [-1.0, -1.0], false, false);
    assert_portal_overlay_split_keeps_chrome(&skipped);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "page sibling must still fingerprint-replay; only portal-containing ancestors stay live"
    );
    let split_t = skipped
        .overlay_split
        .text_start
        .min(skipped.text_nodes.len());
    assert!(
        skipped.text_nodes[split_t..]
            .iter()
            .any(|t| t.text.contains("ViewportModal")),
        "dialog title must stay in the overlay text suffix after skip replay"
    );
}
