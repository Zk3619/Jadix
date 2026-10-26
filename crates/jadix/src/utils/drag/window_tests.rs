use super::{pointer, registry, runtime, session};
use crate::app::window_scope::{self, WindowScope};
use crate::core::reactivity::create_root;
use crate::core::tree::Element;
use std::rc::Rc;

fn begin() {
    session::begin(session::BeginDrag {
        scope_id: "same-provider".into(),
        draggable_id: "same-item".into(),
        pointer: [10.0, 10.0],
        grab_offset: [0.0; 2],
        start_bounds: [0.0, 0.0, 40.0, 24.0],
        from_list: None,
        from_index: None,
        item_type: String::new(),
        group: String::new(),
        capture_id: "same-item".into(),
    });
}

#[test]
fn same_named_products_and_prepared_work_are_window_local() {
    let _primary = window_scope::enter_primary();
    crate::authoring::window::activate(WindowScope::Primary);
    let (primary_owner, root) =
        create_root(|| super::drag_scope("same-provider").child(Element::new));
    registry::set_item_data("same-provider", "same-item", Rc::new(11_u32));
    begin();
    let work = runtime::prepare_move(
        Some(&root),
        [30.0, 10.0],
        [400.0, 300.0],
        runtime::MoveMode::Follow,
    );
    let secondary = WindowScope::Secondary(804);
    crate::authoring::window::activate(secondary);
    let _secondary = window_scope::enter(secondary);
    let (secondary_owner, secondary_root) =
        create_root(|| super::drag_scope("same-provider").child(Element::new));
    registry::set_item_data("same-provider", "same-item", Rc::new(22_u32));
    begin();
    assert!(work.dispatch());
    assert_eq!(window_scope::active(), secondary);
    assert_eq!(session::last_pointer(), Some([10.0, 10.0]));
    assert_eq!(
        registry::item_data("same-provider", "same-item")
            .unwrap()
            .downcast_ref::<u32>(),
        Some(&22)
    );
    let retired = runtime::prepare_end(&secondary_root);
    {
        let _primary = window_scope::enter_primary();
        assert_eq!(session::last_pointer(), Some([30.0, 10.0]));
        assert_eq!(
            registry::item_data("same-provider", "same-item")
                .unwrap()
                .downcast_ref::<u32>(),
            Some(&11)
        );
        secondary_owner.dispose();
        assert!(session::is_dragging());
        assert!(pointer::is_active());
    }
    assert!(!session::scope_exists("same-provider"));
    assert!(!retired.dispatch());
    crate::authoring::window::forget(secondary);
    assert!(!session::is_dragging());
    drop(_secondary);
    primary_owner.dispose();
    assert!(!session::is_dragging());
}
