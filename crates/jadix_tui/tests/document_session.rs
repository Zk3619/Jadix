use jadix_tui::{
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    CheckpointStore, EditorCheckpoint, TerminalClipboard, TerminalEditor,
};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
fn command(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}
fn directory() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "jadix-editor-checkpoint-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}
#[test]
fn editor_checkpoint_restores_unicode_selection_undo_redo_and_fresh_identity() {
    let mut editor = TerminalEditor::new("中e\u{301}👩‍💻").unwrap();
    editor
        .set_selection(editor.document().len(), editor.document().len())
        .unwrap();
    editor.insert("\nnext").unwrap();
    editor.handle(&command('z')).unwrap();
    editor.set_selection(editor.document().len(), 0).unwrap();
    let bytes = editor.checkpoint_bytes().unwrap();
    let old_revision = editor.document().revision();
    let mut restored = TerminalEditor::new("unrelated").unwrap();
    restored
        .restore_checkpoint(restored.document().revision(), &bytes)
        .unwrap();
    assert_eq!(restored.document().text(), editor.document().text());
    assert_eq!(restored.selection(), editor.selection());
    assert_eq!(
        (restored.anchor(), restored.caret()),
        (editor.anchor(), editor.caret())
    );
    assert_ne!(restored.document().revision(), old_revision);
    assert!(restored.can_redo());
    restored.handle(&command('y')).unwrap();
    assert_eq!(restored.document().text(), "中e\u{301}👩‍💻\nnext");
    restored.handle(&command('z')).unwrap();
    assert_eq!(restored.document().text(), "中e\u{301}👩‍💻");
}
#[test]
fn external_revisions_and_malformed_checkpoints_never_overwrite_local_state() {
    let mut editor = TerminalEditor::new("private clipboard").unwrap();
    editor.handle(&command('a')).unwrap();
    editor.handle(&command('c')).unwrap();
    editor
        .apply_external(editor.document().revision(), "public")
        .unwrap();
    let bytes = editor.checkpoint_bytes().unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private clipboard"));
    let old = editor.document().revision();
    editor.insert("中").unwrap();
    let snapshot = editor.document().clone();
    let selection = editor.selection();
    assert_eq!(
        editor.restore_checkpoint(old, &bytes).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(editor.apply_external(old, "late result").is_err());
    for data in [b"{".to_vec(), b"{\"schema\":1,\"schema\":1}".to_vec()] {
        assert!(editor
            .restore_checkpoint(snapshot.revision(), &data)
            .is_err());
        assert_eq!(editor.document(), &snapshot);
    }
    let base: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for malformed in [
        {
            let mut v = base.clone();
            v["schema"] = 99.into();
            v
        },
        {
            let mut v = base.clone();
            v["current"]["caret"] = 999.into();
            v
        },
        {
            let mut v = base.clone();
            v["clipboard"] = "unexpected".into();
            v
        },
        {
            let mut v = base.clone();
            v["undo"] = serde_json::json!([{"text":"中","anchor":1,"caret":1}]);
            v
        },
        {
            let mut v = base.clone();
            v["undo"] = serde_json::json!(vec![
                serde_json::json!({"text":"","anchor":0,"caret":0});
                129
            ]);
            v
        },
        {
            let mut v = base.clone();
            v["redo"] = serde_json::json!(vec![
                serde_json::json!({"text":"x".repeat(1024*1024),"anchor":0,"caret":0});
                9
            ]);
            v
        },
    ] {
        assert!(editor
            .restore_checkpoint(
                snapshot.revision(),
                &serde_json::to_vec(&malformed).unwrap()
            )
            .is_err());
        assert_eq!(editor.document(), &snapshot);
        assert_eq!(editor.selection(), selection);
    }
    let same = editor.document().text().to_owned();
    assert!(!editor.apply_external(snapshot.revision(), same).unwrap());
    assert!(editor.can_undo());
    editor.apply_external(snapshot.revision(), "短").unwrap();
    assert!(!editor.can_undo() && !editor.can_redo());
    assert!(editor.document().text().is_char_boundary(editor.caret()));
    let mut escaped = TerminalEditor::new("\0".repeat(TerminalEditor::MAX_BYTES)).unwrap();
    for i in 0..3 {
        escaped
            .set_selection(escaped.document().len(), escaped.document().len())
            .unwrap();
        escaped.insert("x").unwrap_err();
        escaped.set_selection(0, 1).unwrap();
        escaped.insert(if i % 2 == 0 { "a" } else { "b" }).unwrap();
    }
    assert!(
        escaped.checkpoint_bytes().is_err(),
        "encoded budget must reject without truncating history"
    );
    assert_eq!(escaped.document().len(), TerminalEditor::MAX_BYTES);
}
#[test]
fn saved_history_generation_conflicts_locks_and_corruption_preserve_data() {
    let directory = directory();
    let path = directory.join("draft.json");
    let mut editor = TerminalEditor::new("draft").unwrap();
    editor.insert("中").unwrap();
    let mut first = EditorCheckpoint::new(&path).unwrap();
    assert_eq!(first.save(&editor).unwrap(), 1);
    let mut second = EditorCheckpoint::new(&path).unwrap();
    assert!(second.save(&editor).is_err());
    let mut restored = TerminalEditor::new("").unwrap();
    let revision = restored.document().revision();
    assert!(second.load(&mut restored, revision).unwrap());
    assert!(restored.can_undo());
    editor.insert("new").unwrap();
    assert_eq!(first.save(&editor).unwrap(), 2);
    let saved = fs::read(&path).unwrap();
    assert!(second.save(&restored).is_err());
    assert_eq!(second.generation(), Some(1));
    assert_eq!(fs::read(&path).unwrap(), saved);
    let lock = directory.join("draft.json.lock");
    fs::write(&lock, b"external owner").unwrap();
    assert!(first.save(&editor).is_err());
    assert_eq!(fs::read(&lock).unwrap(), b"external owner");
    fs::remove_file(lock).unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    corrupt["payload"] = "altered".into();
    fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    let snapshot = restored.document().clone();
    assert!(second.load(&mut restored, snapshot.revision()).is_err());
    assert_eq!(restored.document(), &snapshot);
    assert!(CheckpointStore::new(&path, 0).is_err());
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
#[test]
fn clipboard_opt_in_budget_and_io_errors_are_explicit() {
    let mut output = Vec::new();
    assert!(TerminalClipboard::new(false)
        .copy("text", &mut output)
        .is_err());
    assert!(output.is_empty());
    let clipboard = TerminalClipboard::new(true);
    assert!(clipboard.copy("", &mut output).is_err());
    assert!(clipboard
        .copy(&"x".repeat(TerminalClipboard::MAX_BYTES + 1), &mut output)
        .is_err());
    assert!(output.is_empty());
    let text = "中👩‍💻\n\x1b_Gi=1;OK\x1b\\";
    clipboard.copy(text, &mut output).unwrap();
    use base64::Engine;
    let encoded = &output[b"\x1b]52;c;".len()..output.len() - 2];
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
        text.as_bytes()
    );
    struct Failed;
    impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(clipboard.copy(text, &mut Failed).is_err());
}
