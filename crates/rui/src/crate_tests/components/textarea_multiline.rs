//! Regression: Textarea leaf height must grow with hard `\n` after paint sync.

use crate::components::input::input_value_signal_key;
use crate::components::textarea::textarea;
use crate::components::text_edit::sync_text_edit_paint_subtree;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::text::install_shared_font_system;
use crate::utils::text_field::{
    input_text_template_id, text_edit_ink_pad_px, text_edit_line_box_px,
};

fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(f) = find_tid(c, tid) {
            return Some(f);
        }
    }
    None
}

#[test]
fn textarea_leaf_grows_with_hard_newlines_after_paint_sync() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-measure";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let ink = text_edit_ink_pad_px(size);

    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(&input_value_signal_key(id), String::new).set(String::new());

    let el: Element = textarea(id).rows(4).placeholder("notes").into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let h0 = find_tid(&root, &tid)
        .expect("text leaf")
        .layout_bounds[3];
    assert!(
        h0 > lh * 0.8 && h0 < lh + ink * 2.0 + 4.0,
        "placeholder leaf should be ~1 line + bottom ink (lh={lh} ink={ink}), got {h0}"
    );

    use_signal(&input_value_signal_key(id), String::new).set("aa\nbb".into());
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    assert!(
        matches!(impact, LayoutStatus::LayoutChanged),
        "text change must report LayoutChanged, got {impact:?}"
    );
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], false, false);

    let text_el = find_tid(&root, &tid).expect("text leaf after sync");
    assert_eq!(text_el.text.as_deref(), Some("aa\nbb"));
    let h1 = text_el.layout_bounds[3];
    assert!(
        h1 > lh * 1.8 + ink,
        "leaf height must grow for 2 hard lines (lh={lh}), got h0={h0} h1={h1}; text={:?}",
        text_el.text
    );

    let result = tree.build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], false, false);
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("textarea text TextLayoutInfo");
    assert!(
        info.height > lh * 1.8,
        "TextLayoutInfo.height must be multi-line, got {}",
        info.height
    );
    assert_eq!(info.style.line_height_px, Some(lh));
    let prepared = info.prepared.as_ref().expect("prepared attached");
    let metrics = crate::text::layout(prepared, info.width.max(1.0));
    assert!(
        metrics.line_count >= 2 && metrics.height > lh * 1.8,
        "prepared layout must be >=2 lines, got count={} h={}",
        metrics.line_count,
        metrics.height
    );
}

#[test]
fn textarea_built_with_newlines_has_multi_line_leaf() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-build";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let ink = text_edit_ink_pad_px(size);

    use_signal(&input_value_signal_key(id), String::new).set("aa\nbb\ncc".into());
    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let text_el = find_tid(&root, &tid).expect("text leaf");
    let h = text_el.layout_bounds[3];
    assert_eq!(text_el.text.as_deref(), Some("aa\nbb\ncc"));
    assert!(
        h > lh * 2.7 + ink,
        "build-time 3 hard lines must measure ~3*lh+ink (lh={lh}), got {h}"
    );
}

#[test]
fn textarea_leaf_height_ignores_chrome_resize() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-resize-decouple";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);

    use_signal(&input_value_signal_key(id), String::new).set("123\n456".into());
    let h_key = crate::utils::text_field::textarea_height_signal_key(id);
    use_signal(&h_key, || 0.0_f32).set(120.0);

    let el: Element = textarea(id).rows(4).resizable(true).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let h_short = find_tid(&root, &tid).expect("text leaf").layout_bounds[3];

    use_signal(&h_key, || 0.0_f32).set(280.0);
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], false, false);
    let h_tall = find_tid(&root, &tid).expect("text leaf after resize").layout_bounds[3];

    assert!(
        (h_short - h_tall).abs() < 1.0,
        "leaf height must stay content-driven when chrome is dragged taller \
         (short={h_short}, tall={h_tall}, lh={lh})"
    );
    assert!(
        h_tall > lh * 1.8 && h_tall < lh * 3.5,
        "2-line leaf should remain ~2*lh (+ink), got {h_tall}"
    );
}

