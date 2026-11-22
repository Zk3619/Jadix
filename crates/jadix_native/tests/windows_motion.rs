#![cfg(all(windows, feature = "windows", feature = "motion"))]

use jadix_native::{
    host::NativeRoot,
    motion::{Ease, LayoutMotion, MotionPolicy, Transition},
    presentation::*,
    windows::WindowsHost,
};
use std::time::Duration;

#[test]
fn native_layout_animation_retains_edit_selection_undo_and_finishes_without_idle_publication() {
    let host = WindowsHost::new("Jadix Native layout motion", [480, 320]).unwrap();
    assert_eq!(host.motion_policy().unwrap(), MotionPolicy::Paused);
    let mut root = NativeRoot::new(host);
    let owner = root.model().id();
    let panel = NodeId::new(owner);
    let editor = NodeId::new(owner);
    let mut nodes = vec![PresentationNode {
        id: panel,
        descriptor: NodeDescriptor {
            content: NodeContent::Container,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(400.0),
                height: Some(250.0),
                ..Default::default()
            },
        },
        children: vec![PresentationNode {
            id: editor,
            descriptor: NodeDescriptor {
                content: NodeContent::Textarea {
                    max_length: None,
                    value: String::from("文档 😀"),
                    placeholder: String::new(),
                    enabled: true,
                    revision: 0,
                },
                frame: Rect::default(),
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                interaction: Default::default(),
                keyboard: Default::default(),
                navigation_keys: Vec::new(),
                accessibility: Default::default(),
                accessibility_relations: Default::default(),
                accessible_description: None,
                accessible_name: Some(String::from("Animated native editor")),
                style: style::NodeStyle {
                    width: Some(220.0),
                    height: Some(100.0),
                    shrink: 0.0,
                    ..Default::default()
                },
            },
            children: vec![],
        }],
    }];
    let mut motion = LayoutMotion::new(root.model().lifetime());
    motion
        .transition(
            editor,
            Some(Transition::Timing {
                duration: Duration::from_millis(200),
                easing: Ease::Linear,
            }),
        )
        .unwrap();
    root.present_with_motion(
        &mut nodes,
        [480.0, 320.0],
        1,
        &mut motion,
        Duration::ZERO,
        MotionPolicy::Full,
    )
    .unwrap();
    let identity = root.host().control_identity(editor).unwrap();
    root.host().show();
    root.host().pump();
    assert!(matches!(
        root.host().motion_policy().unwrap(),
        MotionPolicy::Full | MotionPolicy::Reduced
    ));
    root.host().select_range(editor, [5, 5]).unwrap();
    root.host().replace_selection(editor, " / 动画").unwrap();
    let events = root.poll_events().unwrap();
    let (value, revision) = events
        .iter()
        .rev()
        .find_map(|event| match &event.payload {
            EventPayload::Edit {
                value, revision, ..
            } => Some((value.clone(), *revision)),
            _ => None,
        })
        .unwrap();
    nodes[0].children[0].descriptor.content = NodeContent::Textarea {
        max_length: None,
        value: value.clone(),
        placeholder: String::new(),
        enabled: true,
        revision,
    };
    let selection = root.host().selection(editor).unwrap();
    nodes[0].descriptor.style.padding = [40.0; 4];
    for milliseconds in [10, 60, 110, 160, 210] {
        root.present_with_motion(
            &mut nodes,
            [480.0, 320.0],
            1,
            &mut motion,
            Duration::from_millis(milliseconds),
            MotionPolicy::Full,
        )
        .unwrap();
        assert_eq!(root.host().control_identity(editor), Some(identity));
        assert_eq!(root.host().text(editor).unwrap(), value);
        assert_eq!(root.host().selection(editor).unwrap(), selection);
    }
    assert_eq!(root.model().node(editor).unwrap().descriptor.frame.x, 40.0);
    assert!(!motion.is_active());
    let commits = root.host().stats().commits;
    root.present_with_motion(
        &mut nodes,
        [480.0, 320.0],
        1,
        &mut motion,
        Duration::from_secs(1),
        MotionPolicy::Full,
    )
    .unwrap();
    assert_eq!(root.host().stats().commits, commits);
    assert!(root.host().undo(editor).unwrap());
    assert_eq!(root.host().text(editor).unwrap(), "文档 😀");
    root.dispose();
    assert!(!motion.is_active());
    assert!(root.host().motion_policy().is_err());
}
