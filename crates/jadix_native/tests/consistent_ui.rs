#![cfg(feature = "consistent-ui")]

use jadix_native::presentation::style::NodeStyle;
use jadix_native::{environment::*, host::*, layout::*, presentation::*, ConsistentTheme};

#[derive(Default)]
struct Host {
    publications: usize,
}
impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: 80.0,
            height: 24.0,
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        self.publications += 1;
        Ok(())
    }
    fn dispose(&mut self) {}
}

fn editor(root: RootId) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                max_length: None,
                value: "中😀".into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
            frame: Rect::default(),
            style: Default::default(),
            accessible_description: None,
            accessible_name: Some("Name".into()),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}

#[test]
fn opt_in_theme_relayouts_without_mutating_authoring_or_replacing_control_identity() {
    let (owner, preferences) =
        jadix_core::create_root(|| EnvironmentHost::new(EnvironmentSnapshot::default()).unwrap());
    let mut root = NativeRoot::new(Host::default());
    let roots = vec![editor(root.model().id())];
    let source = roots.clone();
    let theme = ConsistentTheme::default();
    root.present_consistent(&roots, [320.0, 480.0], &preferences.environment(), &theme)
        .unwrap();
    let id = roots[0].id;
    assert_eq!(root.model().node(id).unwrap().descriptor.frame.height, 44.0);
    root.event(&PlatformEvent {
        node: id,
        payload: EventPayload::Focus,
    })
    .unwrap();
    let next = EnvironmentSnapshot {
        color_scheme: Some(ColorScheme::Dark),
        font_scale: 1.5,
        ..Default::default()
    };
    preferences.apply(1, next).unwrap();
    root.present_consistent(&roots, [320.0, 480.0], &preferences.environment(), &theme)
        .unwrap();
    let current = root.model().node(id).unwrap();
    assert_eq!(current.descriptor.style.font_size, 24.0);
    assert_eq!(current.descriptor.frame.height, 66.0);
    assert_eq!(current.descriptor.style.background, theme.dark_control);
    assert_eq!(root.model().roots(), &[id]);
    assert_eq!(roots, source);
    let publications = root.host().publications;
    root.present_consistent(&roots, [320.0, 480.0], &preferences.environment(), &theme)
        .unwrap();
    assert_eq!(root.host().publications, publications);
    let invalid = ConsistentTheme {
        font_size: f32::INFINITY,
        ..theme
    };
    assert!(root
        .present_consistent(&roots, [320.0, 480.0], &preferences.environment(), &invalid)
        .is_err());
    assert_eq!(root.host().publications, publications);
    owner.dispose();
    assert!(root
        .present_consistent(
            &roots,
            [320.0, 480.0],
            &preferences.environment(),
            &ConsistentTheme::default()
        )
        .is_err());
}

#[test]
fn native_default_and_explicit_styles_remain_available() {
    let mut root = NativeRoot::new(Host::default());
    let mut roots = vec![editor(root.model().id())];
    root.present(&mut roots, [320.0, 480.0], 0).unwrap();
    assert_eq!(
        root.model().node(roots[0].id).unwrap().descriptor.style,
        Default::default()
    );
    roots[0].descriptor.style.foreground = [1, 2, 3, 255];
    roots[0].descriptor.style.background = [4, 5, 6, 255];
    roots[0].descriptor.style.font_family = "custom-font".into();
    let result = ConsistentTheme::default()
        .project(&roots, &EnvironmentSnapshot::default())
        .unwrap();
    assert_eq!(result[0].descriptor.style.foreground, [1, 2, 3, 255]);
    assert_eq!(result[0].descriptor.style.background, [4, 5, 6, 255]);
    assert_eq!(result[0].descriptor.style.font_family, "custom-font");
    let mut invalid = roots.clone();
    invalid[0].descriptor.style.min_height = f32::NAN;
    assert!(ConsistentTheme::default()
        .project(&invalid, &EnvironmentSnapshot::default())
        .is_err());
}

