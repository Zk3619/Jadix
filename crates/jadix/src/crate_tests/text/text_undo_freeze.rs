//! Freeze contract for [`docs/plans/text_undo_redo.md`] (B19).
//!
//! Frozen surface = L0 protocol + L1/L2 Input/Textarea undo/redo + Registry
//! close-tab isolation. Not frozen: rope, LSP, multi-cursor, CRDT, `default` feature.
//!
//! CI:
//! ```text
//! cargo test -p jadix --lib text_undo_freeze
//! cargo test -p jadix --lib --features text-undo text_undo_freeze
//! ```

use crate::utils::clipboard::consumes_undo_redo_letter;
use winit::keyboard::ModifiersState;

fn host_edit_mods() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::META
    } else {
        ModifiersState::CONTROL
    }
}

#[test]
fn freeze_z_y_chords_are_consumed_not_typed() {
    let mods = host_edit_mods();
    assert!(consumes_undo_redo_letter(Some('z'), &mods));
    assert!(consumes_undo_redo_letter(Some('y'), &mods));
    assert!(consumes_undo_redo_letter(
        Some('z'),
        &(mods | ModifiersState::SHIFT)
    ));
    assert!(!consumes_undo_redo_letter(
        Some('z'),
        &ModifiersState::empty()
    ));
}

#[cfg(not(feature = "text-undo"))]
#[test]
fn freeze_text_undo_is_opt_in_not_default() {
    assert!(
        !cfg!(feature = "text-undo"),
        "`text-undo` must stay out of crate default features"
    );
}

#[cfg(not(feature = "text-undo"))]
#[test]
fn freeze_undo_apis_are_nops_without_feature() {
    use crate::utils::text_edit_history::{
        can_redo, can_undo, redo_field, reset, undo_field, undo_slot_exists, will_user_edit_field,
        EditKind,
    };
    will_user_edit_field("freeze-off", EditKind::Insert);
    assert!(!undo_field("freeze-off"));
    assert!(!redo_field("freeze-off"));
    assert!(!can_undo("freeze-off"));
    assert!(!can_redo("freeze-off"));
    assert!(!undo_slot_exists("freeze-off"));
    reset("freeze-off");
}

#[cfg(feature = "text-undo")]
mod on {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use crate::utils::document_registry::DocumentRegistry;
    use crate::utils::text_buffer::{StringBuffer, TextBuffer};
    use crate::utils::text_edit_history::{
        can_undo, edit_group, install_field_config, last_payload_is_marker, note_after_edit,
        past_len, reset, undo, will_user_edit, with_test_replace_caps, EditKind, TextEditSnapshot,
        TextUndoConfig,
    };

    fn snap(value: &str, caret: usize) -> TextEditSnapshot {
        TextEditSnapshot {
            value: value.into(),
            caret,
            ..Default::default()
        }
    }

    #[test]
    fn freeze_feature_on_compiles_history() {
        assert!(cfg!(feature = "text-undo"));
    }

    #[test]
    fn freeze_password_never_records() {
        reset("fz_pw");
        install_field_config("fz_pw", TextUndoConfig::password_default());
        will_user_edit("fz_pw", snap("", 0), EditKind::Insert);
        note_after_edit("fz_pw", snap("secret", 6));
        assert_eq!(undo("fz_pw"), None);
        reset("fz_pw");
    }

    #[test]
    fn freeze_instant_coords_undo_reverses_txn() {
        reset("fz_txn");
        let id = "fz_txn";
        {
            let _g = edit_group(id, EditKind::Replace);
            will_user_edit(id, snap("a", 1), EditKind::Replace);
            note_after_edit(id, snap("ab", 2));
            will_user_edit(id, snap("ab", 2), EditKind::Replace);
            note_after_edit(id, snap("abc", 3));
        }
        assert_eq!(undo(id).unwrap().value, "a");
        reset(id);
    }

    #[test]
    fn freeze_guard_drop_on_panic_starts_new_group() {
        reset("fz_panic");
        let id = "fz_panic";
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _g = edit_group(id, EditKind::Insert);
            will_user_edit(id, snap("", 0), EditKind::Insert);
            note_after_edit(id, snap("a", 1));
            panic!("txn unwind");
        }));
        assert!(panicked.is_err());
        will_user_edit(id, snap("a", 1), EditKind::Insert);
        note_after_edit(id, snap("ab", 2));
        assert_eq!(undo(id).unwrap().value, "a");
        assert_eq!(undo(id).unwrap().value, "");
        reset(id);
    }

    #[test]
    fn freeze_marker_with_old_keeps_earlier_steps() {
        reset("fz_mark");
        let id = "fz_mark";
        with_test_replace_caps(64, 1024 * 1024, || {
            will_user_edit(id, snap("", 0), EditKind::Insert);
            note_after_edit(id, snap("hello", 5));
            let huge = "x".repeat(200);
            will_user_edit(id, snap("hello", 5), EditKind::Replace);
            note_after_edit(id, snap(&huge, huge.len()));
            assert!(last_payload_is_marker(id));
            assert_eq!(undo(id).unwrap().value, "hello");
            assert_eq!(undo(id).unwrap().value, "");
        });
        reset(id);
    }

    #[test]
    fn freeze_unrestorable_marker_is_a_wall() {
        reset("fz_wall");
        let id = "fz_wall";
        with_test_replace_caps(64, 4, || {
            will_user_edit(id, snap("", 0), EditKind::Insert);
            note_after_edit(id, snap("hello", 5));
            let huge = "x".repeat(200);
            will_user_edit(id, snap("hello", 5), EditKind::Replace);
            note_after_edit(id, snap(&huge, huge.len()));
            assert!(last_payload_is_marker(id));
            assert!(!can_undo(id));
            assert_eq!(undo(id), None);
            assert_eq!(past_len(id), 2);
        });
        reset(id);
    }

    #[test]
    fn freeze_string_buffer_is_l1_l2_protocol() {
        let mut b = StringBuffer::from("hi");
        b.replace(2..2, "!");
        assert_eq!(b.as_str(), "hi!");
        b.replace(0..2, "");
        assert_eq!(b.as_str(), "!");
    }

    #[test]
    fn freeze_close_tab_b_does_not_touch_a() {
        let reg = DocumentRegistry::new();
        let a = reg
            .register("file:///freeze-a.rs", Some("keep".into()))
            .expect("a");
        let b = reg
            .register("file:///freeze-b.rs", Some("gone".into()))
            .expect("b");
        will_user_edit(a.field_id(), snap("keep", 4), EditKind::Insert);
        note_after_edit(a.field_id(), snap("keep!", 5));
        will_user_edit(b.field_id(), snap("gone", 4), EditKind::Insert);
        note_after_edit(b.field_id(), snap("gone!", 5));
        assert!(reg.unregister(b.uri()));
        assert_eq!(undo(a.field_id()).unwrap().value, "keep");
        reg.unregister(a.uri());
    }
}
