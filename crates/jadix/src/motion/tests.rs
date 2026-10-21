use super::*;
use crate::app::pointer_events::PointerEvent;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::path::{ATTR_PATH_D, PathCommand};
use crate::style::{TimingFunction, tick_animations};
use std::cell::Cell;

fn linear() -> ValueDriver {
    ValueDriver::tween(100.0, TimingFunction::Linear)
}

fn sync(element: &mut Element) -> LayoutStatus {
    element
        .reactive_sync
        .clone()
        .unwrap()
        .run(element, &mut LayoutTree::new())
}

fn advance(element: &mut Element, milliseconds: f32) -> LayoutStatus {
    let mut remaining = milliseconds;
    while remaining > 0.001 {
        let step = remaining.min(50.0);
        tick_animations(step / 1000.0);
        remaining -= step;
    }
    sync(element)
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

#[test]
fn motion_initial_animates_paint_without_relayout() {
    let state = MotionState::new();
    let mut element = Element::new().motion(
        &state,
        Motion::new(MotionTarget::new().translate([80.0, 20.0]).opacity(1.0))
            .initial(MotionTarget::new().translate([0.0, 0.0]).opacity(0.0))
            .transition(linear()),
    );
    close(element.style.opacity, 0.0);
    assert_eq!(advance(&mut element, 50.0), LayoutStatus::StyleChanged);
    close(element.style.translate[0], 40.0);
    close(element.style.opacity, 0.5);
    advance(&mut element, 50.0);
    close(element.style.opacity, 1.0);
    assert!(!state.is_active());
    assert_eq!(sync(&mut element), LayoutStatus::Clean);
}

#[test]
fn motion_interaction_priority_and_baseline_restore() {
    let state = MotionState::new();
    let mut element = Element::new().scale(0.9).motion(
        &state,
        Motion::new(MotionTarget::new())
            .while_hover(MotionTarget::new().scale(1.1))
            .while_tap(MotionTarget::new().scale(0.8))
            .transition(linear()),
    );
    (element.on_hover.clone().unwrap().borrow_mut())(true);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.scale[0], 1.1);
    let event = PointerEvent::new(0.0, 0.0, winit::event::MouseButton::Left, 1);
    (element.on_mousedown.clone().unwrap().borrow_mut())(&event);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.scale[0], 0.8);
    (element.on_hover.clone().unwrap().borrow_mut())(false);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.scale[0], 0.9);
}

#[test]
fn motion_focus_drag_and_disabled_variants() {
    let state = MotionState::new();
    let mut element = Element::new().focus_id("motion-focus-test").motion(
        &state,
        Motion::new(MotionTarget::new().opacity(1.0))
            .while_focus(MotionTarget::new().opacity(0.8))
            .while_drag(MotionTarget::new().opacity(0.4))
            .transition(linear()),
    );
    use_signal("__jadix_button_focus_id", String::new).set("motion-focus-test".to_owned());
    use_signal("__jadix_focus_visible", || false).set(true);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.opacity, 0.8);
    state.set_dragging(true);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.opacity, 0.4);
    element.set_attr("aria-disabled", "true");
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.opacity, 1.0);
    use_signal("__jadix_button_focus_id", String::new).set(String::new());
    use_signal("__jadix_focus_visible", || false).set(false);
}

#[test]
fn motion_keeps_handlers_and_merges_sync_impact() {
    let events = Rc::new(Cell::new(0));
    let previous = events.clone();
    let mut element = Element::new()
        .on_hover(move |_| previous.set(previous.get() + 1))
        .reactive_sync(|_, _| LayoutStatus::StructureChanged)
        .motion(
            &MotionState::new(),
            Motion::new(MotionTarget::new()).while_hover(MotionTarget::new().scale(1.1)),
        );
    (element.on_hover.clone().unwrap().borrow_mut())(true);
    assert_eq!(events.get(), 1);
    assert_eq!(sync(&mut element), LayoutStatus::StructureChanged);
}

