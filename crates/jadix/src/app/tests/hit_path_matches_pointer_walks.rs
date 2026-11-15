use super::{find_focus_id_at, pick_cursor};
use crate::app::pointer_events::{hit_path, path_deepest_focus_id, path_resolved_cursor};
use crate::core::tree::Element;
use crate::style::{CursorKind, CursorValue};

#[test]
fn exclusive_hit_path_matches_focus_and_cursor_walks() {
    let mut leaf = Element::new();
    leaf.layout_bounds = [40.0, 40.0, 30.0, 20.0];
    leaf.focus_id = Some("scroll:main".into());
    leaf.cursor = Some(CursorValue::keyword(CursorKind::Pointer));
    let mut root = Element::new();
    root.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    root.focus_id = Some("page".into());
    root.cursor = Some(CursorValue::keyword(CursorKind::Default));
    root.children.push(leaf);

    let path = hit_path(&root, 50.0, 50.0);
    assert_eq!(
        path_deepest_focus_id(&path),
        find_focus_id_at(&root, 50.0, 50.0, [0.0, 0.0]).as_deref()
    );
    assert_eq!(
        path_resolved_cursor(&path),
        pick_cursor(&root, 50.0, 50.0, [0.0, 0.0])
    );
}

#[test]
fn hit_path_reaches_last_row_at_maximum_scroll_offset() {
    let mut last_row = Element::new();
    last_row.layout_bounds = [0.0, 180.0, 100.0, 20.0];
    last_row.focus_id = Some("last-row".into());
    last_row.style.bg_color = [0.2, 0.3, 0.4, 1.0];

    let mut viewport = Element::new();
    viewport.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    viewport.focus_id = Some("scroll:feed".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    viewport.scroll_offset = [0.0, 100.0];
    viewport.children.push(last_row);
    crate::core::reactivity::use_signal(
        &crate::core::scroll_viewport::scroll_offset_y_signal_key("feed"),
        || 0.0_f32,
    )
    .set_silent(100.0);

    let mut root = Element::new().child(viewport);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let path = hit_path(&root, 50.0, 90.0);
    assert_eq!(path_deepest_focus_id(&path).as_deref(), Some("last-row"));
}
