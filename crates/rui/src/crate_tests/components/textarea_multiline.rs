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