#[test]
fn motion_dimensions_reflow_and_colors_paint() {
    let state = MotionState::new();
    let mut element = Element::new().w(100.0).motion(
        &state,
        Motion::new(
            MotionTarget::new()
                .width(200.0)
                .background_color([1.0, 0.0, 0.0, 1.0]),
        )
        .initial(
            MotionTarget::new()
                .width(100.0)
                .background_color([0.0, 0.0, 1.0, 1.0]),
        )
        .transition(linear()),
    );
    assert_eq!(advance(&mut element, 50.0), LayoutStatus::LayoutChanged);
    close(
        crate::layout::dim_length_px(element.style.size.width).unwrap(),
        150.0,
    );
    close(element.style.bg_color[0], 0.5);
    close(element.style.bg_color[2], 0.5);
}

#[test]
fn motion_retarget_and_rebuild_keep_live_presentation() {
    let state = MotionState::new();
    let motion = |destination| {
        Motion::new(MotionTarget::new().translate([destination, 0.0]))
            .initial(MotionTarget::new().translate([0.0, 0.0]))
            .transition(linear())
    };
    let mut element = Element::new().motion(&state, motion(100.0));
    advance(&mut element, 50.0);
    close(element.style.translate[0], 50.0);
    let mut rebuilt = Element::new().motion(&state, motion(200.0));
    close(rebuilt.style.translate[0], 50.0);
    advance(&mut rebuilt, 50.0);
    close(rebuilt.style.translate[0], 125.0);
}

#[test]
fn motion_spring_retarget_preserves_velocity() {
    let state = MotionState::new();
    let mut element = Element::new().motion(
        &state,
        Motion::new(MotionTarget::new().translate([100.0, 0.0]))
            .initial(MotionTarget::new().translate([0.0, 0.0])),
    );
    advance(&mut element, 50.0);
    let velocity = state.0.borrow().tracks[&PropertyId::Translate].channels[0].velocity();
    assert!(velocity > 0.0);
    state.set_target(MotionTarget::new().translate([-100.0, 0.0]));
    sync(&mut element);
    close(
        state.0.borrow().tracks[&PropertyId::Translate].channels[0].velocity(),
        velocity,
    );
}

#[test]
fn motion_reduced_motion_finishes_in_flight_tracks() {
    let state = MotionState::new();
    let mut element = Element::new().motion(
        &state,
        Motion::new(MotionTarget::new().opacity(1.0))
            .initial(MotionTarget::new().opacity(0.0))
            .transition(linear()),
    );
    advance(&mut element, 25.0);
    crate::style::set_prefers_reduced_motion(Some(true));
    sync(&mut element);
    crate::style::set_prefers_reduced_motion(None);
    close(element.style.opacity, 1.0);
    assert!(!state.is_active());
}

#[test]
fn motion_jump_cancels_pending_targets_velocity_and_queue() {
    let number = AnimatedNumber::spring(0.0, 100.0).then_to(200.0, linear());
    tick_animations(0.03);
    number.set(300.0);
    number.jump(42.0);
    tick_animations(0.5);
    close(number.peek(), 42.0);
    close(number.velocity(), 0.0);
    assert!(!number.is_active());
    assert!(number.finished().peek());
}

