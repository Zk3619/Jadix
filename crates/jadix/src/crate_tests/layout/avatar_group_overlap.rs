#![cfg(feature = "product-tests")]
//! Collapsed AvatarGroup A/B/C dest-reset vs the pairwise overlap crescents.
//!
//! Gallery: 32px faces, `GROUP_OVERLAP` -8px. A later face's AABB dest-reset
//! must not CLEAR pixels that sit in an earlier disk and outside the later
//! disk — that hole is the overlay-close flash at the stack overlap.

use crate::app::damage::{intersect_rects, overlay_restore_replay_rank};
use crate::app::draw_cull::node_present_dest_reset_full;
use crate::app::reactive::reset_registered_anchors;
use crate::components::avatar::{
    AvatarGroup, AvatarGroupCount, AvatarGroupCountProps, AvatarGroupProps,
};
use crate::core::reactivity::create_root;
use crate::core::tree::{Element, IntoElementList, LayoutTree};
use crate::layout::layout_tree::{hoist_portal_subtrees, NodePaint};
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn avatar_with_fallback(label: &str) -> Element {
    crate::components::avatar::avatar().fallback(label).into()
}

fn build_stacked_abc_row() -> Element {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    frag = frag.children(vec![
        avatar_with_fallback("A"),
        avatar_with_fallback("B"),
        avatar_with_fallback("C"),
        AvatarGroupCount(AvatarGroupCountProps {
            children: Some(Element::new().text("+3")),
            ..Default::default()
        }),
    ]);
    let mut page = Element::new().w(400.0).h(120.0).bg([0.96, 0.96, 0.96, 1.0]);
    for kid in AvatarGroup(AvatarGroupProps {
        children: Some(frag),
        ..Default::default()
    })
    .into_element_list()
    {
        page = page.child(kid);
    }
    page
}

fn fmt_rect(r: [f32; 4]) -> String {
    format!("[{:.1},{:.1} {:.1}×{:.1}]", r[0], r[1], r[2], r[3])
}

fn point_in_rect(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[1] >= r[1] && p[0] < r[0] + r[2] && p[1] < r[1] + r[3]
}

fn point_in_disk(rest: [f32; 4], p: [f32; 2]) -> bool {
    let cx = rest[0] + rest[2] * 0.5;
    let cy = rest[1] + rest[3] * 0.5;
    let rad = rest[2].min(rest[3]) * 0.5;
    let dx = p[0] - cx;
    let dy = p[1] - cy;
    dx * dx + dy * dy <= rad * rad
}

/// Pixel of `earlier`'s disk that sits in `wipe` but outside `later`'s disk.
fn dest_reset_punches_earlier_disk(
    wipe: [f32; 4],
    earlier: [f32; 4],
    later: [f32; 4],
) -> Option<[f32; 2]> {
    let hit = intersect_rects(wipe, earlier)?;
    let mut y = hit[1] + 0.5;
    while y < hit[1] + hit[3] {
        let mut x = hit[0] + 0.5;
        while x < hit[0] + hit[2] {
            let p = [x, y];
            if point_in_rect(wipe, p) && point_in_disk(earlier, p) && !point_in_disk(later, p) {
                return Some(p);
            }
            x += 2.0;
        }
        y += 2.0;
    }
    None
}

fn chip_slots(paints: &[NodePaint]) -> Vec<&NodePaint> {
    let mut chips: Vec<&NodePaint> = paints
        .iter()
        .filter(|p| {
            !p.overlay
                && !p.layered
                && crate::app::damage::is_rest_stack_chip(p.rest.unwrap_or(p.bounds))
        })
        .collect();
    chips.sort_by(|a, b| {
        a.bounds[0]
            .partial_cmp(&b.bounds[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.bounds[2]
                    .partial_cmp(&b.bounds[2])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .reverse(),
            )
    });
    chips.dedup_by(|a, b| a.key == b.key);
    chips
}

#[test]
fn collapsed_abc_later_face_dest_reset_must_not_punch_earlier_crescent() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let (_owner, mut root) = create_root(build_stacked_abc_row);
    let mut tree = LayoutTree::new();
    let _ = hoist_portal_subtrees(&mut root);
    let result = tree.build_and_layout(&mut root, 400.0, 120.0, [-1.0, -1.0]);

    let chips = chip_slots(&result.node_paints);
    assert!(
        chips.len() >= 3,
        "need A/B/C chips, got {} paints={}",
        chips.len(),
        result
            .node_paints
            .iter()
            .map(|p| format!(
                "k{} bounds={} rest={:?}",
                p.key,
                fmt_rect(p.bounds),
                p.rest.map(fmt_rect)
            ))
            .collect::<Vec<_>>()
            .join(" | ")
    );

    let dump = chips
        .iter()
        .map(|p| {
            format!(
                "k{} bounds={} rest={:?} inst={:?}",
                p.key,
                fmt_rect(p.bounds),
                p.rest.map(fmt_rect),
                p.instance
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let hole = chips.iter().fold(chips[0].bounds, |acc, p| {
        crate::app::damage::union_rects(acc, p.bounds)
    });
    let ranks: Vec<String> = result
        .node_paints
        .iter()
        .filter_map(|p| {
            overlay_restore_replay_rank(p.bounds, hole)
                .map(|r| format!("rank{r} k{} {}", p.key, fmt_rect(p.bounds)))
        })
        .collect();

    let mut punches = Vec::new();
    for later_i in 1..chips.len() {
        let later = chips[later_i];
        let Some(reset) = node_present_dest_reset_full(
            later.key,
            later.bounds,
            &result.node_paints,
            &result.instances,
            &[],
            &[],
            1.0,
            None,
        ) else {
            continue;
        };
        let later_rest = later.rest.unwrap_or(later.bounds);
        for earlier in chips.iter().take(later_i) {
            let earlier_rest = earlier.rest.unwrap_or(earlier.bounds);
            if let Some(pt) = dest_reset_punches_earlier_disk(reset.rect, earlier_rest, later_rest)
            {
                punches.push(format!(
                    "later k{} wipe={} punches earlier k{} rest={} disk at ({:.1},{:.1})",
                    later.key,
                    fmt_rect(reset.rect),
                    earlier.key,
                    fmt_rect(earlier_rest),
                    pt[0],
                    pt[1]
                ));
            }
        }
    }

    assert!(
        punches.is_empty(),
        "later-face AABB dest-reset punches an earlier disk crescent (overlay-close flash)\n\
         chips:\n{dump}\n\
         overlay ranks:\n{}\n\
         punches:\n{}",
        ranks.join("\n"),
        punches.join("\n")
    );
}
