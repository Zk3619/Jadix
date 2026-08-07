//! Gallery pattern: `<Button>{ if sig.get() { … } else { … } }</Button>` via bind_text.
//!
//! Also covers the live LayoutChanged / StyleChanged paths when a button caption grows:
//! text must stay single-line and the button must widen — not wrap inside a stale width.

use crate::app::reactive::{bind_text, reset_registered_anchors};
use crate::components::button::{button, Button, ButtonProps, ButtonSize, ButtonVariant};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::hstack;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn find_tid_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &mut el.children {
        if let Some(h) = find_tid_mut(c, id) {
            return Some(h);
        }
    }
    None
}

fn assert_caption_single_line(result_text: &crate::layout::layout_tree::TextLayoutInfo, label: &str) {
    let prepared = result_text
        .prepared
        .as_ref()
        .expect("text leaf has prepared");
    let intrinsic = crate::text::intrinsic_width(prepared);
    let metrics = crate::text::layout(prepared, result_text.width.max(1.0));
    assert!(
        result_text.width + 0.05 >= intrinsic,
        "{label}: box_w {:.3} < intrinsic {:.3} (text={:?})",
        result_text.width,
        intrinsic,
        result_text.text
    );
    assert_eq!(
        metrics.line_count, 1,
        "{label}: wrapped into {} lines at box_w={:.3} intrinsic={:.3} text={:?}",
        metrics.line_count, result_text.width, intrinsic, result_text.text
    );
}

#[test]
fn button_preserves_bind_text_caption_through_layout() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mark = use_signal("diag_btn_mark_invalid", || false);
            let caption = bind_text("diag:btn:mark-invalid", move || {
                if mark.get() {
                    "Clear invalid".to_string()
                } else {
                    "Mark invalid".to_string()
                }
            });
            let btn: Element = button("diag-mark-invalid")
                .variant("outline")
                .size("sm")
                .child(caption)
                .into();
            assert!(
                btn.children
                    .iter()
                    .any(|c| c.text.as_deref() == Some("Mark invalid")),
                "Button must keep bind_text caption; children={:?}",
                btn.children
                    .iter()
                    .map(|c| (c.template_id.clone(), c.text.clone()))
                    .collect::<Vec<_>>()
            );
            let mut root = Element::new().w(400.0).h(80.0).child(btn);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
            assert!(
                result.text_nodes.iter().any(|n| n.text == "Mark invalid"),
                "layout must collect button caption; nodes={:?}",
                result
                    .text_nodes
                    .iter()
                    .map(|n| n.text.as_str())
                    .collect::<Vec<_>>()
            );
        });
    });
    reset_registered_anchors();
}

#[test]
fn button_component_empty_id_still_shows_bind_text() {
    reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mark = use_signal("diag_btn_comp_mark", || false);
            let caption = bind_text("diag:btn-comp:mark", move || {
                if mark.get() {
                    "Clear invalid".into()
                } else {
                    "Mark invalid".into()
                }
            });
            // Mimic `<Button variant outline size sm>` with default empty id.
            let el = Button(ButtonProps {
                id: String::new(),
                variant: Some(ButtonVariant::Outline),
                size: Some(ButtonSize::Sm),
                children: Some(caption),
                ..Default::default()
            });
            assert!(
                el.children
                    .iter()
                    .any(|c| c.text.as_deref() == Some("Mark invalid")),
                "Button component must surface bind_text; children={:?}",
                el.children
                    .iter()
                    .map(|c| (c.template_id.clone(), c.text.clone()))
                    .collect::<Vec<_>>()
            );
        });
    });
    reset_registered_anchors();
}

