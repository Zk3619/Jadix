use super::*;
use crate::core::reactivity::create_root;

#[test]
fn progress_empty_id_never_writes_empty_template() {
    let el: Element = progress().default_value(0.25).into();
    let tid = el.template_id.as_deref().unwrap_or("");
    assert_ne!(tid, "");
    assert!(!tid.is_empty() || el.template_id.is_none());
}

#[test]
fn progress_empty_id_resolves_nonempty_instance_attr() {
    let el: Element = progress().into();
    assert_ne!(el.template_id.as_deref(), Some(""));
}

#[test]
fn builder_track_with_marks_slot() {
    let (_owner, el): (_, Element) = create_root(|| {
        progress()
            .id("prog-track-chrome")
            .default_value(0.4)
            .track_with(|t| t.class("h-3"))
            .into()
    });
    fn find_track(el: &Element) -> bool {
        el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_TRACK)
            || el.children.iter().any(find_track)
    }
    assert!(
        find_track(&el),
        "injected track must carry data-slot=progress-track"
    );
    fn track_h(el: &Element) -> Option<f32> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_TRACK) {
            match crate::layout::dim_kind(el.style.size.height) {
                crate::layout::DimKind::Length(px) => return Some(px),
                _ => {}
            }
        }
        el.children.iter().find_map(track_h)
    }
    let h = track_h(&el).expect("track height");
    assert!(
        (h - 0.75 * 16.0).abs() < 0.5,
        "track_with class h-3 must override default 0.5rem, got {h}"
    );
}

#[test]
fn progress_circle_class_lands_on_root() {
    let el: Element = progress_circle()
        .id("pc-chrome")
        .default_value(0.5)
        .class("opacity-50")
        .into();
    assert!(
        (el.style.opacity - 0.5).abs() < 0.01,
        "progress_circle().class must apply via SlotChrome, opacity={}",
        el.style.opacity
    );
}

fn fill_size_pct(el: &Element) -> Option<f32> {
    if el
        .template_id
        .as_deref()
        .is_some_and(|t| t.ends_with("__fill"))
    {
        match crate::core::tree::dim_kind(el.style.size.width) {
            crate::core::tree::DimKind::Percent(p) if p > 0.0 => return Some(p),
            _ => {}
        }
        match crate::core::tree::dim_kind(el.style.size.height) {
            crate::core::tree::DimKind::Percent(p) if p > 0.0 => return Some(p),
            _ => {}
        }
    }
    el.children.iter().find_map(fill_size_pct)
}

fn has_track_view_island(el: &Element) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|t| t.contains("__progress_track_view"))
        || el.children.iter().any(has_track_view_island)
}

fn progress_attr(el: &Element) -> Option<String> {
    if let Some(v) = el.attrs.get("jadix_progress") {
        return Some(v.clone());
    }
    el.children.iter().find_map(progress_attr)
}

