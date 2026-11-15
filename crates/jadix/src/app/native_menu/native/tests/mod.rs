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
    let a = build(items()).unwrap().callbacks;
    let b = build(items()).unwrap().callbacks;
    assert_eq!(a.len(), 1);
    assert!(!b.contains_key(a.keys().next().unwrap()));
    let disabled = build(vec![NativeMenuItem::submenu(
        "File",
        vec![NativeMenuItem::command("save", "Save", || {})],
    )
    .enabled(false)])
    .unwrap()
    .callbacks;
    assert!(disabled.is_empty());
}

#[cfg(feature = "i18n")]
#[test]
fn localized_native_roles_follow_the_application_locale() {
    crate::i18n::with_lock(|| {
        use jadix_core::menu::MenuRole;
        let previous = crate::i18n::locale();
        let items = || {
            vec![NativeMenuItem::submenu(
                "Authored menu",
                vec![
                    NativeMenuItem::localized_role("copy", MenuRole::Copy, || {}),
                    NativeMenuItem::command("paste", "Authored paste", || {}).role(MenuRole::Paste),
                ],
            )]
        };
        for (locale, expected) in [("zh-TW", "複製"), ("ja", "コピー"), ("fr", "Copy")] {
            crate::i18n::set_locale(locale);
            let built = build(items()).unwrap();
            assert_eq!(built.labels.len(), 1);
            assert_eq!(built.labels[0].1.text(), expected);
            let submenu = built.menu.items()[0].as_submenu().unwrap().clone();
            assert_eq!(submenu.text(), "Authored menu");
            assert_eq!(
                submenu.items()[1].as_predefined_menuitem().unwrap().text(),
                "Authored paste"
            );
        }
        crate::i18n::set_locale(previous);
    });
}
