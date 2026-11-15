use super::*;
use jadix_native::{host::PlatformHost, layout::*, presentation::*};

struct Host;
impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: request.key.max_width.min(80.0),
            height: request.key.max_height.min(24.0),
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Ok(())
    }
    fn dispose(&mut self) {}
}
fn app() -> NotesApp<Host> {
    NotesApp::new(Host, [360.0, 640.0], 0, String::new()).unwrap()
}
fn node(app: &NotesApp<Host>, predicate: impl Fn(&NodeDescriptor) -> bool) -> NodeId {
    let mut pending = app.root.model().roots().to_vec();
    while let Some(id) = pending.pop() {
        let node = app.root.model().node(id).unwrap();
        if predicate(&node.descriptor) {
            return id;
        }
        pending.extend(&node.children);
    }
    panic!("node missing");
}
fn send(app: &mut NotesApp<Host>, id: NodeId, payload: Value) {
    app.event(
        &json!({"root":app.root.model().id().token(),"node":id.token(),"payload":payload})
            .to_string(),
    )
    .unwrap();
}
fn click(app: &mut NotesApp<Host>, label: &str) {
    let id = node(
        app,
        |descriptor| matches!(&descriptor.content,NodeContent::Button{label:current,..} if current==label),
    );
    send(app, id, json!({"kind":"activate"}));
}
fn edit(app: &mut NotesApp<Host>, placeholder: &str, value: &str) {
    let id = node(
        app,
        |descriptor| matches!(&descriptor.content,NodeContent::Input{placeholder:current,..}|NodeContent::Textarea{placeholder:current,..} if current==placeholder),
    );
    let revision = match app.root.model().node(id).unwrap().descriptor.content {
        NodeContent::Input { revision, .. } | NodeContent::Textarea { revision, .. } => {
            revision + 1
        }
        _ => unreachable!(),
    };
    let length = value.encode_utf16().count();
    send(
        app,
        id,
        json!({"kind":"edit","value":value,"revision":revision.to_string(),"selection":[length,length],"composing":null}),
    );
}
fn save(app: &mut NotesApp<Host>, title: &str, body: &str) {
    edit(app, "Title / 标题", title);
    edit(app, "Write a note / 正文", body);
    click(app, "Save note");
}
#[test]
fn storage_notifications_only_serialize_pending_documents_and_keep_retry_authority() {
    let mut app = app();
    assert!(app.storage_notification().is_none());
    edit(&mut app, "Title / 标题", "Unsaved");
    edit(&mut app, "Write a note / 正文", "中文😀");
    assert!(app.storage_notification().is_none());
    assert!(
        serde_json::from_str::<Value>(&app.state()).unwrap()["dirty"]
            .as_bool()
            .unwrap()
    );
    click(&mut app, "Save note");
    let saved = app.state();
    assert_eq!(app.storage_notification(), Some(saved));
    app.storage_result(Some("quota".into())).unwrap();
    assert_eq!(app.storage_notification(), Some(app.state()));
    app.storage_result(None).unwrap();
    assert!(app.storage_notification().is_none());
    let archive = serde_json::from_str::<Value>(&app.state()).unwrap()["archive"].clone();
    click(&mut app, "New note");
    assert!(app.storage_notification().is_none());
    assert_eq!(
        serde_json::from_str::<Value>(&app.state()).unwrap()["archive"],
        archive
    );
}
#[test]
fn notebook_preserves_unicode_and_stable_non_reused_identities() {
    let mut notebook = Notebook::default();
    let first = notebook
        .save(None, "中文😀".into(), "first\nsecond".into())
        .unwrap();
    notebook
        .save(Some(first), "Edited".into(), "body".into())
        .unwrap();
    assert_eq!(Notebook::decode(&notebook.encode()).unwrap(), notebook);
    notebook.notes.remove(&first);
    assert!(notebook.save(None, "Next".into(), String::new()).unwrap() > first);
}
#[test]
fn invalid_imports_preserve_the_original_notebook() {
    let mut notebook = Notebook::default();
    notebook
        .save(None, "Existing".into(), "safe".into())
        .unwrap();
    let original = notebook.clone();
    for source in [
        "{}",
        "[]",
        "{\"schema\":2}",
        "{\"schema\":1,\"next_id\":\"2\",\"notes\":[{\"id\":\"1\",\"title\":\"A\",\"body\":\"\"},{\"id\":\"1\",\"title\":\"B\",\"body\":\"\"}]}",
    ] {
        assert!(Notebook::decode(source).is_err());
        assert_eq!(notebook, original);
    }
}
#[test]
fn save_limits_are_atomic_and_utf8_based() {
    let mut notebook = Notebook::default();
    assert!(notebook.save(None, " ".into(), String::new()).is_err());
    assert!(notebook.save(None, "中".repeat(86), String::new()).is_err());
    assert!(notebook
        .save(None, "ok".into(), "x".repeat(MAX_BODY_BYTES + 1))
        .is_err());
    assert_eq!(notebook, Notebook::default());
    for index in 0..MAX_NOTES {
        notebook
            .save(None, index.to_string(), String::new())
            .unwrap();
    }
    let original = notebook.clone();
    assert!(notebook
        .save(None, "overflow".into(), String::new())
        .is_err());
    assert_eq!(notebook, original);
    notebook
        .save(Some(1), "still editable".into(), String::new())
        .unwrap();
}
#[test]
fn aggregate_size_and_identity_exhaustion_preserve_prior_data() {
    let mut notebook = Notebook::default();
    while notebook
        .save(None, "Large".into(), "x".repeat(MAX_BODY_BYTES))
        .is_ok()
    {}
    assert!(notebook.notes.len() < MAX_NOTES);
    let original = notebook.clone();
    assert!(notebook
        .save(None, "Large".into(), "x".repeat(MAX_BODY_BYTES))
        .is_err());
    assert_eq!(notebook, original);
    notebook.next_id = u64::MAX;
    let original = notebook.clone();
    assert!(notebook
        .save(None, "Exhausted".into(), String::new())
        .is_err());
    assert_eq!(notebook, original);
}
#[test]
fn actual_events_save_reopen_guard_dirty_drafts_and_revert() {
    let mut app = app();
    save(&mut app, "First 中文😀", "one\ntwo");
    click(&mut app, "New note");
    save(&mut app, "Second", "body");
    click(&mut app, "Open: First 中文😀");
    assert_eq!(app.body.get(), "one\ntwo");
    edit(&mut app, "Write a note / 正文", "unsaved");
    click(&mut app, "Open: Second");
    assert_eq!(app.selected, Some(1));
    assert!(app.status.get().contains("Save or revert"));
    click(&mut app, "Revert draft");
    assert!(!app.dirty());
    click(&mut app, "Open: Second");
    assert_eq!(app.selected, Some(2));
}
#[test]
fn search_and_list_identity_survive_nonstructural_updates() {
    let mut app = app();
    save(&mut app, "First", "中文 body");
    let first = node(
        &app,
        |d| matches!(&d.content,NodeContent::Button{label,..} if label=="Open: First"),
    );
    edit(&mut app, "Write a note / 正文", "draft");
    let after = node(
        &app,
        |d| matches!(&d.content,NodeContent::Button{label,..} if label=="Open: First"),
    );
    assert_eq!(first, after);
    edit(&mut app, "Search notes / 搜索", "中文");
    assert_eq!(
        node(
            &app,
            |d| matches!(&d.content,NodeContent::Button{label,..} if label=="Open: First")
        ),
        first
    );
    edit(&mut app, "Search notes / 搜索", "absent");
    node(
        &app,
        |d| matches!(&d.content,NodeContent::Text(text) if text=="No matching notes"),
    );
}
#[test]
fn dialog_cancel_preserves_data_and_confirmation_deletes() {
    let mut app = app();
    save(&mut app, "Keep", "body");
    click(&mut app, "Delete note");
    assert_eq!(app.notebook.notes.len(), 1);
    click(&mut app, "Cancel");
    assert_eq!(app.notebook.notes.len(), 1);
    click(&mut app, "Delete note");
    click(&mut app, "Confirm");
    assert!(app.notebook.notes.is_empty());
    assert_eq!(app.selected, None);
}
#[test]
fn malformed_import_does_not_discard_drafts_or_documents() {
    let mut app = app();
    save(&mut app, "Keep", "saved");
    edit(&mut app, "Write a note / 正文", "unsaved");
    edit(&mut app, "Notebook JSON for export or import", "{}");
    click(&mut app, "Import notebook");
    click(&mut app, "Confirm");
    assert_eq!(app.body.get(), "unsaved");
    assert_eq!(app.notebook.notes.len(), 1);
    assert!(!app.replacement_confirmed);
}
#[test]
fn export_import_and_recovery_keep_schema_and_draft_separate() {
    let mut original = app();
    save(&mut original, "Persistent", "saved");
    click(&mut original, "Export notebook");
    let exported = original.exchange.get();
    edit(&mut original, "Write a note / 正文", "unsaved draft");
    let mut restored = app();
    restored.restore(&original.state()).unwrap();
    assert_eq!(restored.body.get(), "unsaved draft");
    assert!(restored.dirty());
    let mut reopened = app();
    reopened.restore(&exported).unwrap();
    assert!(!reopened.dirty());
    assert_eq!(reopened.selected, None);
    click(&mut reopened, "Open: Persistent");
    assert_eq!(reopened.body.get(), "saved");
    edit(
        &mut reopened,
        "Notebook JSON for export or import",
        &Notebook::default().encode(),
    );
    click(&mut reopened, "Import notebook");
    click(&mut reopened, "Confirm");
    assert!(reopened.notebook.notes.is_empty());
    assert!(reopened.replacement_confirmed);
}
#[test]
fn storage_failures_and_oversized_edits_are_visible_and_recoverable() {
    let mut app = app();
    save(&mut app, "Safe", "saved");
    edit(
        &mut app,
        "Write a note / 正文",
        &"中".repeat(MAX_BODY_BYTES / 3 + 1),
    );
    assert_eq!(app.body.get(), "saved");
    assert!(app.status.get().contains("rejected"));
    app.storage_result(Some("quota".into())).unwrap();
    assert!(app.status.get().contains("Storage failed"));
    assert!(app.storage_pending);
    click(&mut app, "Save note");
    app.storage_result(None).unwrap();
    assert!(!app.storage_pending);
    assert_eq!(app.status.get(), "Saved to device storage");
    let old = app.state();
    assert!(app.restore("{}").is_err());
    assert_eq!(app.state(), old);
    app.root.dispose();
    assert!(app.restore(&old).is_err());
}

#[test]
fn storage_acknowledgement_consumes_only_successful_replacement() {
    let mut app = app();
    edit(
        &mut app,
        "Notebook JSON for export or import",
        &Notebook::default().encode(),
    );
    click(&mut app, "Import notebook");
    click(&mut app, "Confirm");
    app.storage_result(Some("retry".into())).unwrap();
    assert!(app.replacement_confirmed && app.storage_pending);
    app.storage_result(None).unwrap();
    assert!(!app.replacement_confirmed && !app.storage_pending);
}

#[test]
fn empty_list_and_editor_identities_are_stable_during_typing() {
    let mut app = app();
    let before = app.root.model().resource_usage().retired_nodes;
    for index in 0..100 {
        edit(&mut app, "Title / 标题", &index.to_string());
    }
    assert_eq!(app.root.model().resource_usage().retired_nodes, before);
}
