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
