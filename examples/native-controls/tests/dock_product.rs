use jadix_native::{
    host::PlatformHost,
    layout::{LayoutError, MeasureRequest},
    presentation::*,
};
use jadix_native_controls::dock::{DockView, DockWorkspace};

#[derive(Default)]
struct Host {
    events: Vec<PlatformEvent>,
    fail: bool,
}

impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: 100.0f64.min(request.key.max_width),
            height: 32.0f64.min(request.key.max_height),
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        if self.fail {
            Err(String::from("injected native apply failure"))
        } else {
            Ok(())
        }
    }
    fn dispose(&mut self) {}
    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        Ok(std::mem::take(&mut self.events))
    }
}

#[test]
fn tabs_keep_edit_identity_preedit_is_not_checkpointed_and_restored_closed_panes_get_fresh_nodes() {
    let mut workspace = DockWorkspace::demo().unwrap();
    let mut view = DockView::new(Host::default(), 1);
    view.render(&workspace, [800.0, 600.0], 0).unwrap();
    let editor = view.editor(2).unwrap();
    workspace.layout.activate(2).unwrap();
    workspace.current = 2;
    view.render(&workspace, [800.0, 600.0], 0).unwrap();
    assert_eq!(view.editor(2), Some(editor));
    let original = workspace.value(2).unwrap().to_owned();
    let value = "预编辑 😀";
    let end = value.encode_utf16().count();
    view.root.host_mut().events.push(PlatformEvent {
        node: editor,
        payload: EventPayload::Edit {
            value: value.into(),
            revision: 1,
            selection_utf16: [end, end],
            composing_utf16: Some([0, end]),
        },
    });
    view.poll(&mut workspace).unwrap();
    assert!(workspace.composing());
    view.render(&workspace, [900.0, 600.0], 1).unwrap();
    assert_eq!(view.editor(2), Some(editor));
    assert_eq!(
        DockWorkspace::from_checkpoint(&workspace.checkpoint().unwrap())
            .unwrap()
            .value(2),
        Some(original.as_str())
    );
    view.root.host_mut().events.push(PlatformEvent {
        node: editor,
        payload: EventPayload::Edit {
            value: value.into(),
            revision: 2,
            selection_utf16: [end, end],
            composing_utf16: None,
        },
    });
    view.poll(&mut workspace).unwrap();
    assert!(!workspace.composing());
    assert_eq!(
        DockWorkspace::from_checkpoint(&workspace.checkpoint().unwrap())
            .unwrap()
            .value(2),
        Some(value)
    );
    workspace.layout.close(2).unwrap();
    workspace.current = 1;
    view.render(&workspace, [900.0, 600.0], 1).unwrap();
    assert!(view.editor(2).is_none());
    workspace.layout.undo().unwrap();
    view.render(&workspace, [900.0, 600.0], 1).unwrap();
    assert_ne!(view.editor(2), Some(editor));
    assert_eq!(workspace.value(2), Some(value));
    view.root.host_mut().fail = true;
    workspace.layout.activate(3).unwrap();
    assert!(view.render(&workspace, [900.0, 600.0], 1).is_err());
    assert!(view.root.model().lifetime().is_disposed());
}

#[test]
fn workspace_checkpoint_rejects_bad_inventory_and_replaces_an_existing_file_without_losing_it_on_failure(
) {
    let mut workspace = DockWorkspace::demo().unwrap();
    let bytes = workspace.checkpoint().unwrap();
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    invalid["documents"]["99"] = "foreign document".into();
    assert!(DockWorkspace::from_checkpoint(&serde_json::to_vec(&invalid).unwrap()).is_err());
    invalid = serde_json::from_slice(&bytes).unwrap();
    invalid["documents"]["2"] = "x".repeat(128 * 1024 + 1).into();
    assert!(DockWorkspace::from_checkpoint(&serde_json::to_vec(&invalid).unwrap()).is_err());
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../.theme-evidence/native/n14-20261104/workspace-test-{}",
        std::process::id()
    ));
    workspace.save(&directory).unwrap();
    workspace.layout.activate(3).unwrap();
    workspace.current = 3;
    workspace.save(&directory).unwrap();
    assert_eq!(DockWorkspace::load(&directory).unwrap().current, 3);
    let saved = std::fs::read(directory.join("workspace.json")).unwrap();
    workspace.current = 999;
    assert!(workspace.save(&directory).is_err());
    assert_eq!(
        std::fs::read(directory.join("workspace.json")).unwrap(),
        saved
    );
    let blocked = directory.join("blocked");
    std::fs::create_dir_all(blocked.join("workspace.json")).unwrap();
    std::fs::write(blocked.join("keep.tmp"), b"preexisting").unwrap();
    workspace.current = 3;
    assert!(workspace.save(&blocked).is_err());
    assert_eq!(
        std::fs::read(blocked.join("keep.tmp")).unwrap(),
        b"preexisting"
    );
    assert_eq!(std::fs::read_dir(&blocked).unwrap().count(), 2);
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn deferred_native_close_can_be_cancelled_and_drains_the_final_edit_before_root_disposal() {
    let mut workspace = DockWorkspace::demo().unwrap();
    let host =
        jadix_native::windows::WindowsHost::new("Jadix Dock close lifecycle", [800, 600]).unwrap();
    host.defer_close().unwrap();
    let mut view = DockView::new(host, 1);
    view.render(&workspace, [800.0, 600.0], 0).unwrap();
    let editor = view.editor(1).unwrap();
    let original = view.root.host().text(editor).unwrap();
    view.root
        .host()
        .select_range(editor, [0, original.encode_utf16().count()])
        .unwrap();
    view.root
        .host()
        .replace_selection(editor, "last native edit / 最后 😀")
        .unwrap();
    view.root.host().request_close().unwrap();
    view.poll(&mut workspace).unwrap();
    assert_eq!(workspace.value(1), Some("last native edit / 最后 😀"));
    assert!(view.root.host().close_requested());
    assert!(!view.root.host().is_closed());
    view.root.host().resolve_close(false).unwrap();
    assert!(!view.root.host().close_requested());
    assert!(view.root.host().resolve_close(true).is_err());
    view.render(&workspace, [800.0, 600.0], 0).unwrap();
    view.root.host().request_close().unwrap();
    view.poll(&mut workspace).unwrap();
    view.root.host().resolve_close(true).unwrap();
    assert!(view.root.host().is_closed());
    assert!(view.poll(&mut workspace).is_err());
    assert!(view.root.model().lifetime().is_disposed());
}