#[test]
fn motion_layout_projection_matches_paint_hit_and_children() {
    let motion = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    let mut root = Element::new().w(400.0).h(200.0).child(
        Element::new()
            .absolute()
            .left(20.0)
            .top(20.0)
            .w(100.0)
            .h(60.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .layout_motion(&motion)
            .child(Element::new().w(20.0).h(20.0).bg([0.0, 0.0, 1.0, 1.0])),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    root.children[0].style.inset.left = taffy::style::LengthPercentageAuto::length(120.0);
    root.children[0].style.size.width = taffy::style::Dimension::length(200.0);
    let first = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let shell = first
        .instances
        .iter()
        .find(|instance| instance.color == [1.0, 0.0, 0.0, 1.0])
        .unwrap();
    close(shell.rect_pos[0], 20.0);
    close(shell.rect_size[0], 100.0);
    let hit = crate::core::tree::visual_paint_aabb(&root.children[0], [0.0; 2]);
    close(hit[0], shell.rect_pos[0]);
    close(hit[2], shell.rect_size[0]);
    close(root.children[0].style.translate[0], 0.0);
    close(root.children[0].style.scale[0], 1.0);
    tick_animations(0.05);
    let middle = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let shell = middle
        .instances
        .iter()
        .find(|instance| instance.color == [1.0, 0.0, 0.0, 1.0])
        .unwrap();
    close(shell.rect_pos[0], 70.0);
    close(shell.rect_size[0], 150.0);
    assert!(crate::layout::collect_cache::node_paint_needs_frame(
        &root.children[0]
    ));
    tick_animations(0.05);
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    assert!(!motion.is_active());
    close(
        crate::motion::presentation_translate(&root.children[0])[0],
        0.0,
    );
}

#[test]
fn motion_layout_interruption_starts_at_current_rectangle() {
    let motion = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    motion.update([0.0, 0.0, 100.0, 50.0]);
    motion.update([100.0, 0.0, 200.0, 50.0]);
    tick_animations(0.05);
    motion.update([200.0, 0.0, 300.0, 50.0]);
    close(motion.bounds().unwrap()[0], 50.0);
    tick_animations(0.05);
    close(motion.bounds().unwrap()[0], 125.0);
}

#[test]
fn motion_layout_zero_size_and_reduced_motion_are_finite() {
    let motion = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    motion.update([0.0, 0.0, 0.0, 0.0]);
    motion.update([20.0, 10.0, 100.0, 50.0]);
    let (translate, scale) = motion.projection([50.0, 25.0]);
    assert!(
        translate
            .iter()
            .chain(&scale)
            .all(|value| value.is_finite())
    );
    crate::style::set_prefers_reduced_motion(Some(true));
    motion.update([40.0, 30.0, 200.0, 80.0]);
    crate::style::set_prefers_reduced_motion(None);
    assert!(!motion.is_active());
    assert_eq!(motion.bounds(), Some([40.0, 30.0, 200.0, 80.0]));
}

#[test]
fn motion_prepared_paths_measure_close_without_connecting_subpaths() {
    let path = PreparedPath::new(&[
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::LineTo((10.0, 0.0)),
        PathCommand::Close,
        PathCommand::MoveTo((100.0, 0.0)),
        PathCommand::LineTo((100.0, 10.0)),
    ]);
    close(path.length(), 30.0);
    assert_eq!(path.sample(1.0).position, (100.0, 10.0));
    let trim = path.trim(0.5, 0.9);
    assert_eq!(
        trim.iter()
            .filter(|command| matches!(command, PathCommand::MoveTo(_)))
            .count(),
        2
    );
    assert!(path.trim(0.5, 0.5).is_empty());
}

#[test]
fn motion_prepared_curve_has_arc_length_sampling() {
    let path = PreparedPath::new(&[
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::QuadTo {
            ctrl: (50.0, 100.0),
            to: (100.0, 0.0),
        },
    ]);
    assert!(path.length() > 100.0);
    let sample = path.sample(0.5);
    close(sample.position.0, 50.0);
    assert!((sample.position.1 - 50.0).abs() < 0.2);
}

#[test]
fn motion_morph_validates_topology_and_keeps_curve_controls() {
    let from = [
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::QuadTo {
            ctrl: (20.0, 40.0),
            to: (60.0, 0.0),
        },
    ];
    let to = [
        PathCommand::MoveTo((10.0, 0.0)),
        PathCommand::QuadTo {
            ctrl: (40.0, 60.0),
            to: (80.0, 0.0),
        },
    ];
    let morph = PathMorph::new(&from, &to).unwrap();
    assert_eq!(
        morph.sample(0.5)[1],
        PathCommand::QuadTo {
            ctrl: (30.0, 50.0),
            to: (70.0, 0.0)
        }
    );
    assert!(PathMorph::new(&from, &[PathCommand::Close]).is_err());
    assert!(PathMorph::new(&[PathCommand::MoveTo((f32::NAN, 0.0))], &to).is_err());
}

#[test]
fn motion_path_reveal_binding_updates_retained_element() {
    let path = Rc::new(PreparedPath::new(&[
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::LineTo((100.0, 0.0)),
    ]));
    let progress = AnimatedNumber::tween(0.0, 1.0, 100.0, TimingFunction::Linear);
    let mut element = bind_path_reveal(Element::new(), path, progress);
    assert_eq!(element.attrs.get(ATTR_PATH_D).unwrap(), "");
    assert_eq!(advance(&mut element, 50.0), LayoutStatus::StyleChanged);
    let commands = crate::primitives::path::decode_path_d(element.attrs.get(ATTR_PATH_D).unwrap());
    assert_eq!(commands.last(), Some(&PathCommand::LineTo((50.0, 0.0))));
    assert_eq!(sync(&mut element), LayoutStatus::Clean);
}

#[test]
fn motion_path_reveal_collects_each_intermediate_geometry_without_relayout() {
    use crate::primitives::path::{PathStyle, attach_path};
    let commands = [
        PathCommand::MoveTo((4.0, 20.0)),
        PathCommand::LineTo((104.0, 20.0)),
    ];
    let path = Rc::new(PreparedPath::new(&commands));
    let progress = AnimatedNumber::tween(0.0, 1.0, 100.0, TimingFunction::Linear);
    let drawing = attach_path(
        Element::new().w(120.0).h(40.0),
        &commands,
        &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
    );
    let mut root = Element::new().child(bind_path_reveal(drawing, path, progress));
    let mut tree = LayoutTree::new();
    tree.set_collect_skip_allowed(true);
    let initial = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
    assert!(initial.textured_draws.is_empty());
    let mut previous_key = None;
    for _ in 0..4 {
        tick_animations(0.025);
        let mut status = LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut status,
            400.0,
            200.0,
            [0.0; 2],
        );
        assert_eq!(status, LayoutStatus::StyleChanged);
        let frame = tree.collect_animation_frame(&mut root, [-1.0; 2], false);
        let drawing = frame
            .textured_draws
            .iter()
            .find(|draw| draw.texture_key.starts_with("jadix_path:"))
            .expect("reveal must paint intermediate geometry, not replay its empty first frame");
        assert_ne!(previous_key.as_ref(), Some(&drawing.texture_key));
        previous_key = Some(drawing.texture_key.clone());
    }
}

#[test]
fn motion_path_follow_composes_authored_transform() {
    let path = Rc::new(PreparedPath::new(&[
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::LineTo((0.0, 100.0)),
    ]));
    let progress = AnimatedNumber::tween(0.0, 1.0, 100.0, TimingFunction::Linear);
    let mut element = bind_path_follow(Element::new().translate(10.0, 20.0), path, progress, true);
    advance(&mut element, 50.0);
    close(element.style.translate[0], 10.0);
    close(element.style.translate[1], 70.0);
    close(element.style.rotate_deg, 90.0);
}

#[test]
fn motion_restores_auto_dimensions_without_permanent_layout_work() {
    let state = MotionState::new();
    let mut element = Element::new().motion(
        &state,
        Motion::new(MotionTarget::new())
            .while_hover(MotionTarget::new().width(200.0))
            .transition(linear()),
    );
    (element.on_hover.clone().unwrap().borrow_mut())(true);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(
        crate::layout::dim_length_px(element.style.size.width).unwrap(),
        200.0,
    );
    (element.on_hover.clone().unwrap().borrow_mut())(false);
    assert_eq!(sync(&mut element), LayoutStatus::LayoutChanged);
    assert!(matches!(
        crate::layout::dim_kind(element.style.size.width),
        crate::layout::DimKind::Auto
    ));
    assert_eq!(sync(&mut element), LayoutStatus::Clean);
}

#[test]
fn motion_keyboard_tap_preserves_existing_handler_and_releases() {
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{Key, KeyCode, KeyLocation, PhysicalKey};
    let state = MotionState::new();
    let calls = Rc::new(Cell::new(0));
    let previous = calls.clone();
    let mut element = Element::new()
        .focus_id("motion-keyboard-test")
        .focusable(true)
        .on_click(|| {})
        .on_key_down(move |_| {
            previous.set(previous.get() + 1);
            false
        })
        .motion(
            &state,
            Motion::new(MotionTarget::new())
                .while_tap(MotionTarget::new().scale(0.8))
                .transition(linear()),
        );
    use_signal("__jadix_button_focus_id", String::new).set("motion-keyboard-test".into());
    use_signal("__jadix_focus_visible", || false).set(true);
    let mut event = KeyEvent {
        physical_key: PhysicalKey::Code(KeyCode::Space),
        logical_key: Key::Character(" ".into()),
        text: Some(" ".into()),
        location: KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: Some(" ".into()),
        key_without_modifiers: Key::Character(" ".into()),
    };
    assert!(!(element.on_key_down.clone().unwrap().borrow_mut())(
        event.clone()
    ));
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.scale[0], 0.8);
    event.state = ElementState::Released;
    (element.on_key_down.clone().unwrap().borrow_mut())(event);
    sync(&mut element);
    advance(&mut element, 100.0);
    close(element.style.scale[0], 1.0);
    assert_eq!(calls.get(), 2);
    use_signal("__jadix_button_focus_id", String::new).set(String::new());
    use_signal("__jadix_focus_visible", || false).set(false);
}