/// Gallery symptom: short chrome shows 1 of 3 lines; dragging taller reveals lines AND
/// grows empty space. Distinguish flex-shrunk leaf (bad) vs overflow clip (ok).
#[test]
fn textarea_short_chrome_clips_content_tall_leaf_not_flex_shrink() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-short-clip";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let pad_y = 0.5 * 16.0;
    // Barely ~1 line visible inside chrome (rows padding).
    let chrome_short = lh + pad_y * 2.0 + 4.0;
    let chrome_mid = lh * 2.0 + pad_y * 2.0 + 4.0;
    let chrome_tall = lh * 6.0 + pad_y * 2.0;

    use_signal(&input_value_signal_key(id), String::new).set("啊\n啊\n啊".into());
    let h_key = crate::utils::text_field::textarea_height_signal_key(id);
    use_signal(&h_key, || 0.0_f32).set(chrome_short);

    let el: Element = textarea(id).rows(4).resizable(true).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(500.0).child(el);
    let result_short = tree.build_and_layout(&mut root, 400.0, 500.0, [-1.0, -1.0]);
    let leaf_short = find_tid(&root, &tid).expect("text leaf");
    let leaf_h_short = leaf_short.layout_bounds[3];
    let chrome_h_short = root.children[0].layout_bounds[3];
    let info_short = result_short
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text info short");
    let clip_h_short = info_short.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);

    // Grow through mid → tall like a drag.
    for chrome in [chrome_mid, chrome_tall] {
        use_signal(&h_key, || 0.0_f32).set(chrome);
        let mut impact = LayoutStatus::Clean;
        sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
        let _ = tree.build_and_layout_fast(&mut root, 400.0, 500.0, [-1.0, -1.0], false, false);
    }
    let result_tall = tree.build_and_layout_fast(&mut root, 400.0, 500.0, [-1.0, -1.0], false, false);
    let leaf_h_tall = find_tid(&root, &tid).expect("text leaf tall").layout_bounds[3];
    let chrome_h_tall = root.children[0].layout_bounds[3];
    let info_tall = result_tall
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text info tall");
    let clip_h_tall = info_tall.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);

    eprintln!(
        "[diag] chrome_short={chrome_h_short:.1} leaf_short={leaf_h_short:.1} clip_short={clip_h_short:.1} info_h_short={:.1}",
        info_short.height
    );
    eprintln!(
        "[diag] chrome_tall={chrome_h_tall:.1} leaf_tall={leaf_h_tall:.1} clip_tall={clip_h_tall:.1} info_h_tall={:.1}",
        info_tall.height
    );

    // Content-driven leaf must stay ~3 lines; chrome is clamped to rows min (~4lh).
    assert!(
        leaf_h_short > lh * 2.7,
        "text leaf must stay content-tall (leaf={leaf_h_short} lh={lh} chrome={chrome_h_short})"
    );
    assert!(
        (leaf_h_short - leaf_h_tall).abs() < 1.0,
        "leaf must not grow with chrome drag (short={leaf_h_short} tall={leaf_h_tall})"
    );
    assert!(
        clip_h_tall > clip_h_short + lh * 0.5,
        "clip window should grow with chrome (short={clip_h_short} tall={clip_h_tall})"
    );
    // At rows=4 min chrome, clip is taller than 3-line leaf — so "only first line
    // visible when small" is NOT explained by overflow clipping the leaf.
    assert!(
        clip_h_short + 0.5 >= leaf_h_short,
        "min chrome should fully fit content leaf (clip={clip_h_short} leaf={leaf_h_short}); \
         if gallery still hides lines 2–3, look at glyphon soft_h / paint, not flex shrink"
    );
}