#[test]
fn consistent_layout_direction_flows_to_shared_row_layout_and_respects_authored_override() {
    let (owner, preferences) = jadix_core::create_root(|| {
        EnvironmentHost::new(EnvironmentSnapshot {
            layout_direction: Some(LayoutDirection::RightToLeft),
            ..Default::default()
        })
        .unwrap()
    });
    let mut root = NativeRoot::new(Host::default());
    let root_id = root.model().id();
    let parent_id = NodeId::new(root_id);
    let child_id = NodeId::new(root_id);
    let child = PresentationNode {
        id: child_id,
        descriptor: NodeDescriptor {
            content: NodeContent::Container,
            frame: Rect::default(),
            style: NodeStyle {
                width: Some(20.0),
                height: Some(20.0),
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    };
    let mut parent = PresentationNode {
        id: parent_id,
        descriptor: NodeDescriptor {
            content: NodeContent::Container,
            frame: Rect::default(),
            style: NodeStyle {
                width: Some(100.0),
                height: Some(40.0),
                axis: jadix_native::presentation::style::Axis::Row,
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![child],
    };

    root.present_consistent(
        &[parent.clone()],
        [100.0, 40.0],
        &preferences.environment(),
        &ConsistentTheme::default(),
    )
    .unwrap();
    assert_eq!(
        root.model().node(child_id).unwrap().descriptor.frame.x,
        80.0
    );

    parent.descriptor.style.direction = Some(LayoutDirection::LeftToRight);
    root.present_consistent(
        &[parent],
        [100.0, 40.0],
        &preferences.environment(),
        &ConsistentTheme::default(),
    )
    .unwrap();
    assert_eq!(root.model().node(child_id).unwrap().descriptor.frame.x, 0.0);
    owner.dispose();
}

#[test]
fn consistent_theme_projects_control_colors_to_actionable_nodes() {
    let root = RootId::new();
    let action = |content, background| PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            style: NodeStyle {
                background,
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    };
    let mut roots = vec![
        action(
            NodeContent::Button {
                label: "Save".into(),
                enabled: true,
            },
            NodeStyle::default().background,
        ),
        action(
            NodeContent::Checkbox {
                label: "Remember".into(),
                checked: false,
                enabled: true,
                revision: 0,
            },
            NodeStyle::default().background,
        ),
        action(
            NodeContent::Switch {
                label: "Notifications".into(),
                checked: true,
                enabled: true,
                revision: 0,
            },
            [8, 9, 10, 255],
        ),
        action(
            NodeContent::Button {
                label: "Unavailable".into(),
                enabled: false,
            },
            NodeStyle::default().background,
        ),
    ];
    roots[0].descriptor.accessible_role = Some(AccessibleRole::RadioItem { tab_stop: true });

    let theme = ConsistentTheme::default();
    let projected = theme
        .project(
            &roots,
            &EnvironmentSnapshot {
                color_scheme: Some(ColorScheme::Dark),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(projected[0].descriptor.style.background, theme.dark_control);
    assert_eq!(projected[1].descriptor.style.background, theme.dark_control);
    assert_eq!(projected[2].descriptor.style.background, [8, 9, 10, 255]);
    assert_eq!(
        projected[3].descriptor.style.background,
        theme.dark_disabled_control
    );
    assert_eq!(
        projected[3].descriptor.style.foreground,
        theme.dark_disabled_text
    );
    assert_eq!(
        projected[3].descriptor.style.min_height,
        theme.control_height
    );
    assert_eq!(
        roots[0].descriptor.style.background,
        NodeStyle::default().background
    );
}

#[test]
fn high_contrast_theme_uses_opt_in_palette_and_preserves_authored_colors() {
    let root = RootId::new();
    let button = |enabled, background| PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content: NodeContent::Button {
                label: "Action".into(),
                enabled,
            },
            frame: Rect::default(),
            style: NodeStyle {
                background,
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    };
    let theme = ConsistentTheme::default();
    let roots = [
        button(true, NodeStyle::default().background),
        button(false, NodeStyle::default().background),
        button(true, [12, 34, 56, 255]),
    ];
    let projected = theme
        .project(
            &roots,
            &EnvironmentSnapshot {
                high_contrast: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(
        projected[0].descriptor.style.background,
        theme.high_contrast_control
    );
    assert_eq!(
        projected[0].descriptor.style.foreground,
        theme.high_contrast_text
    );
    assert_eq!(
        projected[1].descriptor.style.background,
        theme.high_contrast_disabled_control
    );
    assert_eq!(
        projected[1].descriptor.style.foreground,
        theme.high_contrast_disabled_text
    );
    assert_eq!(projected[2].descriptor.style.background, [12, 34, 56, 255]);
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn real_windows_editor_keeps_handle_selection_and_undo_during_theme_change() {
    use jadix_native::windows::WindowsHost;
    let (owner, preferences) =
        jadix_core::create_root(|| EnvironmentHost::new(EnvironmentSnapshot::default()).unwrap());
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix consistent native editor", [400, 280]).unwrap());
    let mut roots = vec![editor(root.model().id())];
    let id = roots[0].id;
    root.present_consistent(
        &roots,
        [400.0, 280.0],
        &preferences.environment(),
        &ConsistentTheme::default(),
    )
    .unwrap();
    let identity = root.host().control_identity(id).unwrap();
    root.host().show();
    root.focus(Some(id)).unwrap();
    root.host().replace_selection(id, "新增😀").unwrap();
    let text = root.host().text(id).unwrap();
    let selection = root.host().selection(id).unwrap();
    let events = root.poll_events().unwrap();
    for event in events {
        if let EventPayload::Edit {
            value, revision, ..
        } = event.payload
        {
            if let NodeContent::Input {
                value: current,
                revision: accepted,
                ..
            } = &mut roots[0].descriptor.content
            {
                *current = value;
                *accepted = revision;
            }
        }
    }
    preferences
        .apply(
            1,
            EnvironmentSnapshot {
                color_scheme: Some(ColorScheme::Dark),
                font_scale: 1.25,
                ..Default::default()
            },
        )
        .unwrap();
    root.present_consistent(
        &roots,
        [400.0, 280.0],
        &preferences.environment(),
        &ConsistentTheme::default(),
    )
    .unwrap();
    assert_eq!(root.host().control_identity(id), Some(identity));
    assert_eq!(root.host().selection(id).unwrap(), selection);
    assert_eq!(root.host().text(id).unwrap(), text);
    assert!(root.host().undo(id).unwrap());
    assert_ne!(root.host().text(id).unwrap(), text);
    root.dispose();
    owner.dispose();
}
