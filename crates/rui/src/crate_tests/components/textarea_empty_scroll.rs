//! Empty textarea must not report scroll (leaf fits in padded scrollport).

use crate::components::input::input_value_signal_key;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;
use crate::utils::text_field::{
    input_text_template_id, text_edit_line_box_px, textarea_scroll_rest,
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
fn empty_textarea_no_spurious_scroll() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-empty-scroll";
    let tid = input_text_template_id(id);
    let rest = textarea_scroll_rest(id);
    let region_tid = format!("__rui_textarea:{id}:region");
    let lh = text_edit_line_box_px(14.0, None, None);

    use_signal(&input_value_signal_key(id), String::new).set(String::new());

    let el: Element = textarea(id).rows(4).resizable(true).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(400.0).h(400.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

    let chrome = &root.children[0];
    let region = find_tid(chrome, &region_tid).expect("region");
    let leaf = find_tid(chrome, &tid).expect("leaf");
    let (_id, bounds, max_scroll, content) = result
        .scroll_metrics
        .iter()
        .find(|(r, ..)| r == &rest)
        .expect("metrics");

    let pad_top = match region.style.padding.top {
        taffy::style::LengthPercentage::Length(v) => v,
        _ => 0.0,
    };
    let pad_bottom = match region.style.padding.bottom {
        taffy::style::LengthPercentage::Length(v) => v,
        _ => 0.0,
    };
    let content_box_h = bounds[3] - pad_top - pad_bottom;
    let leaf_h = leaf.layout_bounds[3];

    assert!(
        leaf_h <= content_box_h + 0.5,
        "empty leaf must fit padded content box (leaf={leaf_h} box={content_box_h})"
    );
    assert!(
        max_scroll[1] < 0.5,
        "empty textarea must not scroll (max={} content={} vp={} leaf={})",
        max_scroll[1],
        content[1],
        bounds[3],
        leaf_h
    );
    assert!(
        (content[1] - bounds[3]).abs() < 0.5,
        "empty scrollport content_h must equal viewport (content={} vp={})",
        content[1],
        bounds[3]
    );
    assert!(
        leaf_h < lh * 1.5,
        "empty leaf should be ~1 line (lh={lh}), got {leaf_h}"
    );
}
