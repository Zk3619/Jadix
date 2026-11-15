#![cfg(all(windows, feature = "windows"))]
use jadix_native::{environment::*, host::NativeRoot, presentation::*, windows::WindowsHost};

#[test]
fn real_windows_preferences_and_native_edit_survive_environment_updates() {
    let snapshot = system_snapshot().expect("actual Windows preferences");
    println!(
        "system_environment={}",
        serde_json::to_string(&snapshot).unwrap()
    );
    let (owner, preferences) = jadix_core::create_root(|| EnvironmentHost::new(snapshot).unwrap());
    let environment = preferences.environment();
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix environment regression", [400, 220]).unwrap());
    let id = NodeId::new(root.model().id());
    let mut tree = [PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                max_length: None,
                value: "中😀 retained editor".into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
            frame: Rect::default(),
            accessible_description: None,
            accessible_name: Some("Environment continuity".into()),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            style: style::NodeStyle {
                width: Some(340.0),
                height: Some(42.0),
                ..Default::default()
            },
        },
        children: vec![],
    }];
    root.present_environment(&mut tree, [400.0, 220.0], &environment)
        .unwrap();
    let control = root.host().control_identity(id).unwrap();
    root.host().select_range(id, [1, 3]).unwrap();
    let before = root.host().text(id).unwrap();
    let selection = root.host().selection(id).unwrap();
    let mut next = environment.snapshot();
    next.font_scale = 1.25;
    next.locale.hour_cycle = Some(HourCycle::H12);
    preferences.apply(environment.revision(), next).unwrap();
    tree[0].descriptor.style.font_size = 20.0;
    let stats = root
        .present_environment(&mut tree, [400.0, 220.0], &environment)
        .unwrap();
    assert!(stats.measurements > 0);
    assert_eq!(root.host().control_identity(id), Some(control));
    assert_eq!(root.host().text(id).unwrap(), before);
    assert_eq!(root.host().selection(id).unwrap(), selection);
    root.host().replace_selection(id, "replacement").unwrap();
    let edited = root.host().text(id).unwrap();
    assert_ne!(edited, before);
    let mut next = environment.snapshot();
    next.color_scheme = Some(ColorScheme::Light);
    preferences.apply(environment.revision(), next).unwrap();
    root.present_environment(&mut tree, [400.0, 220.0], &environment)
        .unwrap();
    assert_eq!(
        root.host().text(id).unwrap(),
        edited,
        "unrelated preferences cannot reset user input"
    );
    assert!(root.host().undo(id).unwrap());
    assert_eq!(root.host().text(id).unwrap(), before);
    root.dispose();
    owner.dispose();
}
