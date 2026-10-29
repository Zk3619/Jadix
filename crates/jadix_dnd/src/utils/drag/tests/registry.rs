use super::*;
use crate::utils::drag::geometry::Rect;

#[test]
fn registered_hits_filter_by_scope() {
    register_slot(DragHit {
        id: "a".into(),
        scope_id: "s1".into(),
        is_draggable: true,
        is_droppable: false,
        rect: Rect::default(),
        item_type: String::new(),
        accept: String::new(),
        list_id: None,
        index: None,
        group: String::new(),
        disabled: false,
        list_axis: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
    });
    register_slot(DragHit {
        id: "b".into(),
        scope_id: "s2".into(),
        is_draggable: true,
        is_droppable: false,
        rect: Rect::default(),
        item_type: String::new(),
        accept: String::new(),
        list_id: None,
        index: None,
        group: String::new(),
        disabled: false,
        list_axis: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
    });
    assert_eq!(registered_hits_in_scope("s1").len(), 1);
    assert_eq!(registered_hits_in_scope("s1")[0].id, "a");
}
