#![cfg(feature = "motion")]

use jadix_native::{
    host::{HostError, NativeRoot, PlatformHost},
    layout::{LayoutError, MeasureRequest},
    motion::{
        DecayOptions, Ease, LayoutMotion, MotionError, MotionPolicy, MotionValue, Transition,
    },
    presentation::*,
};
use std::time::Duration;

fn time(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}
fn linear() -> Transition {
    Transition::Timing {
        duration: time(200),
        easing: Ease::Linear,
    }
}
fn spring() -> Transition {
    Transition::Spring {
        stiffness: 170.0,
        damping: 26.0,
        mass: 1.0,
    }
}

#[test]
fn timing_retargets_without_jumping_and_rejects_invalid_commands_atomically() {
    let mut value = MotionValue::new(0.0).unwrap();
    value.animate_to(100.0, linear(), time(0)).unwrap();
    assert_eq!(value.sample(time(100)).unwrap().value, 50.0);
    value.animate_to(150.0, linear(), time(100)).unwrap();
    assert_eq!(value.current().value, 50.0);
    assert_eq!(value.sample(time(200)).unwrap().value, 100.0);
    let before = value.current();
    assert_eq!(
        value.animate_to(f32::NAN, linear(), time(201)),
        Err(MotionError::InvalidValue)
    );
    assert_eq!(
        value.animate_to(
            1.0,
            Transition::Timing {
                duration: time(20_001),
                easing: Ease::Linear
            },
            time(201)
        ),
        Err(MotionError::InvalidOptions)
    );
    assert_eq!(value.sample(time(199)), Err(MotionError::TimeWentBackwards));
    assert_eq!(value.current(), before);
    assert_eq!(value.sample(time(300)).unwrap().value, 150.0);
    assert!(value.current().done);
    value.animate_to(10.0, spring(), time(300)).unwrap();
    value.jump(40.0).unwrap();
    assert_eq!(value.sample(time(400)).unwrap().value, 40.0);
    assert!(value.current().done);
}

#[test]
fn spring_preserves_velocity_and_decay_finishes_inside_bounds() {
    let mut value = MotionValue::new(0.0).unwrap();
    value.animate_to(100.0, spring(), time(0)).unwrap();
    let moving = value.sample(time(70)).unwrap();
    assert!(moving.velocity > 0.0 && moving.value > 0.0 && moving.value < 100.0);
    value.animate_to(-50.0, spring(), time(70)).unwrap();
    assert!((value.current().value - moving.value).abs() < 0.001);
    assert!((value.current().velocity - moving.velocity).abs() < 0.001);
    assert_eq!(value.sample(time(20_070)).unwrap().value, -50.0);
    value.jump(20.0).unwrap();
    value
        .decay(
            DecayOptions {
                velocity: 800.0,
                min: Some(0.0),
                max: Some(120.0),
                ..Default::default()
            },
            time(20_070),
        )
        .unwrap();
    assert_eq!(value.target(), 120.0);
    assert!(value.sample(time(20_100)).unwrap().value > 20.0);
    assert_eq!(value.sample(time(40_070)).unwrap().value, 120.0);
    assert!(value.current().done);
    let before = value.current();
    assert_eq!(
        value.decay(
            DecayOptions {
                min: Some(9.0),
                max: Some(1.0),
                ..Default::default()
            },
            time(40_071)
        ),
        Err(MotionError::InvalidOptions)
    );
    assert_eq!(value.current(), before);
}

#[derive(Default)]
struct Host {
    publishes: usize,
    disposed: bool,
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
            width: request.key.max_width.min(100.0),
            height: request.key.max_height.min(30.0),
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        if self.fail {
            return Err("injected publication failure".into());
        }
        self.publishes += 1;
        Ok(())
    }
    fn dispose(&mut self) {
        self.disposed = true;
    }
}