/// Chrome uses `flex_shrink(0)` + absolute text_region fill: parent flex must not
/// crush the field or leave a short clip while the white chrome stays tall.
#[test]
fn textarea_parent_flex_shrink_does_not_crush_leaf_height() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-parent-shrink";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);

    use_signal(&input_value_signal_key(id), String::new).set("啊\n啊\n啊".into());
    let h_key = crate::utils::text_field::textarea_height_signal_key(id);
    use_signal(&h_key, || 0.0_f32).set(280.0);

    let el: Element = textarea(id).rows(4).resizable(true).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new()
        .w(400.0)
        .h(lh * 1.6)
        .flex_col()
        .child(el);
    let result = tree.build_and_layout(&mut root, 400.0, lh * 1.6, [-1.0, -1.0]);
    let leaf_h = find_tid(&root, &tid).expect("text leaf").layout_bounds[3];
    let chrome_h = root.children[0].layout_bounds[3];
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text info");
    let clip_h = info.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);

    eprintln!("[diag-parent-shrink] chrome={chrome_h:.1} leaf={leaf_h:.1} clip={clip_h:.1}");

    assert!(
        leaf_h > lh * 2.7,
        "leaf must stay ~3 lines under parent pressure (leaf={leaf_h})"
    );
    // Prefer keeping the signaled chrome height over being crushed by a short parent.
    assert!(
        chrome_h > 200.0,
        "flex_shrink(0) chrome should keep height under parent pressure (chrome={chrome_h})"
    );
    assert!(
        (clip_h - chrome_h).abs() < 1.0,
        "absolute text_region clip must fill chrome (clip={clip_h} chrome={chrome_h})"
    );
    assert!(
        clip_h + 0.5 >= leaf_h,
        "clip must fit content leaf (clip={clip_h} leaf={leaf_h})"
    );
}

/// Glyph Metrics must stay at style font_size/line_box — never track chrome/leaf height.
#[test]
fn textarea_shaped_line_height_matches_style_not_chrome() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-shape-lh";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);

    use_signal(&input_value_signal_key(id), String::new).set("aa\nbb".into());
    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text info");
    assert_eq!(info.style.line_height_px, Some(lh));
    assert!(
        info.height > lh * 1.8 && info.height < lh * 3.5,
        "leaf must be content-tall, got {}",
        info.height
    );

    let family = best_font_family(&fs);
    let soft_h = 2.0 * lh + text_edit_ink_pad_px(size);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(&mut fs, Some(info.width.max(1.0)), Some(soft_h));
    buffer.set_text(
        &mut fs,
        info.text.as_str(),
        glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(400)),
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut fs, false);
    let runs: Vec<_> = buffer.layout_runs().collect();
    assert!(runs.len() >= 2, "expected 2 shaped lines, got {}", runs.len());
    for (i, run) in runs.iter().enumerate() {
        assert!(
            (run.line_height - lh).abs() < 0.5,
            "run {i} line_height must equal style lh ({lh}), got {}",
            run.line_height
        );
        assert!(
            run.line_height > size * 1.2,
            "run {i} must not be crushed below font size (lh={}, size={})",
            run.line_height,
            size
        );
    }
}

/// Regression for gallery: after hard `\n`, glyphon must yield every line (not only the first).
#[test]
fn textarea_glyphon_buffer_keeps_all_hard_lines_after_grow() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-glyphon-grow";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);

    use_signal(&input_value_signal_key(id), String::new).set("啊啊".into());
    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    use_signal(&input_value_signal_key(id), String::new).set("啊啊\n啊\n啊".into());
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let result = tree.build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], false, false);
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text info");
    assert_eq!(info.text.as_str(), "啊啊\n啊\n啊");

    let family = best_font_family(&fs);
    let hard = info.text.split('\n').count().max(1) as f32;
    let soft_h = (hard * lh).max(lh) + text_edit_ink_pad_px(size);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    // Simulate prepare_text cache: first shape short, then grow with content soft_h.
    buffer.set_size(&mut fs, Some(info.width.max(1.0)), Some(lh + 4.0));
    buffer.set_text(
        &mut fs,
        "啊啊",
        glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(400)),
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut fs, false);
    assert_eq!(buffer.layout_runs().count(), 1);

    buffer.set_size(&mut fs, Some(info.width.max(1.0)), Some(soft_h));
    buffer.set_text(
        &mut fs,
        info.text.as_str(),
        glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(400)),
        glyphon::Shaping::Advanced,
    );
    buffer.set_scroll(glyphon::cosmic_text::Scroll::default());
    buffer.shape_until_scroll(&mut fs, false);
    let runs = buffer.layout_runs().count();
    assert!(
        runs >= 3,
        "content-tall buffer must keep all hard lines after grow (runs={runs}, soft_h={soft_h})"
    );
}

