use super::*;
use crate::core::reactivity::{
    bind_current_owner_window_key_scope, create_effect, create_root, create_signal,
    set_signal_key_scope, use_signal,
};

#[test]
fn tree_home_maps_scope_without_swap() {
    assert_eq!(tree_home_for(WindowScope::Primary), TreeHome::AppState);
    assert_eq!(
        tree_home_for(WindowScope::Secondary(1)),
        TreeHome::SecondarySlot
    );
}

#[test]
fn window_local_signal_keys_split_by_scope() {
    let _g = enter(WindowScope::Primary);
    let primary = use_signal("__jadix_button_focus_id", String::new);
    primary.set("main".into());

    let _g2 = enter(WindowScope::Secondary(42));
    let secondary = use_signal("__jadix_button_focus_id", String::new);
    assert_eq!(secondary.peek(), "");
    secondary.set("sec".into());
    drop(_g2);

    assert_eq!(primary.peek(), "main");
    let _ = set_signal_key_scope(0);
}

#[test]
fn nested_effect_rerun_keeps_owner_window_scope() {
    let (_owner, seen) = create_root(|| {
        bind_current_owner_window_key_scope(99);
        let seen = create_signal(Vec::<WindowScope>::new());
        let tick = create_signal(0u32);
        let seen2 = seen;
        create_effect(move || {
            let _ = tick.get();
            seen2.update(|v| {
                v.push(active());
            });
        });
        // Initial run under create_root still has signal scope 0 unless we enter —
        // bind only stamps the Owner; run_effect restores it on (re)run.
        tick.set(1);
        seen
    });
    let scopes = seen.peek();
    assert!(
        scopes.contains(&WindowScope::Secondary(99)),
        "effect re-run must see Secondary(99), got {scopes:?}"
    );
    let _ = set_signal_key_scope(0);
}
