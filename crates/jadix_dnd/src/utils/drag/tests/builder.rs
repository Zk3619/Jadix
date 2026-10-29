use super::*;
use crate::core::reactivity::{create_render_effect, create_root};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn live_slot_notifies_when_index_changes() {
    let (_o, _) = create_root(|| {
        let runs = Rc::new(Cell::new(0));
        let runs2 = runs.clone();
        let slot = live_slot("drag-live-idx", Some(1usize));
        create_render_effect(move || {
            let _ = slot.get();
            runs2.set(runs2.get() + 1);
        });
        assert_eq!(runs.get(), 1);
        live_slot("drag-live-idx", Some(0usize));
        assert!(
            runs.get() >= 2,
            "for_keyed remap must refresh frozen island index; runs={}",
            runs.get()
        );
        assert_eq!(slot.peek(), Some(0));
    });
}

#[test]
fn cross_list_slot_changes_layout_once_without_dimension_animation() {
    let (_owner, ()) = create_root(|| {
        let active = Signal::new(Some("row".to_string()));
        let from = Signal::new(Some("source".to_string()));
        let hover = Signal::new(Some("target".to_string()));
        let extent = Signal::new([120.0, 24.0]);
        let list = Signal::new(Some("source".to_string()));
        let rank = Signal::new(0);
        let bumped = Cell::new(false);
        let author_z = Cell::new(0);
        let margin = Cell::new(None);
        let block = Cell::new(None);
        let overflow = Cell::new(None);
        let collapsed = Cell::new(false);
        let mut shell = keyed_item_shell(Element::new().h(24.0));
        let authored = block_dims(&shell, Axis::Y);
        let sync = |el: &mut Element| {
            apply_shell_stack_z(
                el,
                "row",
                "slot-regression",
                active,
                DragStack::Keep,
                rank,
                &bumped,
                &author_z,
                from,
                hover,
                extent,
                Axis::Y,
                list,
                &margin,
                &block,
                &overflow,
                &collapsed,
            )
        };
        assert_eq!(sync(&mut shell), LayoutStatus::LayoutChanged);
        assert!(dims_eq(
            shell.style.size.height,
            taffy::style::Dimension::length(0.0)
        ));
        assert!(shell.style.transition.is_empty());
        for _ in 0..8 {
            assert_eq!(sync(&mut shell), LayoutStatus::Clean);
            assert!(!motion_is_animating(
                &shell.style,
                &shell.transition_snapshot
            ));
        }
        hover.set(Some("source".into()));
        assert_eq!(sync(&mut shell), LayoutStatus::LayoutChanged);
        assert!(dims_eq(
            shell.style.size.height,
            taffy::style::Dimension::length(24.0)
        ));
        assert_eq!(sync(&mut shell), LayoutStatus::Clean);
        active.set(None);
        assert_eq!(sync(&mut shell), LayoutStatus::LayoutChanged);
        assert!(!dims_changed(block_dims(&shell, Axis::Y), authored));

        hover.set(Some("target".into()));
        let mut target = Element::new().p(4.0);
        let target_pad = Cell::new(None);
        let target_margin = Cell::new(None);
        let target_position = Cell::new(None);
        let over = Signal::new(Some("target".into()));
        let hover_index = Signal::new(Some(0));
        let overlay_open = Signal::new(false);
        let sync_target = |el: &mut Element| {
            apply_live_droppable_chrome(
                el,
                "slot-regression",
                "target",
                "target",
                over,
                from,
                hover,
                hover_index,
                overlay_open,
                extent,
                Axis::Y,
                &target_pad,
                &target_margin,
                &target_position,
                0,
            )
        };
        assert_eq!(sync_target(&mut target), LayoutStatus::LayoutChanged);
        assert!(crate::layout::lp_length_px(target.style.padding.bottom).unwrap() > 4.0);
        assert!(target.style.transition.is_empty());
        for _ in 0..8 {
            assert_eq!(sync_target(&mut target), LayoutStatus::Clean);
        }
        hover.set(Some("source".into()));
        assert_eq!(sync_target(&mut target), LayoutStatus::LayoutChanged);
        assert_eq!(
            crate::layout::lp_length_px(target.style.padding.bottom),
            Some(4.0)
        );
        assert_eq!(sync_target(&mut target), LayoutStatus::Clean);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn sortable_shift_flip_animates_mid_then_snaps_on_session_end() {
    use crate::style::{
        set_css_transition_timeline_ms, set_css_transition_virtual_timeline,
        set_prefers_reduced_motion, TimingFunction, TransitionEntry, TransitionProperty,
    };
    use crate::utils::motion::{ensure_motion_transitions, snap_motion_baseline};
    set_prefers_reduced_motion(Some(false));
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    el.style.translate = [0.0, 0.0];
    ensure_motion_transitions(
        &mut el.style,
        &[TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: SORTABLE_FLIP_MS,
            delay_ms: 0.0,
            timing: TimingFunction::Ease,
        }],
    );
    snap_motion_baseline(&mut el);
    el.style.translate = [0.0, 40.0];
    assert!(motion_is_animating(&el.style, &el.transition_snapshot));
    let _ = el.tick_css_transitions(0.0);
    set_css_transition_timeline_ms(f64::from(SORTABLE_FLIP_MS) * 0.5);
    el.style.translate = [0.0, 40.0];
    let _ = el.tick_css_transitions(0.0);
    assert!(
        el.style.translate[1] > 8.0 && el.style.translate[1] < 36.0,
        "direct motion helpers mid, got {}",
        el.style.translate[1]
    );
    let mut motion = MotionStyle::new();
    apply_item_shift_motion(&mut el, [0.0, 24.0], 1.0, false, true, true, &mut motion);
    assert!(
        !el.style.transition.is_empty(),
        "live session must attach FLIP transition"
    );
    assert_eq!(el.style.translate[1], 24.0);
    assert!(apply_item_shift_motion(
        &mut el,
        [0.0, 0.0],
        1.0,
        false,
        false,
        true,
        &mut motion
    ));
    assert_eq!(el.style.translate, [0.0, 0.0]);
    assert!(!motion_is_animating(&el.style, &el.transition_snapshot));
    set_css_transition_virtual_timeline(false);
    set_prefers_reduced_motion(None);
}

#[test]
fn pointer_follow_skips_flip_transition() {
    let mut el = Element::new();
    let mut motion = MotionStyle::new();
    apply_item_shift_motion(&mut el, [12.0, 8.0], 1.0, true, true, true, &mut motion);
    assert_eq!(el.style.translate, [12.0, 8.0]);
    assert!(el.style.transition.is_empty());
    apply_item_shift_motion(&mut el, [20.0, 14.0], 1.0, true, true, true, &mut motion);
    assert_eq!(el.style.translate, [20.0, 14.0]);
    assert!(!motion_is_animating(&el.style, &el.transition_snapshot));
}

#[test]
fn sortable_shift_snaps_when_animation_disabled() {
    let mut el = Element::new();
    let mut motion = MotionStyle::new();
    apply_item_shift_motion(&mut el, [0.0, 40.0], 1.0, false, true, false, &mut motion);
    assert_eq!(el.style.translate, [0.0, 40.0]);
    assert!(
        el.style.transition.is_empty(),
        "animation=false must not attach FLIP transition"
    );
    apply_item_shift_motion(&mut el, [0.0, 0.0], 1.0, false, true, false, &mut motion);
    assert_eq!(el.style.translate, [0.0, 0.0]);
    assert!(!motion_is_animating(&el.style, &el.transition_snapshot));
}