#[test]
fn textarea_inner_clip_taller_than_glyphs_and_no_scroll_offset() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-clip";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(&input_value_signal_key(id), String::new).set("aa\nbb".into());

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text info");
    let clip_h = info.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
    assert!(
        info.height > lh * 1.5,
        "leaf too short: h={} lh={}",
        info.height,
        lh
    );
    assert!(
        clip_h > lh * 3.0,
        "inner clip too short — would crop glyphs+caret: clip_h={clip_h} lh={lh}"
    );
    let chrome_h = root.children[0].layout_bounds[3];
    assert!(
        chrome_h > info.height + lh,
        "chrome should be taller than text leaf (chrome={chrome_h} leaf={})",
        info.height
    );
    let text_el = find_tid(&root, &tid).expect("text leaf");
    assert_eq!(
        text_el.scroll_offset,
        [0.0, 0.0],
        "textarea text must not carry a scroll offset"
    );
}

#[test]
fn textarea_leaf_shrinks_when_hard_lines_deleted() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-shrink";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);

    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(&input_value_signal_key(id), String::new).set("aa\nbb\ncc\ndd".into());

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let h_tall = find_tid(&root, &tid).expect("text leaf").layout_bounds[3];
    assert!(h_tall > lh * 3.5, "fixture tall leaf, got {h_tall}");

    use_signal(&input_value_signal_key(id), String::new).set("aa".into());
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    assert!(
        matches!(impact, LayoutStatus::LayoutChanged),
        "delete lines must LayoutChanged, got {impact:?}"
    );
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], false, false);
    let h_short = find_tid(&root, &tid).expect("text leaf after shrink").layout_bounds[3];
    assert!(
        h_short < h_tall - lh,
        "leaf must shrink after deleting lines (tall={h_tall} short={h_short} lh={lh})"
    );
    assert!(
        h_short < lh * 2.2,
        "single-line leaf should be ~1*lh+ink, got {h_short}"
    );
}

#[test]
fn textarea_wheel_scroll_not_clamped_to_caret_every_frame() {
    // Caret on an early line must not pull sy back after the user scrolls down.
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-wheel-free";
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let text = "啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊";
    use_signal(&input_value_signal_key(id), String::new).set(text.into());
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    // Caret on line 1 (byte 0).
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(0));

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    // First paint: records last caret.
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);

    let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
    let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest);
    use_signal(&sy_key, || 0.0_f32).set(80.0);
    fn set_region_sy(el: &mut Element, fid: &str, sy: f32) -> bool {
        if el.focus_id.as_deref() == Some(fid) {
            el.scroll_offset[1] = sy;
            return true;
        }
        for c in &mut el.children {
            if set_region_sy(c, fid, sy) {
                return true;
            }
        }
        false
    }
    assert!(set_region_sy(&mut root, &vp_fid, 80.0));

    // Second paint: caret unchanged — must NOT snap sy back to 0.
    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let sy = use_signal(&sy_key, || 0.0_f32).peek();
    assert!(
        sy > 40.0,
        "unchanged caret must not clamp wheel scroll to caret_top (sy={sy})"
    );
}

