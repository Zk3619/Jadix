use super::*;
use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
use crate::utils::text_field::text_edit_line_box_px;
use crate::utils::text_field::TEXTAREA_FOCUS_PREFIX;
use taffy::style::Overflow;

fn textarea_like_tree() -> (Element, f32) {
    let lh = text_edit_line_box_px(14.0, None, None);
    let field_id = "ta-hit";
    let rest = "ta-hit-scroll";

    let mut text = Element::new().w(300.0);
    text.layout_bounds = [34.0, 58.0, 300.0, lh * 3.0];
    text.text = Some("啊\n啊\n啊".into());
    text.focus_id = None;

    let mut viewport = Element::new().w(368.0).h(98.0);
    viewport.layout_bounds = [0.0, 0.0, 368.0, 98.0];
    viewport.style.overflow.y = Overflow::Hidden;
    viewport.focus_id = Some(focus_id_for_scroll_viewport(rest));
    viewport = viewport.child(text);

    let mut chrome = Element::new().w(400.0).h(100.0);
    chrome.layout_bounds = [0.0, 0.0, 400.0, 100.0];
    chrome.focus_id = Some(format!("{TEXTAREA_FOCUS_PREFIX}{field_id}"));
    chrome = chrome.child(viewport);

    (chrome, lh)
}

#[test]
fn find_input_text_at_hits_multiline_leaf_through_scroll_viewport() {
    let (root, lh) = textarea_like_tree();
    let line2_y = 58.0 + lh * 1.5;
    let hit = find_input_text_at(&root, None, 120.0, line2_y, [0.0, 0.0])
        .expect("line 2 must hit textarea text leaf through scroll viewport");
    assert_eq!(hit.0, "ta-hit");
    assert_eq!(hit.5, format!("{TEXTAREA_FOCUS_PREFIX}ta-hit"));
    assert!(hit.2.contains('\n'), "display must include hard newlines");
}

#[test]
fn find_input_text_at_hits_scrollport_padding_not_only_leaf() {
    let (root, lh) = textarea_like_tree();
    let line0_y = 58.0 + lh * 0.4;
    let left = find_input_text_at(&root, None, 10.0, line0_y, [0.0, 0.0])
        .expect("left region padding must resolve to the textarea leaf");
    assert_eq!(left.0, "ta-hit");
    assert_eq!(
        left.1[0], 34.0,
        "must return leaf bounds, not the scrollport"
    );

    let below = find_input_text_at(&root, None, 120.0, 90.0, [0.0, 0.0])
        .expect("empty clip below a short leaf must still resolve to the textarea");
    assert_eq!(below.0, "ta-hit");
}

#[test]
fn find_input_text_leaf_for_rest_finds_multiline_leaf_through_scroll_viewport() {
    let (root, _) = textarea_like_tree();
    let (bounds, text, _, _) =
        find_input_text_leaf_for_rest(&root, None, "ta-hit").expect("leaf for drag-select");
    assert_eq!(text.matches('\n').count(), 2);
    assert!(bounds[3] > 50.0, "bounds must span multi-line leaf");
}

fn input_like_tree_with_padding_and_adornment() -> Element {
    use crate::core::tree::CursorKind;
    use crate::utils::text_field::INPUT_FOCUS_PREFIX;

    let mut text = Element::new();
    text.layout_bounds = [12.0, 4.0, 152.0, 28.0];
    text.text = Some("hello world".into());

    let mut icon = Element::new();
    icon.layout_bounds = [176.0, 8.0, 20.0, 20.0];
    icon.text = Some("x".into());
    icon.text_style.font_family = Some("lucide".into());

    let mut btn = Element::new()
        .on_click(|| {})
        .cursor_kind(CursorKind::Pointer);
    btn.layout_bounds = [176.0, 8.0, 20.0, 20.0];
    btn = btn.child(icon);

    let mut chrome = Element::new().on_click(|| {}).cursor_text();
    chrome.layout_bounds = [0.0, 0.0, 200.0, 36.0];
    chrome.style.overflow.x = Overflow::Hidden;
    chrome.style.overflow.y = Overflow::Hidden;
    chrome.focus_id = Some(format!("{INPUT_FOCUS_PREFIX}pad-hit"));
    chrome.child(text).child(btn)
}

