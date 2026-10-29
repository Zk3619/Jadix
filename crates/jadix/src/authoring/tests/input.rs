use super::*;

#[test]
fn scrolling_state_is_scoped_and_cleared_with_the_window() {
    let _primary = window_scope::enter_primary();
    set_geometry_changing(true);
    {
        let _secondary = window_scope::enter(WindowScope::Secondary(712));
        assert!(!geometry_changing());
        set_geometry_changing(true);
        forget(WindowScope::Secondary(712));
        assert!(!geometry_changing());
    }
    assert!(geometry_changing());
    set_geometry_changing(false);
}