#[test]
fn textarea_has_overlay_y_scrollbar_track() {
    let id = "ta-ml-scrollbar";
    use_signal(&input_value_signal_key(id), String::new)
        .set("1\n2\n3\n4\n5\n6\n7\n8".into());
    let el: Element = textarea(id).rows(2).into();
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let track_fid = format!(
        "{}{}:y",
        crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX,
        rest
    );
    fn find_focus<'a>(el: &'a Element, fid: &str) -> bool {
        if el.focus_id.as_deref() == Some(fid) {
            return true;
        }
        el.children.iter().any(|c| find_focus(c, fid))
    }
    assert!(
        find_focus(&el, &track_fid),
        "textarea chrome must mount a Y scrollbar track for {rest}"
    );
}

#[test]
fn textarea_paint_sync_keeps_scroll_when_leaf_bounds_match_viewport() {
    // Simulate a flex-sized leaf (layout_bounds ≈ clip) while store text is tall.
    // Old paint used layout_bounds alone → content_fits → sy cleared every frame.
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-short-leaf-scroll";
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let text = "啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊";
    use_signal(&input_value_signal_key(id), String::new).set(text.into());
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(text.len()));

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let tid = input_text_template_id(id);
    let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest);
    fn crush_leaf_to_region(el: &mut Element, tid: &str, vp_fid: &str) {
        fn find_h(el: &Element, vp_fid: &str) -> Option<f32> {
            if el.focus_id.as_deref() == Some(vp_fid) {
                return Some(el.layout_bounds[3]);
            }
            for c in &el.children {
                if let Some(h) = find_h(c, vp_fid) {
                    return Some(h);
                }
            }
            None
        }
        let region_h = find_h(el, vp_fid).unwrap_or(0.0).max(1.0);
        fn crush(el: &mut Element, tid: &str, h: f32) {
            if el.template_id.as_deref() == Some(tid) {
                el.layout_bounds[3] = h;
                return;
            }
            for c in &mut el.children {
                crush(c, tid, h);
            }
        }
        crush(el, tid, region_h);
    }
    crush_leaf_to_region(&mut root, &tid, &vp_fid);

    let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
    use_signal(&sy_key, || 0.0_f32).set(40.0);
    fn set_region_sy(el: &mut Element, fid: &str, sy: f32) -> bool {
        if el.focus_id.as_deref() == Some(fid) {
            el.scroll_offset[1] = sy;
            return true;
        }
        for c in &mut el.children {
            if set_region_sy(c, fid, sy) {
                return true;
            }
        }
        false
    }
    assert!(set_region_sy(&mut root, &vp_fid, 40.0));

    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let sy = use_signal(&sy_key, || 0.0_f32).peek();
    assert!(
        sy > 8.0,
        "short leaf layout_bounds must not clear overflow scroll (sy={sy})"
    );
}

#[test]
fn textarea_paint_sync_scrolls_caret_into_view_when_past_fold() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-caret-scroll";
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let text = "啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊";
    use_signal(&input_value_signal_key(id), String::new).set(text.into());
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(text.len()));

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let tid = input_text_template_id(id);
    let y0 = tree
        .build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], true, false)
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text")
        .y;

    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let sy = use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
        || 0.0_f32,
    )
    .peek();
    assert!(
        sy > 8.0,
        "caret on last line must scroll textarea into view, sy={sy}"
    );

    let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest);
    let region = find_tid(&root, &format!("__rui_textarea:{id}:region")).expect("region");
    assert_eq!(region.focus_id.as_deref(), Some(vp_fid.as_str()));
    assert!(
        region.scroll_offset[1] > 8.0,
        "region.scroll_offset must mirror caret scroll, got {:?}",
        region.scroll_offset
    );

    let info1 = tree
        .build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], true, false)
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text after scroll")
        .clone();
    let dy = y0 - info1.y;
    assert!(
        dy > sy - 1.0 && dy < sy + 1.0,
        "collect must apply caret scroll to text y (sy={sy} dy={dy} y0={y0} y1={})",
        info1.y
    );
    let clip_h = info1.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
    assert!(
        info1.height > clip_h + 8.0,
        "scrolled leaf must stay taller than clip (leaf={} clip={})",
        info1.height,
        clip_h
    );
}

