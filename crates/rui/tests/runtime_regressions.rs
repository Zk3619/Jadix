use rui::app::damage::*;
use rui::core::reactivity::{batch, create_effect, create_memo, create_signal};
use rui::core::tree::{Element, LayoutTree};
use rui::style::{Animation, KeyframeEffect};
use std::{cell::Cell, rc::Rc};

#[test]
fn region_overlap_must_not_suppress_uncovered_node_pixels() {
    let node = PaintDirty {
        key: 1,
        bounds: [90.0, 40.0, 16.0, 16.0],
        kind: PaintDirtyKind::Node,
    };
    let region = PaintDirty {
        key: 2,
        bounds: [0.0, 0.0, 100.0, 100.0],
        kind: PaintDirtyKind::Region,
    };
    assert!(
        !paint_dirty_covered_by_overlay_restore(&node, &[region, node]),
        "node x=100..106 lies outside Region's zero-pad scissor and still needs paint"
    );
    let exact_region = PaintDirty {
        kind: PaintDirtyKind::Region,
        ..node
    };
    assert!(
        paint_dirty_covered_by_overlay_restore(&node, &[exact_region, node]),
        "an equal-sized Region already replayed the whole node"
    );
}

#[test]
fn overlay_cut_must_keep_all_visible_damage() {
    let damage = [0.0, 0.0, 100.0, 100.0];
    let overlay = [40.0, 0.0, 20.0, 100.0];
    let kept = page_scissors_excluding_overlay(damage, Some(overlay));
    assert!(kept.iter().all(|r| intersect_rects(*r, overlay).is_none()));
    let actual_area: f32 = kept.iter().map(|r| r[2] * r[3]).sum();
    assert_eq!(
        actual_area, 8000.0,
        "both 40px-wide exposed sides must repaint"
    );
}

#[test]
fn memo_pull_inside_batch_must_notify_subscriber() {
    let source = create_signal(1);
    let derived = create_memo(move || source.get() * 2);
    let observed = Rc::new(Cell::new(0));
    create_effect({
        let observed = observed.clone();
        move || observed.set(derived.get())
    });
    batch(|| {
        source.set(2);
        assert_eq!(derived.get(), 4);
    });
    assert_eq!(
        observed.get(),
        4,
        "subscriber must see the memo value pulled inside batch"
    );
}

#[test]
fn memo_chain_must_refresh_when_read_inside_batch() {
    let source = create_signal(1);
    let a = create_memo(move || source.get() * 2);
    let b = create_memo(move || a.get() + 1);
    batch(|| {
        source.set(2);
        assert_eq!(b.get(), 5, "transitive memo read must be fresh");
    });
}

#[test]
fn animation_finish_callback_can_read_handle() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    anim.on_finish(move || {
        let _ = anim.current_time();
    });
    anim.finish();
}

#[test]
fn animation_finished_subscriber_can_read_handle() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    let finished = anim.finished();
    create_effect(move || {
        if finished.get() {
            let _ = anim.play_state();
        }
    });
    anim.finish();
}

#[test]
fn animation_callbacks_can_restart_and_cancel_reentrantly() {
    let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
    let calls = Rc::new(Cell::new(0));
    anim.on_finish({
        let calls = calls.clone();
        move || {
            assert_eq!(anim.current_time(), 1000.0);
            calls.set(calls.get() + 1);
            anim.play();
        }
    });
    anim.finish();
    assert_eq!(calls.get(), 1);
    assert!(!anim.finished().get());
    anim.on_cancel(move || {
        assert!(anim.cancelled().get());
        anim.play();
    });
    anim.cancel();
    assert!(!anim.cancelled().get());
}

#[test]
fn memo_pull_preserves_equality_and_notifies_once_at_batch_exit() {
    let source = create_signal(1);
    let parity = create_memo(move || source.get() % 2);
    let computes = Rc::new(Cell::new(0));
    let downstream = create_memo({
        let computes = computes.clone();
        move || {
            computes.set(computes.get() + 1);
            parity.get() + 10
        }
    });
    let effects = Rc::new(Cell::new(0));
    create_effect({
        let effects = effects.clone();
        move || {
            let _ = downstream.get();
            effects.set(effects.get() + 1);
        }
    });
    computes.set(0);
    effects.set(0);
    batch(|| {
        source.set(3);
        assert_eq!(downstream.peek(), 11);
    });
    assert_eq!(
        computes.get(),
        0,
        "equal upstream output must not recompute downstream"
    );
    assert_eq!(effects.get(), 0);
    batch(|| {
        source.set(4);
        assert_eq!(downstream.get(), 10);
        assert_eq!(effects.get(), 0, "pull must not flush effects inside batch");
    });
    assert_eq!(computes.get(), 1);
    assert_eq!(effects.get(), 1);
}

#[test]
fn shared_memo_diamonds_validate_once_per_write() {
    let source = create_signal(1_i64);
    let mut value = create_memo(move || source.get());
    for _ in 0..24 {
        let previous = value;
        let left = create_memo(move || previous.get() + 1);
        let right = create_memo(move || previous.get() - 1);
        value = create_memo(move || left.get() + right.get());
    }
    batch(|| {
        source.set(2);
        assert_eq!(value.get(), 2_i64 << 24);
        assert_eq!(value.peek(), 2_i64 << 24);
    });
}

#[test]
fn nested_z_index_must_keep_lower_text_before_upper_fill() {
    let pill = |z, label: &str, x| {
        Element::new()
            .absolute()
            .left(x)
            .top(20.0)
            .w(120.0)
            .h(40.0)
            .z_index(z)
            .bg([0.2, 0.4, 0.9, 1.0])
            .child(Element::new().text(label))
    };
    let arena = Element::new()
        .w(300.0)
        .h(150.0)
        .relative()
        .child(pill(1, "low", 10.0))
        .child(pill(2, "high", 40.0));
    let bar = Element::new()
        .absolute()
        .right(0.0)
        .w(8.0)
        .h(100.0)
        .z_index(1000);
    let mut root = Element::new()
        .w(400.0)
        .h(200.0)
        .relative()
        .child(arena)
        .child(bar);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let ranges = result.overlay_split.page_stack_band_ranges();
    let band = |label: &str| {
        ranges.iter().position(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == label)
        })
    };
    let lo = band("low").expect("low text band");
    let hi = band("high").expect("high text band");
    assert!(
        lo < hi,
        "low text and high fill must not share a band: low={lo}, high={hi}"
    );
}