#[test]
fn motion_initial_delay_is_not_repeated_on_interruption() {
    let state = MotionState::new();
    let mut element = Element::new().motion(
        &state,
        Motion::new(MotionTarget::new().opacity(1.0))
            .initial(MotionTarget::new().opacity(0.0))
            .transition(linear())
            .delay(100.0),
    );
    advance(&mut element, 50.0);
    close(element.style.opacity, 0.0);
    advance(&mut element, 100.0);
    close(element.style.opacity, 0.5);
    state.set_target(MotionTarget::new().opacity(0.0));
    sync(&mut element);
    advance(&mut element, 50.0);
    close(element.style.opacity, 0.25);
}

#[test]
fn motion_layout_nested_position_does_not_double_parent_delta() {
    let parent = LayoutMotion::new(LayoutAnimation::Position).transition(linear());
    let child = LayoutMotion::new(LayoutAnimation::Position).transition(linear());
    let mut root = Element::new().child(
        Element::new()
            .absolute()
            .left(0.0)
            .top(0.0)
            .w(100.0)
            .h(60.0)
            .layout_motion(&parent)
            .child(
                Element::new()
                    .w(20.0)
                    .h(20.0)
                    .bg([0.0, 0.0, 1.0, 1.0])
                    .layout_motion(&child),
            ),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
    root.children[0].style.inset.left = taffy::style::LengthPercentageAuto::length(100.0);
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
    tick_animations(0.05);
    sync(&mut root.children[0]);
    let frame = tree.collect_animation_frame(&mut root, [-1.0; 2], false);
    let child_instance = frame
        .instances
        .iter()
        .find(|instance| instance.color == [0.0, 0.0, 1.0, 1.0])
        .unwrap();
    close(child_instance.rect_pos[0], 50.0);
    assert!(!child.is_active());
    tick_animations(0.05);
    sync(&mut root.children[0]);
    let frame = tree.collect_animation_frame(&mut root, [-1.0; 2], false);
    let child_instance = frame
        .instances
        .iter()
        .find(|instance| instance.color == [0.0, 0.0, 1.0, 1.0])
        .unwrap();
    close(child_instance.rect_pos[0], 100.0);
}

#[test]
fn motion_curve_after_close_starts_at_subpath_origin() {
    let path = PreparedPath::new(&[
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::LineTo((100.0, 0.0)),
        PathCommand::Close,
        PathCommand::QuadTo {
            ctrl: (0.0, 100.0),
            to: (0.0, 200.0),
        },
    ]);
    close(path.length(), 400.0);
}

#[test]
fn motion_layout_fast_reflow_retargets_before_cache_replay() {
    let motion = LayoutMotion::default().transition(linear());
    let mut root = Element::new()
        .flex_row()
        .child(Element::new().w(60.0).h(30.0))
        .child(
            Element::new()
                .w(30.0)
                .h(30.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .layout_motion(&motion),
        );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
    root.children[0].style.size.width = taffy::style::Dimension::length(160.0);
    tree.update_taffy_node_style(&root.children[0]);
    let frame = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [-1.0; 2], false, false);
    let card = frame
        .instances
        .iter()
        .find(|instance| instance.color == [1.0, 0.0, 0.0, 1.0])
        .unwrap();
    close(card.rect_pos[0], 60.0);
    assert!(motion.is_active());
    tick_animations(0.05);
    sync(&mut root.children[1]);
    let frame = tree.collect_animation_frame(&mut root, [-1.0; 2], false);
    let card = frame
        .instances
        .iter()
        .find(|instance| instance.color == [1.0, 0.0, 0.0, 1.0])
        .unwrap();
    close(card.rect_pos[0], 110.0);
}

#[test]
fn motion_frame_dirties_are_local_and_include_final_sample() {
    use crate::app::damage::{paint_element_key, take_frame_paint_nodes};
    let state = MotionState::new();
    let mut root = Element::new()
        .child(
            Element::new()
                .template_id("motion-damage-card")
                .w(40.0)
                .h(40.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .motion(
                    &state,
                    Motion::new(MotionTarget::new().translate([0.0, 0.0])).transition(linear()),
                ),
        )
        .child(
            Element::new()
                .template_id("motion-damage-neighbor")
                .w(40.0)
                .h(40.0),
        );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
    take_frame_paint_nodes();
    state.set_target(MotionTarget::new().translate([100.0, 0.0]));
    let mut status = LayoutStatus::Clean;
    crate::core::tree::sync_reactive_elements_subtree(
        &mut root,
        &mut tree,
        &mut status,
        400.0,
        200.0,
        [0.0; 2],
    );
    let key = paint_element_key(&root.children[0]);
    let neighbor = paint_element_key(&root.children[1]);
    for expected in [50.0, 100.0] {
        take_frame_paint_nodes();
        tick_animations(0.05);
        let mut status = LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut status,
            400.0,
            200.0,
            [0.0; 2],
        );
        assert_eq!(status, LayoutStatus::StyleChanged);
        let dirties = take_frame_paint_nodes();
        assert!(dirties.iter().any(|dirty| dirty.key == key));
        assert!(!dirties.iter().any(|dirty| dirty.key == neighbor));
        let frame = tree.collect_animation_frame(&mut root, [-1.0; 2], false);
        let card = frame
            .instances
            .iter()
            .find(|instance| instance.color == [1.0, 0.0, 0.0, 1.0])
            .unwrap();
        close(card.rect_pos[0], expected);
    }
    assert!(!state.is_active());
}

#[test]
fn motion_layout_nested_size_preserves_child_size_and_position() {
    let parent = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    let child = LayoutMotion::new(LayoutAnimation::Position).transition(linear());
    let mut root = Element::new().child(
        Element::new()
            .absolute()
            .left(0.0)
            .top(0.0)
            .w(100.0)
            .h(80.0)
            .layout_motion(&parent)
            .child(
                Element::new()
                    .absolute()
                    .left(10.0)
                    .top(12.0)
                    .w(20.0)
                    .h(16.0)
                    .bg([0.0, 0.0, 1.0, 1.0])
                    .layout_motion(&child),
            ),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
    root.children[0].style.inset.left = taffy::style::LengthPercentageAuto::length(100.0);
    root.children[0].style.size.width = taffy::style::Dimension::length(200.0);
    root.children[0].style.size.height = taffy::style::Dimension::length(120.0);
    for expected_x in [10.0, 60.0, 110.0] {
        let frame = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0; 2]);
        let chip = frame
            .instances
            .iter()
            .find(|instance| instance.color == [0.0, 0.0, 1.0, 1.0])
            .unwrap();
        close(chip.rect_pos[0], expected_x);
        close(chip.rect_pos[1], 12.0);
        close(chip.rect_size[0], 20.0);
        close(chip.rect_size[1], 16.0);
        let bounds = crate::core::tree::visual_paint_aabb(&root.children[0].children[0], [0.0; 2]);
        close(bounds[0], chip.rect_pos[0]);
        close(bounds[2], chip.rect_size[0]);
        tick_animations(0.05);
    }
}

