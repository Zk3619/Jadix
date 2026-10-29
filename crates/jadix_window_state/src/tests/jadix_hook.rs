use super::*;

#[test]
fn saved_size_is_used_before_native_creation_and_respects_restore_policy() {
    let path = std::env::temp_dir().join(format!(
        "jadix-initial-attrs-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(
        &path,
        "[windows.main]\nwidth = 713\nheight = 451\nmaximized = true\ndecorated = false\n",
    )
    .unwrap();
    let prepare = |manager| {
        let hook = JadixWindowStateHook {
            manager: Arc::new(manager),
            ids: Mutex::new(HashMap::new()),
        };
        let mut attrs = WindowAttributes::default().with_visible(false);
        hook.on_creating("main", &mut attrs);
        attrs
    };
    let attrs = prepare(Builder::new().with_path(&path).build().unwrap());
    assert_eq!(
        attrs.surface_size,
        Some(winit::dpi::PhysicalSize::new(713, 451).into())
    );
    assert!(attrs.maximized);
    assert!(!attrs.decorations);
    assert!(!attrs.visible);
    let size_only = prepare(
        Builder::new()
            .with_path(&path)
            .with_state_flags(StateFlags::SIZE)
            .build()
            .unwrap(),
    );
    assert_eq!(size_only.surface_size, attrs.surface_size);
    assert!(!size_only.maximized);
    assert!(size_only.decorations);
    let skipped = prepare(
        Builder::new()
            .with_path(&path)
            .skip_initial_state("main")
            .build()
            .unwrap(),
    );
    assert_eq!(
        skipped.surface_size,
        WindowAttributes::default().surface_size
    );
    let denied = prepare(
        Builder::new()
            .with_path(&path)
            .with_denylist(&["main"])
            .build()
            .unwrap(),
    );
    assert_eq!(
        denied.surface_size,
        WindowAttributes::default().surface_size
    );
    std::fs::remove_file(path).unwrap();
}