#[test]
fn textarea_nested_in_scroll_area_keeps_own_overflow_metrics() {
    use crate::app::scroll_support::collect_scroll_rest_chain_at;
    use crate::components::scroll_area::scroll_area;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-nested-scroll";
    let tid = input_text_template_id(id);
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    use_signal(&input_value_signal_key(id), String::new)
        .set("啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊".into());

    let body = Element::new()
        .w("100%")
        .flex_col()
        .gap(12.0)
        .p(24.0)
        .child(Element::new().w("100%").h(40.0))
        .child(Element::from(textarea(id).rows(4)));
    let mut root = Element::from(
        scroll_area("root-scroll-diag")
            .w(900.0)
            .h(600.0)
            .overflow_y("auto")
            .overflow_x("auto")
            .child(body),
    );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 900.0, 600.0, [-1.0, -1.0]);

    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text");
    let leaf_h = info.height;
    let clip_h = info.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
    assert!(
        leaf_h > clip_h + lh,
        "overflowing textarea must clip leaf (leaf={leaf_h} clip={clip_h} lh={lh})"
    );

    let (_id, bounds, max_scroll, content) = result
        .scroll_metrics
        .iter()
        .find(|(rid, ..)| rid == &rest)
        .expect("textarea scroll metrics must exist inside outer ScrollArea");
    assert!(
        content[1] > bounds[3] + 1.0 && max_scroll[1] > lh,
        "textarea max_scroll_y must stay > 0 when nested (content={} vp={} max={})",
        content[1],
        bounds[3],
        max_scroll[1]
    );

    // Pointer over the text leaf must prefer the textarea viewport over root-scroll.
    let cx = info.x + info.width * 0.5;
    let cy = info.y + clip_h.min(info.height) * 0.5;
    let mut chain = Vec::new();
    assert!(collect_scroll_rest_chain_at(
        &root, cx, cy, [0.0, 0.0], &mut chain
    ));
    assert_eq!(
        chain.first().map(String::as_str),
        Some(rest.as_str()),
        "deepest scroll under textarea must be the field viewport, got {chain:?}"
    );

    // Paint sync must not treat overflowing content as "fits" and zero scroll.
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(&rest);
    use_signal(&sy_key, || 0.0_f32).set(lh * 2.0);
    let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest);
    fn set_region_sy(el: &mut Element, fid: &str, sy: f32) -> bool {
        if el.focus_id.as_deref() == Some(fid) {
            el.scroll_offset[1] = sy;
            return true;
        }
        for c in &mut el.children {
            if set_region_sy(c, fid, sy) {
                return true;
            }
        }
        false
    }
    assert!(set_region_sy(&mut root, &vp_fid, lh * 2.0));
    let mut impact = LayoutStatus::Clean;
    // Find textarea chrome (has input_paint_sync) under root.
    fn sync_textarea(el: &mut Element, tree: &mut LayoutTree, impact: &mut LayoutStatus) {
        if el.input_paint_sync.is_some() {
            sync_text_edit_paint_subtree(el, tree, impact);
            return;
        }
        for c in &mut el.children {
            sync_textarea(c, tree, impact);
        }
    }
    sync_textarea(&mut root, &mut tree, &mut impact);
    let sy_after = use_signal(&sy_key, || 0.0_f32).peek();
    assert!(
        sy_after > lh,
        "paint sync must not clear scroll while content overflows (sy={sy_after})"
    );
}

