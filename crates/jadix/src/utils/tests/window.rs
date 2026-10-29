#[test]
fn same_named_hover_intents_are_isolated_and_released_with_the_window() {
    use crate::app::window_scope::{self, WindowScope};
    let primary = WindowScope::Primary;
    let secondary = WindowScope::Secondary(805);
    crate::authoring::window::activate(primary);
    crate::authoring::window::activate(secondary);
    let _primary = window_scope::enter(primary);
    super::hover_intent_map()
        .lock()
        .unwrap()
        .insert("same-icon".into(), true);
    {
        let _secondary = window_scope::enter(secondary);
        assert!(!super::hover_intent_map()
            .lock()
            .unwrap()
            .contains_key("same-icon"));
        super::hover_intent_map()
            .lock()
            .unwrap()
            .insert("same-icon".into(), false);
        crate::authoring::window::forget(secondary);
        assert!(super::hover_intent_map().lock().unwrap().is_empty());
    }
    assert_eq!(
        super::hover_intent_map().lock().unwrap().get("same-icon"),
        Some(&true)
    );
    super::release_icon_hover("same-icon");
}
