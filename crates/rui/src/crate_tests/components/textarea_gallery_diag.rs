//! Gallery-shaped repro: controlled value with hard newlines + default chrome.
//! Fails if TextLayoutInfo lost `\n` or leaf collapsed to one line.

use crate::components::input::input_value_signal_key;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;
use crate::utils::text_field::{
    bind_controlled_value, input_text_template_id, text_edit_line_box_px, textarea_height_signal_key,
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
fn gallery_like_textarea_keeps_newlines_and_multi_line_leaf() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-textarea";
    let tid = input_text_template_id(id);
    let lh = text_edit_line_box_px(14.0, None, None);

    // Mirror gallery: external signal + controlled bind.
    let notes = use_signal("gal_textarea_notes_diag", String::new);
    notes.set("啊\n啊\n啊".into());
    bind_controlled_value(id, notes);
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        "啊\n啊\n啊"
    );

    // Default chrome (rows=4 min); do not pre-set height signal taller.
    let _ = use_signal(&textarea_height_signal_key(id), || 0.0_f32);

    let el: Element = textarea(id)
        .rows(4)
        .resizable(true)
        .value_signal(notes)
        .into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(900.0).h(800.0).child(el);
    let result = tree.build_and_layout(&mut root, 900.0, 800.0, [-1.0, -1.0]);

    let leaf = find_tid(&root, &tid).expect("textarea text leaf");
    let leaf_h = leaf.layout_bounds[3];
    let chrome_h = root.children[0].layout_bounds[3];
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("TextLayoutInfo");
    let clip_h = info.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);

    eprintln!(
        "[gallery-diag] chrome={chrome_h:.1} leaf={leaf_h:.1} clip={clip_h:.1} text={:?} newlines={}",
        info.text,
        info.text.matches('\n').count()
    );

    assert!(
        info.text.contains('\n'),
        "TextLayoutInfo must keep hard newlines (got {:?})",
        info.text
    );
    assert_eq!(info.text.split('\n').count(), 3);
    assert!(
        leaf_h > lh * 2.7,
        "leaf must be ~3 lines (leaf={leaf_h} lh={lh})"
    );
    assert!(
        clip_h + 0.5 >= leaf_h,
        "default chrome clip must fit leaf (clip={clip_h} leaf={leaf_h})"
    );
    // Absolute text_region must track chrome (not shrink to first line).
    assert!(
        (clip_h - chrome_h).abs() < 1.0,
        "text_region clip must fill chrome (clip={clip_h} chrome={chrome_h})"
    );
}

/// Nested overflow viewport (gallery ScrollArea shape): when the field is fully
/// in view, text_region clip must fill chrome (≥ content leaf).
#[test]
fn gallery_scroll_area_in_view_textarea_clip_fills_chrome() {
    use crate::primitives::r#box::vstack;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-textarea-scroll";
    let tid = input_text_template_id(id);
    let lh = text_edit_line_box_px(14.0, None, None);
    let notes = use_signal("gal_textarea_notes_scroll", || "啊\n啊\n啊".to_string());
    bind_controlled_value(id, notes);

    let body: Element = vstack()
        .w("100%")
        .gap(16.0)
        .p(24.0)
        .child(Element::new().w("100%").h(40.0))
        .child(
            textarea(id)
                .rows(4)
                .resizable(true)
                .value_signal(notes),
        )
        .into();
    let mut root = Element::new().w(900.0).h(800.0).child(
        Element::new()
            .w("100%")
            .h("100%")
            .overflow_y("auto")
            .child(body),
    );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 900.0, 800.0, [-1.0, -1.0]);
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(tid.as_str()))
        .expect("textarea text");
    let leaf_h = info.height;
    let clip_h = info.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
    eprintln!(
        "[scroll-diag] leaf={leaf_h:.1} clip={clip_h:.1} text={:?}",
        info.text
    );
    assert!(leaf_h > lh * 2.7, "leaf={leaf_h}");
    assert!(
        clip_h + 0.5 >= leaf_h,
        "in-view textarea must not have a short clip (clip={clip_h} leaf={leaf_h})"
    );
}

#[test]
fn gallery_like_textarea_dumps_subtree_geometry() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-textarea-dump";
    let notes = use_signal("gal_textarea_notes_dump", || "啊\n啊\n啊".to_string());
    bind_controlled_value(id, notes);
    let el: Element = textarea(id)
        .rows(4)
        .resizable(true)
        .value_signal(notes)
        .into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(900.0).h(800.0).child(el);
    let _ = tree.build_and_layout(&mut root, 900.0, 800.0, [-1.0, -1.0]);

    fn dump(el: &Element, depth: usize) {
        let pad = "  ".repeat(depth);
        let h = match el.style.size.height {
            taffy::style::Dimension::Length(v) => format!("len({v:.1})"),
            taffy::style::Dimension::Percent(v) => format!("pct({v:.2})"),
            taffy::style::Dimension::Auto => "auto".into(),
        };
        eprintln!(
            "{pad}tid={:?} focus={:?} pos={:?} h={h} grow={} shrink={} overflow_y={:?} \
             bounds={:?} scroll={:?} text={:?}",
            el.template_id,
            el.focus_id,
            el.style.position,
            el.style.flex_grow,
            el.style.flex_shrink,
            el.style.overflow.y,
            el.layout_bounds,
            el.scroll_offset,
            el.text
                .as_ref()
                .map(|t| t.chars().take(12).collect::<String>()),
        );
        for c in &el.children {
            dump(c, depth + 1);
        }
    }
    dump(&root.children[0], 0);
}