#[test]
fn textarea_scroll_offset_shifts_text_inside_fixed_clip() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-scroll-shift";
    let tid = input_text_template_id(id);
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    use_signal(&input_value_signal_key(id), String::new)
        .set("啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊".into());

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let r0 = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let info0 = r0
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text");
    let leaf_h = find_tid(&root, &tid).expect("leaf").layout_bounds[3];
    let clip_h0 = info0.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
    let (_id, bounds, max_scroll, content) = r0
        .scroll_metrics
        .iter()
        .find(|(rid, ..)| rid == &rest)
        .expect("metrics");
    assert!(
        leaf_h > clip_h0 + lh,
        "leaf must be taller than clip for overflow scroll (leaf={leaf_h} clip={clip_h0} lh={lh})"
    );
    assert!(
        max_scroll[1] > lh,
        "max_scroll_y too small: max={} content={} vp={} leaf={}",
        max_scroll[1],
        content[1],
        bounds[3],
        leaf_h
    );

    let sy = (lh * 3.0).min(max_scroll[1]);
    let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest);
    fn set_region_sy(el: &mut Element, fid: &str, sy: f32) -> bool {
        if el.focus_id.as_deref() == Some(fid) {
            el.scroll_offset[1] = sy;
            return true;
        }
        for c in &mut el.children {
            if set_region_sy(c, fid, sy) {
                return true;
            }
        }
        false
    }
    assert!(set_region_sy(&mut root, &vp_fid, sy));
    let r1 = tree.build_and_layout_fast(&mut root, 400.0, 400.0, [-1.0, -1.0], true, false);
    let info1 = r1
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("text after scroll");
    let clip_h1 = info1.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
    let dy = info0.y - info1.y;
    assert!(
        (clip_h1 - clip_h0).abs() < 1.0,
        "clip height must stay viewport-sized (before={clip_h0} after={clip_h1})"
    );
    assert!(
        dy > sy - 1.0 && dy < sy + 1.0,
        "text must translate by scroll_offset (sy={sy} dy={dy} y0={} y1={})",
        info0.y,
        info1.y
    );
}

#[test]
fn controlled_value_update_keeps_tall_leaf_and_large_max_scroll() {
    // Live path: empty field → type many lines via store + paint sync (not rebuild).
    use crate::components::scroll_area::scroll_area;
    use crate::utils::text_field::bind_controlled_value;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-ta-controlled";
    let tid = input_text_template_id(id);
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let notes = use_signal("gal_ta_controlled_notes", String::new);
    bind_controlled_value(id, notes);
    use_signal(&input_value_signal_key(id), String::new).set(String::new());

    let body = Element::new()
        .w("100%")
        .flex_col()
        .gap(12.0)
        .p(24.0)
        .child(
            textarea(id)
                .rows(4)
                .resizable(true)
                .value_signal(notes),
        );
    let mut root = Element::from(
        scroll_area("root-scroll-ta")
            .w(900.0)
            .h(600.0)
            .overflow_y("auto")
            .child(body),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 900.0, 600.0, [-1.0, -1.0]);

    let text = "啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊";
    notes.set(text.to_string());
    use_signal(&input_value_signal_key(id), String::new).set(text.into());
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(text.len()));

    fn sync_ta(el: &mut Element, tree: &mut LayoutTree, impact: &mut LayoutStatus) {
        if el.input_paint_sync.is_some() {
            sync_text_edit_paint_subtree(el, tree, impact);
            return;
        }
        for c in &mut el.children {
            sync_ta(c, tree, impact);
        }
    }
    let mut impact = LayoutStatus::Clean;
    sync_ta(&mut root, &mut tree, &mut impact);
    let result = tree.build_and_layout_fast(&mut root, 900.0, 600.0, [-1.0, -1.0], false, false);

    let leaf = find_tid(&root, &tid).expect("leaf");
    let region = find_tid(&root, &format!("__rui_textarea:{id}:region")).expect("region");
    let (_id, bounds, max_scroll, content) = result
        .scroll_metrics
        .iter()
        .find(|(r, ..)| r == &rest)
        .expect("ta metrics");

    assert!(
        leaf.layout_bounds[3] > region.layout_bounds[3] + lh,
        "after paint sync leaf must stay content-tall (leaf={} region={} style_h={:?})",
        leaf.layout_bounds[3],
        region.layout_bounds[3],
        leaf.style.size.height
    );
    assert!(
        max_scroll[1] > lh * 3.0,
        "after controlled update max_scroll must be large (max={} content={} vp={} leaf={} region={})",
        max_scroll[1],
        content[1],
        bounds[3],
        leaf.layout_bounds[3],
        region.layout_bounds[3]
    );
}

