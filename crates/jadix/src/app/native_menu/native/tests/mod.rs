use super::*;

#[test]
fn rejects_duplicate_ids_and_invalid_shortcuts_before_install() {
    let cmd = || NativeMenuItem::command("save", "Save", || {});
    assert!(build(vec![NativeMenuItem::submenu("File", vec![cmd(), cmd()])]).is_err());
    assert!(build(vec![NativeMenuItem::submenu(
        "File",
        vec![cmd().accelerator("Ctrl+DefinitelyNotAKey")]
    )])
    .is_err());
    assert!(build(vec![cmd()]).is_err());
}

#[test]
fn generations_isolate_stale_events_and_disabled_ancestors() {
    let items = || {
        vec![NativeMenuItem::submenu(
            "File",
            vec![NativeMenuItem::command("save", "Save", || {}).accelerator("CmdOrCtrl+S")],
        )]
    };
    let (_, a) = build(items()).unwrap();
    let (_, b) = build(items()).unwrap();
    assert_eq!(a.len(), 1);
    assert!(!b.contains_key(a.keys().next().unwrap()));
    let (_, disabled) = build(vec![NativeMenuItem::submenu(
        "File",
        vec![NativeMenuItem::command("save", "Save", || {})],
    )
    .enabled(false)])
    .unwrap();
    assert!(disabled.is_empty());
}
