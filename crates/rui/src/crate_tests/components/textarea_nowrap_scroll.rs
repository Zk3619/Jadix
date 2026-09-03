//! wrap=off textarea: horizontal scroll metrics + caret follow-scroll cap at line end.

use crate::components::input::{input_caret_signal_key, input_value_signal_key};
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_w_signal_key, scroll_offset_x_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;
use crate::utils::text_field::{
    ensure_caret_visible_scroll_x, input_text_template_id, text_edit_line_box_px,
    textarea_scroll_rest, TEXTAREA_SCROLLPORT_PAD_X, TEXT_EDIT_CARET_W,
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
fn wrap_off_textarea_reports_horizontal_max_scroll_from_leaf_width() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-nowrap-metrics";
    let rest = textarea_scroll_rest(id);
    let long = "padding: 8px 16px; background: #ffffff; border-radius: 6px — nowrap scroll";
    use_signal(&input_value_signal_key(id), String::new).set(long.into());

    let el: Element = textarea(id).rows(4).wrap("off").into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(200.0).h(120.0).child(el);
    let result = tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);

    let (_id, bounds, max_scroll, content) = result
        .scroll_metrics
        .iter()
        .find(|(r, ..)| r == &rest)
        .expect("textarea scroll metrics");

    assert!(
        max_scroll[0] > 50.0,
        "wrap=off long line must scroll horizontally (max_x={} content_w={} vp_w={})",
        max_scroll[0],
        content[0],
        bounds[2]
    );
    assert!(
        content[0] > bounds[2] + 0.5,
        "content_w must exceed viewport (content={} vp={})",
        content[0],
        bounds[2]
    );
}

#[test]
fn wrap_off_paint_sync_caps_caret_scroll_at_current_line_end() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-nowrap-caret-cap";
    let rest = textarea_scroll_rest(id);
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let short = "short";
    let long = "Supercalifragilisticexpialidocious_unbreakable_token";
    let text = format!("{short}\n{long}");
    let caret_byte = short.len() + 1 + long.len();

    use_signal(&input_value_signal_key(id), String::new).set(text.clone());
    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(caret_byte));

    let el: Element = textarea(id).rows(4).wrap("off").into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(200.0).h(120.0).child(el);
    let result = tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);

    let (_id, bounds, _max_scroll, content) = result
        .scroll_metrics
        .iter()
        .find(|(r, ..)| r == &rest)
        .expect("metrics");
    let vw = bounds[2];
    use_signal(&scroll_viewport_w_signal_key(&rest), || 0.0_f32).set_silent(vw);
    use_signal(&scroll_content_w_signal_key(&rest), || 0.0_f32).set_silent(content[0]);
    use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).set_silent(0.0);

    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);

    let sx = use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).peek();
    let pad_left = TEXTAREA_SCROLLPORT_PAD_X;
    let line_end_x = crate::text::measure_prefix_advance(long, size, 400, lh);
    let cap = pad_left + line_end_x;
    let want = ensure_caret_visible_scroll_x(cap, cap, vw, 0.0);
    assert!(
        (sx - want).abs() < 1.5,
        "caret at long line end must follow-scroll (sx={sx} want={want})"
    );
    assert!(
        cap + TEXT_EDIT_CARET_W - sx <= vw + 1.0,
        "caret must be fully visible (sx={sx} cap={cap} vw={vw})"
    );

    // Caret on the short first line must not inherit the long line's pan.
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(short.len()));
    use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).set_silent(400.0);
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let sx_short = use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).peek();
    assert!(
        sx_short < 1.0,
        "short line caret must snap sx to 0, got {sx_short}"
    );

    let _ = find_tid(&root, &tid).expect("text leaf");
}