fn tree(owner: RootId) -> Vec<PresentationNode> {
    let node = |content, width, height, padding| PresentationNode {
        id: NodeId::new(owner),
        descriptor: NodeDescriptor {
            content,
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
                width: Some(width),
                height: Some(height),
                padding: [padding; 4],
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    };
    let mut root = node(NodeContent::Container, 400.0, 300.0, 0.0);
    let mut panel = node(NodeContent::Container, 200.0, 120.0, 10.0);
    panel.children.push(node(
        NodeContent::Button {
            label: "Native child".into(),
            enabled: true,
        },
        100.0,
        30.0,
        0.0,
    ));
    root.children.push(panel);
    vec![root]
}

fn present(
    root: &mut NativeRoot<Host>,
    tree: &mut [PresentationNode],
    motion: &mut LayoutMotion,
    milliseconds: u64,
    policy: MotionPolicy,
) -> Result<(), HostError> {
    root.present_with_motion(tree, [400.0, 300.0], 1, motion, time(milliseconds), policy)
        .map(|_| ())
}

#[test]
fn layout_projects_native_descendants_retargets_and_stops_idle_commits() {
    let mut root = NativeRoot::new(Host::default());
    let mut motion = LayoutMotion::new(root.model().lifetime());
    let mut nodes = tree(root.model().id());
    let panel = nodes[0].children[0].id;
    let button = nodes[0].children[0].children[0].id;
    motion.transition(panel, Some(linear())).unwrap();
    present(&mut root, &mut nodes, &mut motion, 0, MotionPolicy::Full).unwrap();
    nodes[0].descriptor.style.padding = [40.0; 4];
    present(&mut root, &mut nodes, &mut motion, 100, MotionPolicy::Full).unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 0.0);
    present(&mut root, &mut nodes, &mut motion, 200, MotionPolicy::Full).unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 20.0);
    assert_eq!(root.model().node(button).unwrap().descriptor.frame.x, 30.0);
    nodes[0].descriptor.style.padding = [80.0; 4];
    present(&mut root, &mut nodes, &mut motion, 250, MotionPolicy::Full).unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 30.0);
    present(&mut root, &mut nodes, &mut motion, 450, MotionPolicy::Full).unwrap();
    assert_eq!(root.model().node(button).unwrap().descriptor.frame.x, 90.0);
    assert!(!motion.is_active());
    assert_eq!(motion.next_frame_delay(), None);
    let publishes = root.host().publishes;
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        1_000,
        MotionPolicy::Full,
    )
    .unwrap();
    assert_eq!(root.host().publishes, publishes);
    nodes[0].children.clear();
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        1_001,
        MotionPolicy::Full,
    )
    .unwrap();
    assert_eq!(motion.tracked_nodes(), 0);
}

#[test]
fn paused_clock_does_not_catch_up_and_reduced_motion_snaps_to_the_target() {
    let mut root = NativeRoot::new(Host::default());
    let mut motion = LayoutMotion::new(root.model().lifetime());
    let mut nodes = tree(root.model().id());
    let panel = nodes[0].children[0].id;
    motion.transition(panel, Some(linear())).unwrap();
    present(&mut root, &mut nodes, &mut motion, 0, MotionPolicy::Full).unwrap();
    nodes[0].descriptor.style.padding = [40.0; 4];
    present(&mut root, &mut nodes, &mut motion, 10, MotionPolicy::Full).unwrap();
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        110,
        MotionPolicy::Paused,
    )
    .unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 0.0);
    assert!(!motion.is_active());
    let publishes = root.host().publishes;
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        10_110,
        MotionPolicy::Paused,
    )
    .unwrap();
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        20_110,
        MotionPolicy::Full,
    )
    .unwrap();
    assert_eq!(root.host().publishes, publishes);
    assert!(motion.is_active());
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        20_210,
        MotionPolicy::Full,
    )
    .unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 20.0);
    nodes[0].descriptor.style.padding = [80.0; 4];
    present(
        &mut root,
        &mut nodes,
        &mut motion,
        20_211,
        MotionPolicy::Reduced,
    )
    .unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 80.0);
    assert!(!motion.is_active());
}

#[test]
fn environment_motion_preserves_clock_and_rejects_disposed_preferences() {
    use jadix_native::environment::*;
    let (owner, preferences) =
        jadix_core::create_root(|| EnvironmentHost::new(EnvironmentSnapshot::default()).unwrap());
    let environment = preferences.environment();
    let mut root = NativeRoot::new(Host::default());
    let mut motion = LayoutMotion::new(root.model().lifetime());
    let mut nodes = tree(root.model().id());
    let panel = nodes[0].children[0].id;
    motion.transition(panel, Some(linear())).unwrap();
    let render = |root: &mut NativeRoot<Host>,
                  nodes: &mut [PresentationNode],
                  motion: &mut LayoutMotion,
                  ms| {
        root.present_environment_with_motion(nodes, [400.0, 300.0], &environment, motion, time(ms))
    };
    render(&mut root, &mut nodes, &mut motion, 0).unwrap();
    nodes[0].descriptor.style.padding = [40.0; 4];
    render(&mut root, &mut nodes, &mut motion, 10).unwrap();
    for lifecycle in [Lifecycle::Inactive, Lifecycle::Background] {
        let mut next = environment.snapshot();
        next.lifecycle = lifecycle;
        preferences.apply(environment.revision(), next).unwrap();
        render(
            &mut root,
            &mut nodes,
            &mut motion,
            1000 * environment.revision(),
        )
        .unwrap();
        assert!(!motion.is_active());
        assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 0.0);
    }
    let mut next = environment.snapshot();
    next.lifecycle = Lifecycle::Active;
    preferences.apply(environment.revision(), next).unwrap();
    render(&mut root, &mut nodes, &mut motion, 10_000).unwrap();
    render(&mut root, &mut nodes, &mut motion, 10_100).unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 20.0);
    let mut next = environment.snapshot();
    next.reduced_motion = Some(true);
    preferences.apply(environment.revision(), next).unwrap();
    render(&mut root, &mut nodes, &mut motion, 10_101).unwrap();
    assert_eq!(root.model().node(panel).unwrap().descriptor.frame.x, 40.0);
    assert!(!motion.is_active());
    assert_eq!(root.inspection(false)["environment"]["revision"], "5");
    let publishes = root.host().publishes;
    let tracks = motion.tracked_nodes();
    owner.dispose();
    assert!(render(&mut root, &mut nodes, &mut motion, 10_102).is_err());
    assert_eq!(root.host().publishes, publishes);
    assert_eq!(motion.tracked_nodes(), tracks);
}