#[test]
fn gallery_like_nine_lines_must_have_large_max_scroll() {
    // Live symptom: wheel only moves a few px because scroll_metrics max_y ≈ padding
    // when the leaf is flex-sized to the scrollport instead of content height.
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-ta-nine";
    let tid = input_text_template_id(id);
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let text = "啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊\n啊";
    use_signal(&input_value_signal_key(id), String::new).set(text.into());

    let el: Element = textarea(id).rows(4).resizable(true).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(900.0).h(800.0).child(el);
    let result = tree.build_and_layout(&mut root, 900.0, 800.0, [-1.0, -1.0]);

    let leaf = find_tid(&root, &tid).expect("leaf");
    let region = find_tid(&root, &format!("__rui_textarea:{id}:region")).expect("region");
    let leaf_h = leaf.layout_bounds[3];
    let region_h = region.layout_bounds[3];
    let style_h = match leaf.style.size.height {
        taffy::style::Dimension::Length(v) => v,
        _ => -1.0,
    };
    let (_id, bounds, max_scroll, content) = result
        .scroll_metrics
        .iter()
        .find(|(r, ..)| r == &rest)
        .expect("metrics");

    assert!(
        region_h > lh * 3.0 && region_h < lh * 6.0,
        "region must match rows≈4 chrome, not content (region={region_h} lh={lh})"
    );
    assert!(
        style_h > region_h + lh,
        "style height must be content-tall (style={style_h} region={region_h} lh={lh})"
    );
    assert!(
        leaf_h > region_h + lh,
        "layout leaf must be content-tall, not scrollport-sized \
         (leaf={leaf_h} region={region_h} style={style_h} content={} vp={} max={})",
        content[1],
        bounds[3],
        max_scroll[1]
    );
    assert!(
        max_scroll[1] > lh * 3.0,
        "max_scroll_y must cover several hidden lines (max={} leaf={} region={} content={} vp={})",
        max_scroll[1],
        leaf_h,
        region_h,
        content[1],
        bounds[3]
    );
}

#[test]
fn textarea_overflow_content_enables_y_scroll_metrics() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ml-scroll";
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let pad_y = 0.5 * 16.0;
    // Chrome fits ~2 lines; content is 8 hard lines → must scroll.
    let chrome_h = lh * 2.0 + pad_y * 2.0 + 4.0;
    let rest = crate::utils::text_field::textarea_scroll_rest(id);
    let h_key = crate::utils::text_field::textarea_height_signal_key(id);
    use_signal(&h_key, || 0.0_f32).set(chrome_h);
    use_signal(&input_value_signal_key(id), String::new)
        .set("1\n2\n3\n4\n5\n6\n7\n8".into());

    let el: Element = textarea(id).rows(2).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&rest);
    let region = find_tid(&root, &format!("__rui_textarea:{id}:region")).expect("region");
    assert_eq!(region.focus_id.as_deref(), Some(vp_fid.as_str()));

    let (_id, bounds, max_scroll, content_size) = result
        .scroll_metrics
        .iter()
        .find(|(rid, ..)| rid == &rest)
        .expect("textarea scroll metrics");
    assert!(
        content_size[1] > bounds[3] + 1.0,
        "content must exceed scrollport (content_h={} vp_h={})",
        content_size[1],
        bounds[3]
    );
    assert!(
        max_scroll[1] > 1.0,
        "max_scroll_y must be > 0 when lines overflow chrome, got {}",
        max_scroll[1]
    );
}