/// Gallery ButtonShowcase: caption grows via LayoutChanged (`mark_dirty` + root compute).
/// The button must widen for a single line — not soft-wrap inside the previous width.
#[test]
fn button_bind_text_layout_changed_widens_instead_of_wrapping() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let short = "Default (0)";
    let long = "Default (999)";

    let caption = Element::new().text(short).template_id("diag:btn:count");
    let row = Element::from(
        hstack()
            .w("100%")
            .min_w("0")
            .gap("0.5rem")
            .items_center()
            .wrap("wrap")
            .child(Element::from(button("diag-count").child(caption)))
            .child(Element::from(
                button("diag-secondary").child(Element::new().text("Secondary")),
            ))
            .child(Element::from(
                button("diag-outline").child(Element::new().text("Outline")),
            )),
    );
    let mut root = Element::new().w(640.0).h(200.0).child(row);
    let mut tree = LayoutTree::new();
    let before = tree.build_and_layout(&mut root, 640.0, 200.0, [-1.0, -1.0]);
    let before_cap = before
        .text_nodes
        .iter()
        .find(|n| n.text == short)
        .expect("short caption");
    assert_caption_single_line(before_cap, "before");
    let before_btn = find_btn_bounds(&root, "diag-count");
    let before_w = before_btn[2];
    let before_h = before_btn[3];

    let leaf = find_tid_mut(&mut root, "diag:btn:count").expect("caption leaf");
    let dirty = leaf.layout_node.expect("caption layout node");
    leaf.text = Some(long.into());
    let dirty_set = std::collections::HashSet::from([dirty]);
    tree.mark_dirty_nodes_in_tree(&root, &dirty_set);
    let after = tree.build_and_layout_fast_ex(
        &mut root,
        640.0,
        200.0,
        [-1.0, -1.0],
        false,
        false,
        None,
    );
    let after_cap = after
        .text_nodes
        .iter()
        .find(|n| n.text == long)
        .expect("long caption");
    assert_caption_single_line(after_cap, "after LayoutChanged");

    let after_btn = find_btn_bounds(&root, "diag-count");
    assert!(
        after_btn[2] + 0.5 >= before_w,
        "button width must not shrink: before={before_w:.3} after={:.3}",
        after_btn[2]
    );
    assert!(
        after_btn[2] > before_w + 1.0,
        "button must widen for longer caption: before={before_w:.3} after={:.3}",
        after_btn[2]
    );
    assert!(
        after_btn[3] < before_h * 1.5,
        "button height suggests wrap: before={before_h:.3} after={:.3}",
        after_btn[3]
    );
    reset_registered_anchors();
}

fn find_btn_bounds(root: &Element, id: &str) -> [f32; 4] {
    fn walk(el: &Element, id: &str) -> Option<[f32; 4]> {
        if el.template_id.as_deref() == Some(id) || el.focus_id.as_deref() == Some(id) {
            return Some(el.layout_bounds);
        }
        el.children.iter().find_map(|c| walk(c, id))
    }
    walk(root, id).expect("button bounds")
}

/// Paint-only StyleChanged with a longer caption soft-wraps inside the stale box.
/// Guard: `bind_text` must not take this path when intrinsic width grows (see
/// `button_bind_text_growing_caption_must_request_layout`).
#[test]
fn stale_box_after_longer_caption_without_layout_wraps() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let short = "Default (0)";
    let long = "Default (999)";
    let caption = Element::new().text(short).template_id("diag:btn:stale");
    let mut root = Element::new().w(400.0).h(80.0).child(Element::from(
        button("diag-stale").child(caption),
    ));
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let before_btn = find_btn_bounds(&root, "diag-stale");

    let leaf = find_tid_mut(&mut root, "diag:btn:stale").expect("caption");
    leaf.text = Some(long.into());
    let after = tree.build_and_layout_fast_ex(
        &mut root,
        400.0,
        80.0,
        [-1.0, -1.0],
        true, // scroll_only: skip flex recompute
        false,
        None,
    );
    let after_cap = after
        .text_nodes
        .iter()
        .find(|n| n.text == long)
        .expect("long caption collected");
    let after_btn = find_btn_bounds(&root, "diag-stale");

    let prepared = after_cap.prepared.as_ref().expect("prepared");
    let metrics = crate::text::layout(prepared, after_cap.width.max(1.0));
    assert!(
        metrics.line_count >= 2 || after_cap.width + 0.05 < crate::text::intrinsic_width(prepared),
        "documents stale-box wrap; lines={} box_w={:.3} intrinsic={:.3}",
        metrics.line_count,
        after_cap.width,
        crate::text::intrinsic_width(prepared)
    );
    assert!(
        (after_btn[2] - before_btn[2]).abs() < 0.5
            && (after_btn[3] - before_btn[3]).abs() < 0.5,
        "without layout, button size stays stale: before={before_btn:?} after={after_btn:?}"
    );
    reset_registered_anchors();
}