#[cfg(feature = "consistent-ui")]
#[test]
fn consistent_motion_projects_preferences_without_mutating_authored_styles() {
    use jadix_native::environment::*;
    let (_owner, preferences) = jadix_core::create_root(|| {
        EnvironmentHost::new(EnvironmentSnapshot {
            font_scale: 1.5,
            reduced_motion: Some(true),
            ..Default::default()
        })
        .unwrap()
    });
    let environment = preferences.environment();
    let mut root = NativeRoot::new(Host::default());
    let mut motion = LayoutMotion::new(root.model().lifetime());
    let nodes = tree(root.model().id());
    let authored = nodes.clone();
    let panel = nodes[0].children[0].id;
    let button = nodes[0].children[0].children[0].id;
    motion.transition(panel, Some(linear())).unwrap();
    root.present_consistent_with_motion(
        &nodes,
        [400.0, 300.0],
        &environment,
        &jadix_native::ConsistentTheme::default(),
        &mut motion,
        time(0),
    )
    .unwrap();
    assert_eq!(nodes, authored);
    assert_eq!(
        root.model()
            .node(button)
            .unwrap()
            .descriptor
            .style
            .font_size,
        24.0
    );
    assert!(!motion.is_active());
    root.host_mut().fail = true;
    let mut changed = nodes.clone();
    changed[0].descriptor.style.padding = [40.0; 4];
    assert!(root
        .present_consistent_with_motion(
            &changed,
            [400.0, 300.0],
            &environment,
            &jadix_native::ConsistentTheme::default(),
            &mut motion,
            time(1)
        )
        .is_err());
    assert!(root.host().disposed);
    assert_eq!(motion.tracked_nodes(), 0);
}

#[test]
fn foreign_owners_time_reversal_budget_and_disposal_cannot_publish_motion() {
    let mut root = NativeRoot::new(Host::default());
    let mut nodes = tree(root.model().id());
    let mut foreign = LayoutMotion::new(RootLifetime::new(RootId::new()));
    assert!(matches!(
        present(&mut root, &mut nodes, &mut foreign, 0, MotionPolicy::Full),
        Err(HostError::Motion(MotionError::ForeignRoot))
    ));
    assert_eq!(root.host().publishes, 0);
    let mut motion = LayoutMotion::new(root.model().lifetime());
    for _ in 0..256 {
        motion
            .transition(NodeId::new(root.model().id()), Some(linear()))
            .unwrap();
    }
    assert_eq!(
        motion.transition(nodes[0].id, Some(linear())),
        Err(MotionError::TrackLimit)
    );
    present(&mut root, &mut nodes, &mut motion, 10, MotionPolicy::Full).unwrap();
    motion.transition(nodes[0].id, Some(linear())).unwrap();
    let publishes = root.host().publishes;
    assert!(matches!(
        present(&mut root, &mut nodes, &mut motion, 9, MotionPolicy::Full),
        Err(HostError::Motion(MotionError::TimeWentBackwards))
    ));
    assert_eq!(root.host().publishes, publishes);
    root.dispose();
    assert!(!motion.is_active());
    assert_eq!(motion.tracked_nodes(), 0);
    assert_eq!(
        motion.transition(nodes[0].id, Some(linear())),
        Err(MotionError::Disposed)
    );
}

#[test]
fn platform_publication_failure_revokes_the_motion_and_root_together() {
    let mut root = NativeRoot::new(Host::default());
    let mut nodes = tree(root.model().id());
    let mut motion = LayoutMotion::new(root.model().lifetime());
    let panel = nodes[0].children[0].id;
    motion.transition(panel, Some(spring())).unwrap();
    present(&mut root, &mut nodes, &mut motion, 0, MotionPolicy::Full).unwrap();
    nodes[0].descriptor.style.padding = [40.0; 4];
    present(&mut root, &mut nodes, &mut motion, 1, MotionPolicy::Full).unwrap();
    root.host_mut().fail = true;
    assert!(matches!(
        present(&mut root, &mut nodes, &mut motion, 50, MotionPolicy::Full),
        Err(HostError::Platform(_))
    ));
    assert!(root.host().disposed);
    assert!(!motion.is_active());
    assert_eq!(motion.tracked_nodes(), 0);
}