fn shared_layout_scene(right: bool, motion: &LayoutMotion) -> Element {
    let chip = Element::new()
        .absolute()
        .left(if right { 20.0 } else { 10.0 })
        .top(if right { 15.0 } else { 10.0 })
        .w(if right { 80.0 } else { 40.0 })
        .h(if right { 40.0 } else { 20.0 })
        .bg([0.0, 0.0, 1.0, 1.0])
        .layout_motion(motion)
        .layout_id("shared-chip");
    let mut left = Element::new()
        .template_id("shared-left")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(160.0)
        .h(160.0);
    let mut destination = Element::new()
        .template_id("shared-right")
        .absolute()
        .left(300.0)
        .top(60.0)
        .w(160.0)
        .h(160.0);
    if right {
        destination.children.push(chip);
    } else {
        left.children.push(chip);
    }
    Element::new()
        .w(500.0)
        .h(240.0)
        .children(vec![left, destination])
}

fn shared_chip_bounds(frame: &crate::layout::RenderResult) -> [f32; 4] {
    let chip = frame
        .instances
        .iter()
        .find(|instance| instance.color == [0.0, 0.0, 1.0, 1.0])
        .unwrap();
    [
        chip.rect_pos[0],
        chip.rect_pos[1],
        chip.rect_size[0],
        chip.rect_size[1],
    ]
}

