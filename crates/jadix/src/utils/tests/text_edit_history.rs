use super::*;
use crate::core::reactivity::{create_root, use_signal};
use crate::core::tree::UnmountHook;
use crate::utils::text_field::input_value_signal_key;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn snap(value: &str, caret: usize) -> TextEditSnapshot {
    TextEditSnapshot {
        value: value.into(),
        caret,
        ..Default::default()
    }
}

fn snap_ex(value: &str, caret: usize, extra_carets: Vec<(usize, usize)>) -> TextEditSnapshot {
    TextEditSnapshot {
        value: value.into(),
        caret,
        extra_carets,
        ..Default::default()
    }
}

#[test]
fn insert_coalesces_then_undo_restores_start() {
    reset("h1");
    let id = "h1";
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("a", 1));
    will_user_edit(id, snap("a", 1), EditKind::Insert);
    note_after_edit(id, snap("ab", 2));
    will_user_edit(id, snap("ab", 2), EditKind::Insert);
    note_after_edit(id, snap("abc", 3));
    assert_eq!(past_len(id), 1);
    let undone = undo(id).expect("undo");
    assert_eq!(undone.value, "");
    assert_eq!(undo(id), None);
    let redone = redo(id).expect("redo");
    assert_eq!(redone.value, "abc");
    reset(id);
}

#[test]
fn replace_does_not_coalesce_with_insert() {
    reset("h2");
    let id = "h2";
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("a", 1));
    will_user_edit(id, snap("a", 1), EditKind::Replace);
    note_after_edit(id, snap("hello", 5));
    assert_eq!(undo(id).unwrap().value, "a");
    assert_eq!(undo(id).unwrap().value, "");
    reset(id);
}

#[test]
fn delete_coalesces() {
    reset("h3");
    let id = "h3";
    will_user_edit(id, snap("ab", 2), EditKind::Delete);
    note_after_edit(id, snap("a", 1));
    will_user_edit(id, snap("a", 1), EditKind::Delete);
    note_after_edit(id, snap("", 0));
    assert_eq!(past_len(id), 1);
    assert_eq!(undo(id).unwrap().value, "ab");
    assert_eq!(undo(id), None);
    reset(id);
}

#[test]
fn redo_cleared_by_new_edit() {
    reset("h4");
    let id = "h4";
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("a", 1));
    let _ = undo(id);
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("b", 1));
    assert_eq!(redo(id), None);
    assert_eq!(undo(id).unwrap().value, "");
    reset(id);
}

#[test]
fn step_coalesces() {
    reset("h5");
    let id = "h5";
    will_user_edit(id, snap("1", 1), EditKind::Step);
    note_after_edit(id, snap("2", 1));
    will_user_edit(id, snap("2", 1), EditKind::Step);
    note_after_edit(id, snap("3", 1));
    assert_eq!(undo(id).unwrap().value, "1");
    assert_eq!(undo(id), None);
    reset(id);
}

#[test]
fn paste_replace_does_not_merge_with_typing() {
    reset("h_paste");
    let id = "h_paste";
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("hi", 2));
    will_user_edit(id, snap("hi", 2), EditKind::Replace);
    note_after_edit(id, snap("hipasted", 8));
    will_user_edit(id, snap("hipasted", 8), EditKind::Insert);
    note_after_edit(id, snap("hipasted!", 9));
    assert_eq!(undo(id).unwrap().value, "hipasted");
    assert_eq!(undo(id).unwrap().value, "hi");
    assert_eq!(undo(id).unwrap().value, "");
    reset(id);
}

#[test]
fn ime_commit_replace_is_own_group() {
    reset("h_ime");
    let id = "h_ime";
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("a", 1));
    will_user_edit(id, snap("a", 1), EditKind::Replace);
    note_after_edit(id, snap("a你好", 7));
    will_user_edit(id, snap("a你好", 7), EditKind::Insert);
    note_after_edit(id, snap("a你好b", 8));
    assert_eq!(undo(id).unwrap().value, "a你好");
    assert_eq!(undo(id).unwrap().value, "a");
    assert_eq!(undo(id).unwrap().value, "");
    reset(id);
}