#[test]
fn progress_track_value_set_patches_fill_without_island() {
    use crate::app::reactive::reset_registered_anchors;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, take_flushed_effects};
    use std::cell::RefCell;
    use std::rc::Rc;

    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let tree_for_assert = Rc::clone(&tree);
    let (_owner, v) = create_root(|| {
        let v = use_signal("pt-fill-val", || 0.25f32);
        let tree = Rc::clone(&tree);
        create_effect({
            let v = v;
            move || {
                let el: Element = progress().id("pt-fill").value_signal(v).into();
                tree.borrow_mut().children = vec![el];
            }
        });
        v
    });

    assert!(
        !has_track_view_island(&tree_for_assert.borrow()),
        "linear track must not mount a reactive_view placeholder"
    );
    let before = fill_size_pct(&tree_for_assert.borrow()).expect("fill");
    assert!((before - 0.25).abs() < 1e-4, "initial fill {before}");
    assert_eq!(
        progress_attr(&tree_for_assert.borrow()).as_deref(),
        Some("25.0%")
    );

    let _ = take_flushed_effects();
    v.set(0.8);
    let flushed = take_flushed_effects();
    assert!(
        flushed
            .iter()
            .any(|(_, phase)| *phase == crate::core::reactivity::EffectPhase::Render),
        "value.set must run fill bind Render effects, got {flushed:?}"
    );
    assert!(
        !has_track_view_island(&tree_for_assert.borrow()),
        "value.set must not grow a track island"
    );
    let after = fill_size_pct(&tree_for_assert.borrow()).expect("fill after set");
    assert!(
        (after - 0.8).abs() < 1e-4,
        "fill must patch in place, got {after}"
    );
    assert_eq!(
        progress_attr(&tree_for_assert.borrow()).as_deref(),
        Some("80.0%")
    );
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn builder_label_style() {
    let (_owner, el): (_, Element) = create_root(|| {
        progress()
            .id("prog-label-chrome")
            .child(|| {
                progress_label()
                    .style(|b| b.text_size(20.0))
                    .text("Uploading")
                    .into()
            })
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "progress_label .style(|b| b.text_size(20)) must land on the label tree"
    );
}

#[test]
fn scope_exposes_template() {
    let (_owner, el): (_, Element) = create_root(|| {
        progress()
            .id("prog-scope")
            .value_signal(use_signal("prog-scope-val", || 0.5f32))
            .into()
    });
    assert_eq!(el.template_id.as_deref(), Some("prog-scope__progress"));
}

#[test]
fn labeled_scope_keeps_context() {
    let (_owner, el): (_, Element) = create_root(|| {
        progress().id("prog-lbl").default_value(0.3).child(|| {
            ProgressLabel(ProgressLabelProps {
                children: Some(Element::new().text("Load")),
                base: None,
            })
        })
    });
    assert_eq!(el.template_id.as_deref(), Some("prog-lbl__progress"));
}

#[test]
fn circle_size_table_matches_solid_ui() {
    assert_eq!(ProgressCircleSize::Xs.radius(), 15.0);
    assert_eq!(ProgressCircleSize::Md.stroke_width(), 6.0);
    assert_eq!(ProgressCircleSize::Xl.radius(), 80.0);
    assert_eq!(ProgressCircleSize::parse("LG"), ProgressCircleSize::Lg);
}

#[test]
fn circle_track_is_a_single_alpha_ring_without_a_filled_center() {
    let leaf = progress_circle_svg(0.0, ProgressCircleSize::Md, None, None, false);
    let commands = decode_path_d(&leaf.attrs[ATTR_PATH_D]);
    let pixels = crate::primitives::path::rasterize_layers_rgba(
        &[(commands, PathStyle::fill([1.0, 1.0, 1.0, 0.25]))],
        &[],
        0.0,
        64,
        64,
        4,
    );
    assert_eq!(
        pixels[(32 * 64 + 32) * 4 + 3],
        0,
        "track must leave the center transparent"
    );
    let mut alphas = Vec::new();
    for sample in 0..360 {
        let angle = sample as f32 * std::f32::consts::TAU / 360.0;
        let x = (32.0 + 29.0 * angle.cos()).floor() as usize;
        let y = (32.0 + 29.0 * angle.sin()).floor() as usize;
        alphas.push(pixels[(y * 64 + x) * 4 + 3]);
    }
    assert!(
        alphas.iter().all(|alpha| (62..=65).contains(alpha)),
        "nonuniform track alpha: {alphas:?}"
    );
}

#[test]
fn circle_arc_d_quarter_and_full() {
    let q = progress_circle_arc_d(32.0, 32.0, 29.0, 0.25).unwrap();
    assert!(q.starts_with("M 32 "));
    assert!(q.contains(" A 29 29 "));
    let full = progress_circle_arc_d(10.0, 10.0, 8.0, 1.0).unwrap();
    assert_eq!(full.matches(" A ").count(), 2);
    assert!(progress_circle_arc_d(10.0, 10.0, 8.0, 0.0).is_none());
}

#[test]
fn circle_builder_sync_svg_when_untracked() {
    let el: Element = progress_circle().id("pc").default_value(0.6).into();
    assert_eq!(
        el.attrs.get("jadix_progress").map(String::as_str),
        Some("60%")
    );
    assert!(
        el.get_attr(crate::primitives::path::ATTR_PATH_SCENE)
            .is_some(),
        "untracked circle must compile to a raster scene"
    );
}

#[test]
fn circle_registers_path_scene() {
    let el = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, true);
    assert!(
        el.get_attr(crate::primitives::path::ATTR_PATH_SCENE)
            .is_some(),
        "progress circle must compile to a path scene"
    );
}