#[test]
fn motion_layout_id_reparents_fresh_handles_without_a_jump() {
    let source = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    let destination = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    let mut root = shared_layout_scene(false, &source);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    root = shared_layout_scene(true, &destination);
    for expected in [
        [30.0, 30.0, 40.0, 20.0],
        [175.0, 52.5, 60.0, 30.0],
        [320.0, 75.0, 80.0, 40.0],
    ] {
        let frame = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
        for (actual, expected) in shared_chip_bounds(&frame).into_iter().zip(expected) {
            close(actual, expected);
        }
        let hit = crate::core::tree::visual_paint_aabb(&root.children[1].children[0], [0.0; 2]);
        for (actual, expected) in hit.into_iter().zip(expected) {
            close(actual, expected);
        }
        tick_animations(0.05);
    }
    assert!(!destination.is_active());
}

#[test]
fn motion_layout_id_interrupts_from_last_presentation_and_copies_spring_velocity() {
    let source = LayoutMotion::new(LayoutAnimation::Both);
    let destination = LayoutMotion::new(LayoutAnimation::Both);
    let mut tree = LayoutTree::new();
    let mut root = shared_layout_scene(false, &source);
    tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    root = shared_layout_scene(true, &destination);
    tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    tick_animations(0.05);
    let middle = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    let previous = shared_chip_bounds(&middle);
    let velocity = destination.velocities()[0];
    let returning = LayoutMotion::new(LayoutAnimation::Both);
    root = shared_layout_scene(false, &returning);
    let frame = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    for (actual, expected) in shared_chip_bounds(&frame).into_iter().zip(previous) {
        close(actual, expected);
    }
    close(returning.velocities()[0], velocity);
}

