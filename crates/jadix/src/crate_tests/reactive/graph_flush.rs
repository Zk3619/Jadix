//! WP-G.1: `take_flushed_effects` is the authority for “who ran after this `set`”.

use crate::core::reactivity::{
    batch, create_memo, create_render_effect, create_signal, take_flushed_effects, EffectPhase,
};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn graph_flush_set_runs_only_subscribers() {
    let _ = take_flushed_effects();
    let a = create_signal(0_i32);
    let b = create_signal(0_i32);
    let a_runs = Rc::new(Cell::new(0_u32));
    let b_runs = Rc::new(Cell::new(0_u32));

    let a_id = create_render_effect({
        let a_runs = Rc::clone(&a_runs);
        move || {
            a_runs.set(a_runs.get() + 1);
            let _ = a.get();
        }
    });
    let b_id = create_render_effect({
        let b_runs = Rc::clone(&b_runs);
        move || {
            b_runs.set(b_runs.get() + 1);
            let _ = b.get();
        }
    });

    a_runs.set(0);
    b_runs.set(0);
    let _ = take_flushed_effects();

    a.set(1);
    let flushed = take_flushed_effects();
    assert_eq!(a_runs.get(), 1, "CPU-style leaf subscribed to `a` must run");
    assert_eq!(
        b_runs.get(),
        0,
        "Mem-style leaf subscribed to `b` must not run"
    );
    assert!(
        flushed
            .iter()
            .any(|(id, phase)| *id == a_id && *phase == EffectPhase::Render),
        "flush log must include the `a` Render effect, got {flushed:?}"
    );
    assert!(
        flushed.iter().all(|(id, _)| *id != b_id),
        "flush log must not include the `b` leaf, got {flushed:?}"
    );
}

#[test]
fn graph_flush_batch_collapses_duplicate_sets() {
    let a = create_signal(0_i32);
    let runs = Rc::new(Cell::new(0_u32));
    let id = create_render_effect({
        let runs = Rc::clone(&runs);
        move || {
            runs.set(runs.get() + 1);
            let _ = a.get();
        }
    });

    runs.set(0);
    let _ = take_flushed_effects();
    batch(|| {
        a.set(1);
        a.set(2);
    });
    let flushed = take_flushed_effects();
    assert_eq!(
        runs.get(),
        1,
        "batched sets must flush the leaf once (final value)"
    );
    assert_eq!(
        flushed
            .iter()
            .filter(|(eid, phase)| *eid == id && *phase == EffectPhase::Render)
            .count(),
        1,
        "same Render effect must appear once in the flush log, got {flushed:?}"
    );
}

#[test]
fn graph_flush_memo_unchanged_does_not_run_leaf() {
    let src = create_signal(1_i32);
    let memo = create_memo(move || src.get() % 2);
    let runs = Rc::new(Cell::new(0_u32));
    let leaf = create_render_effect({
        let runs = Rc::clone(&runs);
        move || {
            runs.set(runs.get() + 1);
            let _ = memo.get();
        }
    });

    runs.set(0);
    let _ = take_flushed_effects();
    src.set(3); // 3 % 2 == 1, memo value unchanged (I-4)
    let flushed = take_flushed_effects();
    assert_eq!(runs.get(), 0, "unchanged memo must not notify the leaf");
    assert!(
        flushed.iter().all(|(id, _)| *id != leaf),
        "flush log must omit the leaf when memo is PartialEq-equal, got {flushed:?}"
    );

    src.set(2); // 2 % 2 == 0, memo changes
    let flushed = take_flushed_effects();
    assert_eq!(runs.get(), 1);
    assert!(
        flushed
            .iter()
            .any(|(id, phase)| *id == leaf && *phase == EffectPhase::Render),
        "changed memo must run the leaf Render effect, got {flushed:?}"
    );
}