#[test]
fn circle_layout_paints() {
    use crate::layout::layout_tree::LayoutTree;

    let leaf = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, true);
    let mut root = Element::new().w(200.0).h(200.0).child(leaf);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    assert!(
        result
            .textured_draws
            .iter()
            .any(|d| d.rect[2] > 30.0 && d.rect[3] > 30.0),
        "progress circle must raster with non-zero bounds; draws={:?}",
        result.textured_draws
    );
}

#[test]
fn circle_mounts_after_effect() {
    use crate::app::reactive::reset_registered_anchors;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::create_effect;
    use std::cell::RefCell;
    use std::rc::Rc;

    reset_registered_anchors();
    clear_test_reconcile_root();
    let tree: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));
    let tree_for_assert = Rc::clone(&tree);

    let (_owner, _): (_, ()) = create_root(|| {
        create_effect(move || {
            let pc = progress_circle()
                .id("pc-mount")
                .default_value(0.45)
                .size(ProgressCircleSize::Md)
                .into();
            tree.borrow_mut().children = vec![pc];
        });
    });

    fn has_progress_attr(el: &Element) -> bool {
        el.attrs.contains_key("jadix_progress") || el.children.iter().any(has_progress_attr)
    }
    assert!(
        has_progress_attr(&tree_for_assert.borrow()),
        "bind_prop must ship SVG scene on first mount"
    );
    clear_test_reconcile_root();
}

#[test]
fn circle_bind_prop_ships_scene_immediately() {
    let (_owner, el): (_, Element) =
        create_root(|| progress_circle().id("pc").default_value(0.5).into());
    assert_eq!(el.template_id.as_deref(), Some("pc__view"));
    assert!(
        el.get_attr(crate::primitives::path::ATTR_PATH_SCENE)
            .is_some(),
        "bind_prop initial apply must register path scene"
    );
}

#[test]
fn circle_value_signal_clamps() {
    let (_owner, el): (_, Element) = create_root(|| {
        let v = use_signal("pc-clamp", || 1.5f32);
        progress_circle().id("pc-clamp").value_signal(v).into()
    });
    fn progress_pct(el: &Element) -> Option<&str> {
        el.attrs
            .get("jadix_progress")
            .map(String::as_str)
            .or_else(|| {
                el.children
                    .first()
                    .and_then(|c| c.attrs.get("jadix_progress").map(String::as_str))
            })
    }
    assert_eq!(progress_pct(&el), Some("100%"));
}

#[test]
fn vertical_progress_sets_orientation_attr() {
    let el: Element = progress().orientation("vertical").default_value(0.5).into();
    assert_eq!(
        el.attrs.get(ATTR_ORIENTATION).map(String::as_str),
        Some("vertical")
    );
}

#[test]
fn vertical_scope_uses_default_height() {
    let (_owner, el): (_, Element) = create_root(|| {
        progress()
            .id("prog-vert")
            .orientation(ProgressOrientation::Vertical)
            .default_value(0.4)
            .into()
    });
    assert_eq!(
        el.attrs.get(ATTR_ORIENTATION).map(String::as_str),
        Some("vertical")
    );
    assert!(
        el.children.len() >= 1,
        "vertical progress should include track"
    );
}