#[test]
fn motion_layout_id_is_window_local_and_duplicates_do_not_handoff() {
    let source = LayoutMotion::new(LayoutAnimation::Both);
    let mut root = shared_layout_scene(false, &source);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    let destination = LayoutMotion::new(LayoutAnimation::Both);
    let mut second = shared_layout_scene(true, &destination);
    let frame = LayoutTree::new().build_and_layout(&mut second, 500.0, 240.0, [-1.0; 2]);
    close(shared_chip_bounds(&frame)[0], 320.0);
    assert!(!destination.is_active());
    second = shared_layout_scene(true, &destination);
    second.children[0]
        .children
        .push(Element::new().w(40.0).h(20.0).layout_id("shared-chip"));
    let frame = tree.build_and_layout(&mut second, 500.0, 240.0, [-1.0; 2]);
    close(shared_chip_bounds(&frame)[0], 320.0);
    assert!(!destination.is_active());
}

#[test]
fn motion_layout_id_retains_one_empty_frame_but_expires_stale_snapshots() {
    for empty_frames in [1, 2] {
        let mut tree = LayoutTree::new();
        let mut root = shared_layout_scene(false, &LayoutMotion::default());
        tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
        for _ in 0..empty_frames {
            tree.build_and_layout(&mut Element::new(), 500.0, 240.0, [-1.0; 2]);
        }
        root = shared_layout_scene(
            true,
            &LayoutMotion::new(LayoutAnimation::Both).transition(linear()),
        );
        let frame = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
        close(
            shared_chip_bounds(&frame)[0],
            if empty_frames == 1 { 30.0 } else { 320.0 },
        );
    }
}