#[test]
fn caret_jump_starts_new_insert_group() {
    reset("h_jump");
    let id = "h_jump";
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("a", 1));
    will_user_edit(id, snap("a", 0), EditKind::Insert);
    note_after_edit(id, snap("xa", 1));
    assert_eq!(past_len(id), 2);
    assert_eq!(undo(id).unwrap().value, "a");
    assert_eq!(undo(id).unwrap().value, "");
    reset(id);
}

#[test]
fn undo_restores_extra_carets() {
    reset("h_ex");
    let id = "h_ex";
    will_user_edit(id, snap_ex("ab", 0, vec![(1, 1)]), EditKind::Insert);
    note_after_edit(id, snap_ex("XaXb", 1, vec![(3, 3)]));
    let undone = undo(id).expect("undo");
    assert_eq!(undone.value, "ab");
    assert_eq!(undone.caret, 0);
    assert_eq!(undone.extra_carets, vec![(1, 1)]);
    let redone = redo(id).expect("redo");
    assert_eq!(redone.value, "XaXb");
    assert_eq!(redone.caret, 1);
    assert_eq!(redone.extra_carets, vec![(3, 3)]);
    reset(id);
}

#[test]
fn extra_carets_coalesce_across_typing() {
    reset("h_exc");
    let id = "h_exc";
    // Extra caret sits before a contiguous tail insert so the payloads merge,
    // while extra_carets stay in the snapshot (and must not break coalesce).
    will_user_edit(id, snap_ex("ab", 2, vec![(0, 0)]), EditKind::Insert);
    note_after_edit(id, snap_ex("abx", 3, vec![(0, 0)]));
    will_user_edit(id, snap_ex("abx", 3, vec![(0, 0)]), EditKind::Insert);
    note_after_edit(id, snap_ex("abxy", 4, vec![(0, 0)]));
    assert_eq!(past_len(id), 1);
    let undone = undo(id).expect("undo");
    assert_eq!(undone.value, "ab");
    assert_eq!(undone.caret, 2);
    assert_eq!(undone.extra_carets, vec![(0, 0)]);
    reset(id);
}

#[test]
fn extra_caret_change_starts_new_insert_group() {
    reset("h_exj");
    let id = "h_exj";
    will_user_edit(id, snap_ex("ab", 0, vec![]), EditKind::Insert);
    note_after_edit(id, snap_ex("Xab", 1, vec![]));
    will_user_edit(id, snap_ex("Xab", 1, vec![(2, 2)]), EditKind::Insert);
    note_after_edit(id, snap_ex("XaXb", 2, vec![(3, 3)]));
    assert_eq!(past_len(id), 2);
    let undone = undo(id).expect("undo");
    assert_eq!(undone.value, "Xab");
    assert_eq!(undone.extra_carets, vec![(2, 2)]);
    reset(id);
}

#[test]
fn password_config_records_nothing() {
    reset("h_pw");
    install_field_config("h_pw", TextUndoConfig::password_default());
    will_user_edit("h_pw", snap("", 0), EditKind::Insert);
    note_after_edit("h_pw", snap("secret", 6));
    assert_eq!(past_len("h_pw"), 0);
    assert_eq!(undo("h_pw"), None);
    reset("h_pw");
}

#[test]
fn max_entries_is_clamped_and_evicts_oldest() {
    reset("h_cap");
    install_field_config(
        "h_cap",
        TextUndoConfig {
            enabled: true,
            max_entries: 2,
            max_bytes: 0,
            typing_merge: Duration::ZERO,
            secure_disable: false,
        },
    );
    will_user_edit("h_cap", snap("", 0), EditKind::Insert);
    note_after_edit("h_cap", snap("a", 1));
    will_user_edit("h_cap", snap("a", 1), EditKind::Delete);
    note_after_edit("h_cap", snap("", 0));
    will_user_edit("h_cap", snap("", 0), EditKind::Replace);
    note_after_edit("h_cap", snap("z", 1));
    assert_eq!(past_len("h_cap"), 2);
    assert_eq!(undo("h_cap").unwrap().value, "");
    assert_eq!(undo("h_cap").unwrap().value, "a");
    assert_eq!(undo("h_cap"), None);
    reset("h_cap");
}