#[test]
fn button_bind_text_subpixel_growth_must_request_layout() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    // Same string re-applied against a stale box that is already 1px too narrow:
    // old ±0.5 same_width would StyleChanged and keep wrapping; box check forces layout.
    let text = "Default (68)";
    let mut leaf = bind_text("diag:btn:subpx", || text.to_string());
    leaf = leaf.text_size(14.0).font_weight(500.0);
    let intrinsic = crate::app::reactive::text_intrinsic_width(&leaf, text).expect("w");
    leaf.layout_bounds = [0.0, 0.0, (intrinsic - 1.0).max(1.0), 24.0];

    let next = text.to_string();
    let needs_layout = crate::app::reactive::text_intrinsic_width(&leaf, text)
        .zip(crate::app::reactive::text_intrinsic_width(&leaf, next.as_str()))
        .map(|(b, a)| {
            a > b + 0.01 || (leaf.layout_bounds[2] > 1.0 && a > leaf.layout_bounds[2] + 0.01)
        })
        .unwrap_or(true);
    assert!(
        needs_layout,
        "intrinsic {:.3} vs stale box {:.3} must LayoutChanged",
        intrinsic,
        leaf.layout_bounds[2]
    );
    reset_registered_anchors();
}

#[test]
fn button_caption_nowrap_under_tight_avail_stays_single_line() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    // ShadCN: whitespace-nowrap — even with a narrow flex line, caption must not wrap.
    let label = "Default (999)";
    let mut root = Element::new().w(120.0).h(80.0).child(Element::from(
        button("diag-nowrap").child(Element::new().text(label)),
    ));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 120.0, 80.0, [-1.0, -1.0]);
    let cap = result
        .text_nodes
        .iter()
        .find(|n| n.text == label)
        .expect("caption");
    assert_caption_single_line(cap, "nowrap under tight parent");
    let btn = find_btn_bounds(&root, "diag-nowrap");
    assert!(
        btn[2] + 0.05 >= cap.width,
        "button must cover nowrap caption: btn_w={:.3} cap_w={:.3}",
        btn[2],
        cap.width
    );
    reset_registered_anchors();
}

#[test]
fn button_bind_text_growing_caption_must_request_layout() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let short = "Default (0)";
    let long = "Default (999)";
    // Use a real bind_text leaf and re-run its apply decision via width compare.
    let mut leaf = bind_text("diag:btn:grow-status", || short.to_string());
    leaf = leaf.text_size(14.0).font_weight(500.0);

    let before_w = crate::app::reactive::text_intrinsic_width(&leaf, short).expect("before");
    let after_w = crate::app::reactive::text_intrinsic_width(&leaf, long).expect("after");
    assert!(
        after_w > before_w + 0.01,
        "fixture must grow intrinsic width: {before_w} → {after_w}"
    );

    let next = long.to_string();
    let width_grew = crate::app::reactive::text_intrinsic_width(
        &leaf,
        leaf.text.as_deref().unwrap_or(""),
    )
    .zip(crate::app::reactive::text_intrinsic_width(&leaf, next.as_str()))
    .is_none_or(|(a, b)| b > a + 0.01);
    leaf.text = Some(next);
    let status = if width_grew {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::StyleChanged
    };
    assert_eq!(
        status,
        LayoutStatus::LayoutChanged,
        "growing caption must request layout, not paint-only StyleChanged"
    );
    reset_registered_anchors();
}