#[test]
fn motion_layout_nested_correction_preserves_authored_scales() {
    let parent = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    let child = LayoutMotion::new(LayoutAnimation::Position).transition(linear());
    let mut root = Element::new().child(
        Element::new()
            .absolute()
            .left(0.0)
            .top(0.0)
            .w(100.0)
            .h(80.0)
            .scale(1.25)
            .layout_motion(&parent)
            .child(
                Element::new()
                    .absolute()
                    .left(10.0)
                    .top(12.0)
                    .w(20.0)
                    .h(16.0)
                    .scale(1.5)
                    .bg([0.0, 0.0, 1.0, 1.0])
                    .layout_motion(&child),
            ),
    );
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    root.children[0].style.inset.left = taffy::style::LengthPercentageAuto::length(100.0);
    root.children[0].style.size.width = taffy::style::Dimension::length(200.0);
    for expected_x in [-6.25, 37.5, 81.25] {
        let frame = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
        let bounds = shared_chip_bounds(&frame);
        close(bounds[0], expected_x);
        close(bounds[2], 37.5);
        close(bounds[3], 30.0);
        tick_animations(0.05);
    }
}

#[test]
fn motion_layout_id_converts_authored_parent_and_child_scales() {
    let mut tree = LayoutTree::new();
    let mut root = shared_layout_scene(false, &LayoutMotion::default());
    root.children[0].style.scale = [1.25; 2];
    root.children[0].children[0].style.scale = [1.2; 2];
    let source = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    let previous = shared_chip_bounds(&source);
    root = shared_layout_scene(
        true,
        &LayoutMotion::new(LayoutAnimation::Both).transition(linear()),
    );
    root.children[1].style.scale = [0.8; 2];
    root.children[1].children[0].style.scale = [0.9; 2];
    let frame = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    for (actual, expected) in shared_chip_bounds(&frame).into_iter().zip(previous) {
        close(actual, expected);
    }
}

#[test]
fn motion_layout_id_reduced_motion_snaps_and_hidden_nodes_do_not_own_id() {
    let mut tree = LayoutTree::new();
    let mut root = shared_layout_scene(false, &LayoutMotion::default());
    tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    let destination = LayoutMotion::new(LayoutAnimation::Both).transition(linear());
    root = shared_layout_scene(true, &destination);
    let mut hidden = Element::new().w(10.0).h(10.0).layout_id("shared-chip");
    hidden.style.display = taffy::style::Display::None;
    root.children[0].children.push(hidden);
    let frame = tree.build_and_layout(&mut root, 500.0, 240.0, [-1.0; 2]);
    close(shared_chip_bounds(&frame)[0], 30.0);
    crate::style::set_prefers_reduced_motion(Some(true));
    let frame = tree.collect_animation_frame(&mut root, [-1.0; 2], false);
    crate::style::set_prefers_reduced_motion(None);
    close(shared_chip_bounds(&frame)[0], 320.0);
    assert!(!destination.is_active());
}