#[test]
fn max_bytes_evicts_oldest() {
    reset("h_mem");
    install_field_config(
        "h_mem",
        TextUndoConfig {
            enabled: true,
            max_entries: 50,
            max_bytes: 64 * 1024,
            typing_merge: Duration::ZERO,
            secure_disable: false,
        },
    );
    let big = "x".repeat(50_000);
    will_user_edit("h_mem", snap("", 0), EditKind::Insert);
    note_after_edit("h_mem", snap(&big, big.len()));
    will_user_edit("h_mem", snap(&big, big.len()), EditKind::Replace);
    let big2 = "y".repeat(50_000);
    note_after_edit("h_mem", snap(&big2, big2.len()));
    will_user_edit("h_mem", snap(&big2, big2.len()), EditKind::Delete);
    note_after_edit("h_mem", snap("", 0));
    assert!(current_bytes("h_mem") <= 64 * 1024);
    assert!(past_len("h_mem") <= 2);
    reset("h_mem");
}

#[test]
fn internal_sync_does_not_record() {
    reset("h_sync");
    with_internal_sync("h_sync", || {
        will_user_edit("h_sync", snap("", 0), EditKind::Insert);
        note_after_edit("h_sync", snap("a", 1));
    });
    assert_eq!(past_len("h_sync"), 0);
    assert_eq!(undo("h_sync"), None);
    reset("h_sync");
}

#[test]
fn unmount_hook_releases_history() {
    reset("h_drop");
    let hook = UnmountHook::new(|| reset("h_drop"));
    will_user_edit("h_drop", snap("", 0), EditKind::Insert);
    note_after_edit("h_drop", snap("stay", 4));
    assert_eq!(past_len("h_drop"), 1);
    drop(hook);
    assert_eq!(past_len("h_drop"), 0);
    assert_eq!(undo("h_drop"), None);
    reset("h_drop");
}

#[test]
fn undo_field_restores_value_and_caret() {
    reset("h6");
    let (_owner, ()) = create_root(|| {
        let id = "h6";
        use_signal(&input_value_signal_key(id), String::new).set("ab".into());
        use_signal(
            &crate::utils::text_field::input_caret_signal_key(id),
            || None::<usize>,
        )
        .set(Some(2));
        will_user_edit_field(id, EditKind::Insert);
        use_signal(&input_value_signal_key(id), String::new).set("abc".into());
        use_signal(
            &crate::utils::text_field::input_caret_signal_key(id),
            || None::<usize>,
        )
        .set(Some(3));
        note_field_after_edit(id);
        assert!(undo_field(id));
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "ab"
        );
        assert!(redo_field(id));
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "abc"
        );
        reset(id);
    });
}

#[test]
fn undo_field_restores_extra_carets() {
    reset("h_exf");
    let (_owner, ()) = create_root(|| {
        let id = "h_exf";
        use_signal(&input_value_signal_key(id), String::new).set("ab".into());
        use_signal(
            &crate::utils::text_field::input_caret_signal_key(id),
            || None::<usize>,
        )
        .set(Some(0));
        crate::utils::text_field::extra_carets_signal(id).set(vec![(1, 1)]);
        will_user_edit_field(id, EditKind::Insert);
        use_signal(&input_value_signal_key(id), String::new).set("XaXb".into());
        use_signal(
            &crate::utils::text_field::input_caret_signal_key(id),
            || None::<usize>,
        )
        .set(Some(1));
        crate::utils::text_field::extra_carets_signal(id).set(vec![(3, 3)]);
        note_field_after_edit(id);
        assert!(undo_field(id));
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "ab"
        );
        assert_eq!(
            use_signal(
                &crate::utils::text_field::input_caret_signal_key(id),
                || None::<usize>,
            )
            .peek(),
            Some(0)
        );
        assert_eq!(
            crate::utils::text_field::extra_carets_signal(id).peek(),
            vec![(1, 1)]
        );
        assert!(redo_field(id));
        assert_eq!(
            crate::utils::text_field::extra_carets_signal(id).peek(),
            vec![(3, 3)]
        );
        reset(id);
    });
}

#[test]
fn clear_history_policy_drops_stack() {
    reset("h_pol");
    will_user_edit("h_pol", snap("", 0), EditKind::Insert);
    note_after_edit("h_pol", snap("a", 1));
    apply_set_text_policy("h_pol", SetTextUndoPolicy::ClearHistory, snap("a", 1));
    assert_eq!(undo("h_pol"), None);
    reset("h_pol");
}