#[test]
fn find_input_text_at_hits_field_padding_not_adornment() {
    let root = input_like_tree_with_padding_and_adornment();
    let left = find_input_text_at(&root, None, 6.0, 18.0, [0.0, 0.0])
        .expect("left padding must resolve to the text leaf");
    assert_eq!(left.0, "pad-hit");
    assert_eq!(left.1[0], 12.0, "must return the leaf bounds, not chrome");
    assert_eq!(left.2, "hello world");

    let right = find_input_text_at(&root, None, 168.0, 18.0, [0.0, 0.0])
        .expect("right padding between leaf and adornment must resolve to the text leaf");
    assert_eq!(right.0, "pad-hit");
    assert_eq!(right.2, "hello world");

    assert!(
        find_input_text_at(&root, None, 186.0, 18.0, [0.0, 0.0]).is_none(),
        "clear/reveal adornment must not be treated as field text"
    );
}

#[test]
fn find_input_text_leaf_for_rest_skips_lucide_adornment() {
    let root = input_like_tree_with_padding_and_adornment();
    let (_, text, _, _) =
        find_input_text_leaf_for_rest(&root, None, "pad-hit").expect("field leaf");
    assert_eq!(text, "hello world");
}

#[test]
fn input_adornment_keycap_is_not_the_editable_text_leaf() {
    let mut root = input_like_tree_with_padding_and_adornment();
    root.children.pop();
    let mut keycap = Element::new().text("K");
    keycap.layout_bounds = [176.0, 8.0, 20.0, 20.0];
    let mut addon = Element::new().child(keycap);
    addon.set_attr("data-slot", "input-adornment-addon");
    addon.layout_bounds = [170.0, 0.0, 30.0, 36.0];
    root.children.push(addon);
    let (_, text, _, _) = find_input_text_leaf_for_rest(&root, None, "pad-hit").unwrap();
    assert_eq!(
        text, "hello world",
        "focus delegation must not turn a keycap into input text"
    );
    let hit = find_input_text_at(&root, None, 24.0, 18.0, [0.0, 0.0]).unwrap();
    assert_eq!(
        hit.2, text,
        "pointer-down and subsequent drag must measure the same run"
    );
}

#[test]
fn find_input_text_at_hits_painted_not_layout_after_rotate_90() {
    use crate::utils::text_field::INPUT_FOCUS_PREFIX;
    let mut el = Element::new();
    el.layout_bounds = [0.0, 0.0, 100.0, 20.0];
    el.text = Some("hello".into());
    el.focus_id = Some(format!("{INPUT_FOCUS_PREFIX}rot-in"));
    el.style.rotate_deg = 90.0;
    let world = crate::core::tree::layout_local_to_paint_world(
        &el,
        [90.0, 10.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    assert!(
        (world[0] - 90.0).abs() > 8.0 || (world[1] - 10.0).abs() > 8.0,
        "90° must move the layout-box point, world={world:?}"
    );
    let hit = find_input_text_at(&el, None, world[0], world[1], [0.0, 0.0])
        .expect("painted text box must hit");
    assert_eq!(hit.0, "rot-in");
    assert!(
        (hit.1[0] - 0.0).abs() < 0.5 && (hit.1[2] - 100.0).abs() < 0.5,
        "returned bounds must stay layout-box, got {:?}",
        hit.1
    );
    assert!(
        find_input_text_at(&el, None, 90.0, 10.0, [0.0, 0.0]).is_none(),
        "old layout-box point must miss after rotate"
    );
    let mapped = world_to_input_text_layout_pointer(&el, "rot-in", world, [0.0, 0.0]);
    assert!(
        (mapped[0] - 90.0).abs() < 1.0 && (mapped[1] - 10.0).abs() < 1.0,
        "caret pointer must invert to layout-box, got {mapped:?}"
    );
}

#[test]
fn find_selectable_text_at_hits_painted_not_layout_after_rotate_90() {
    let mut el = Element::new();
    el.layout_bounds = [0.0, 0.0, 100.0, 20.0];
    el.text = Some("hello".into());
    el.text_style.selectable = true;
    el.template_id = Some("rot-label".into());
    el.style.rotate_deg = 90.0;
    let world = crate::core::tree::layout_local_to_paint_world(
        &el,
        [90.0, 10.0],
        [0.0, 0.0],
        crate::style::MAT2D_IDENTITY,
        [1.0, 1.0],
    );
    let hit = find_selectable_text_at(&el, world[0], world[1], [0.0, 0.0])
        .expect("painted label must hit");
    assert_eq!(hit.0, "rot-label");
    assert!(
        find_selectable_text_at(&el, 90.0, 10.0, [0.0, 0.0]).is_none(),
        "old layout-box point must miss after rotate"
    );
    let mapped = world_to_selectable_text_layout_pointer(&el, "rot-label", world, [0.0, 0.0]);
    assert!(
        (mapped[0] - 90.0).abs() < 1.0 && (mapped[1] - 10.0).abs() < 1.0,
        "label caret pointer must invert to layout-box, got {mapped:?}"
    );
}