#[test]
fn commit_value_policy_records_only_when_undoable() {
    reset("h_cv");
    let (_owner, ()) = create_root(|| {
        let id = "h_cv";
        use_signal(&input_value_signal_key(id), String::new).set("a".into());
        crate::utils::text_field::commit_value(id, "b".into());
        assert_eq!(past_len(id), 0);
        crate::utils::text_field::commit_value_with_policy(
            id,
            "c".into(),
            SetTextUndoPolicy::Undoable,
        );
        assert_eq!(past_len(id), 1);
        assert_eq!(undo(id).unwrap().value, "b");
        crate::utils::text_field::commit_value_with_policy(
            id,
            "reset".into(),
            SetTextUndoPolicy::ClearHistory,
        );
        assert_eq!(undo(id), None);
        reset(id);
    });
}

#[test]
fn txn_multi_change_undo_is_reverse_order() {
    reset("h_txn");
    let id = "h_txn";
    {
        let _g = edit_group(id, EditKind::Replace);
        will_user_edit(id, snap("a", 1), EditKind::Replace);
        note_after_edit(id, snap("ab", 2));
        will_user_edit(id, snap("ab", 2), EditKind::Replace);
        note_after_edit(id, snap("abc", 3));
    }
    assert_eq!(past_len(id), 1);
    assert_eq!(last_group_change_count(id), 2);
    assert_eq!(undo(id).unwrap().value, "a");
    assert_eq!(redo(id).unwrap().value, "abc");
    reset(id);
}

#[test]
fn txn_drop_on_panic_starts_new_group() {
    reset("h_panic");
    let id = "h_panic";
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        let _g = edit_group(id, EditKind::Insert);
        will_user_edit(id, snap("", 0), EditKind::Insert);
        note_after_edit(id, snap("a", 1));
        panic!("txn unwind");
    }));
    assert!(panicked.is_err());
    will_user_edit(id, snap("a", 1), EditKind::Insert);
    note_after_edit(id, snap("ab", 2));
    assert_eq!(past_len(id), 2);
    assert_eq!(undo(id).unwrap().value, "a");
    assert_eq!(undo(id).unwrap().value, "");
    reset(id);
}

#[test]
fn internal_sync_flag_clears_on_panic() {
    reset("h_sync_p");
    let id = "h_sync_p";
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        with_internal_sync(id, || panic!("sync unwind"));
    }));
    assert!(panicked.is_err());
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("a", 1));
    assert_eq!(past_len(id), 1);
    reset(id);
}

#[test]
fn large_replace_is_marker_and_keeps_earlier_steps() {
    reset("h_mark");
    let id = "h_mark";
    TEST_LARGE_REPLACE_HARD.with(|c| c.set(64));
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("hello", 5));
    let huge = "x".repeat(200);
    will_user_edit(id, snap("hello", 5), EditKind::Replace);
    note_after_edit(id, snap(&huge, huge.len()));
    assert_eq!(past_len(id), 2);
    assert!(last_payload_is_marker(id));
    assert_eq!(undo(id).unwrap().value, "hello");
    assert_eq!(undo(id).unwrap().value, "");
    TEST_LARGE_REPLACE_HARD.with(|c| c.set(undo_caps::LARGE_REPLACE_HARD));
    reset(id);
}

#[test]
fn unrestorable_marker_is_a_wall() {
    reset("h_skip");
    let id = "h_skip";
    TEST_LARGE_REPLACE_HARD.with(|c| c.set(64));
    TEST_RETAIN_SIDE_MAX.with(|c| c.set(4));
    will_user_edit(id, snap("", 0), EditKind::Insert);
    note_after_edit(id, snap("hello", 5));
    let huge = "x".repeat(200);
    will_user_edit(id, snap("hello", 5), EditKind::Replace);
    note_after_edit(id, snap(&huge, huge.len()));
    assert!(last_payload_is_marker(id));
    assert_eq!(past_len(id), 2);
    assert!(
        !can_undo(id),
        "unrestorable Marker stays on top; Z must not apply earlier deltas to the huge buffer"
    );
    assert_eq!(undo(id), None);
    assert_eq!(past_len(id), 2, "wall does not pop earlier groups");
    TEST_RETAIN_SIDE_MAX.with(|c| c.set(undo_caps::RETAIN_SIDE_MAX));
    TEST_LARGE_REPLACE_HARD.with(|c| c.set(undo_caps::LARGE_REPLACE_HARD));
    reset(id);
}
