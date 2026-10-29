use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn dynamic_deps_branch_only_subscribes_active_arm() {
    let cond = Signal::new(true);
    let a = Signal::new(0_i32);
    let b = Signal::new(0_i32);
    let runs = Rc::new(RefCell::new(0_u32));

    create_effect({
        let runs = runs.clone();
        move || {
            *runs.borrow_mut() += 1;
            if cond.get() {
                let _ = a.get();
            } else {
                let _ = b.get();
            }
        }
    });

    assert_eq!(*runs.borrow(), 1);
    a.set(1);
    assert_eq!(*runs.borrow(), 2);
    b.set(1);
    assert_eq!(*runs.borrow(), 2);
    cond.set(false);
    assert_eq!(*runs.borrow(), 3);
    a.set(2);
    assert_eq!(*runs.borrow(), 3);
    b.set(2);
    assert_eq!(*runs.borrow(), 4);
}

#[test]
fn untrack_get_does_not_subscribe() {
    let a = Signal::new(0_i32);
    let runs = Rc::new(RefCell::new(0_u32));

    create_effect({
        let runs = runs.clone();
        move || {
            *runs.borrow_mut() += 1;
            untrack(|| {
                let _ = a.get();
            });
        }
    });

    assert_eq!(*runs.borrow(), 1);
    a.set(1);
    assert_eq!(*runs.borrow(), 1);
}

#[test]
fn isolate_reads_keeps_tracking_but_discards_deps() {
    let a = Signal::new(0_i32);
    let runs = Rc::new(RefCell::new(0_u32));
    let nested_saw_tracking = Rc::new(Cell::new(false));

    create_effect({
        let runs = runs.clone();
        let nested_saw_tracking = nested_saw_tracking.clone();
        move || {
            *runs.borrow_mut() += 1;
            isolate_reads(|| {
                nested_saw_tracking.set(is_tracking());
                let _ = a.get();
            });
        }
    });

    assert_eq!(*runs.borrow(), 1);
    assert!(nested_saw_tracking.get());
    a.set(1);
    assert_eq!(*runs.borrow(), 1);
}

#[test]
fn nested_create_effect_inner_updates_without_outer() {
    let _parent_dep = Signal::new(0_i32);
    let inner_dep = Signal::new(0_i32);
    let outer_runs = Rc::new(RefCell::new(0_u32));
    let inner_runs = Rc::new(RefCell::new(0_u32));

    create_effect({
        let outer_runs = outer_runs.clone();
        let inner_runs = inner_runs.clone();
        move || {
            *outer_runs.borrow_mut() += 1;
            let _ = _parent_dep.get();
            create_effect({
                let inner_runs = inner_runs.clone();
                move || {
                    *inner_runs.borrow_mut() += 1;
                    let _ = inner_dep.get();
                }
            });
        }
    });

    assert_eq!(*outer_runs.borrow(), 1);
    assert_eq!(*inner_runs.borrow(), 1);

    inner_dep.set(1);
    assert_eq!(*outer_runs.borrow(), 1);
    assert_eq!(*inner_runs.borrow(), 2);
}

#[test]
fn nested_create_effect_runs_after_parent_body() {
    let order = Rc::new(RefCell::new(Vec::new()));
    create_effect({
        let order = order.clone();
        move || {
            order.borrow_mut().push(1_u8);
            create_effect({
                let order = order.clone();
                move || {
                    order.borrow_mut().push(3);
                }
            });
            order.borrow_mut().push(2);
        }
    });
    assert_eq!(*order.borrow(), vec![1, 2, 3]);
}

#[test]
fn parent_effect_rerun_disposes_previous_child_effect() {
    let parent = Signal::new(0_i32);
    let child_runs = Rc::new(RefCell::new(0_u32));
    let cleanups = Rc::new(RefCell::new(0_u32));

    create_effect({
        let child_runs = child_runs.clone();
        let cleanups = cleanups.clone();
        move || {
            let _ = parent.get();
            create_effect({
                let child_runs = child_runs.clone();
                let cleanups = cleanups.clone();
                move || {
                    *child_runs.borrow_mut() += 1;
                    on_cleanup({
                        let cleanups = cleanups.clone();
                        move || {
                            *cleanups.borrow_mut() += 1;
                        }
                    });
                }
            });
        }
    });

    assert_eq!(*child_runs.borrow(), 1);
    assert_eq!(*cleanups.borrow(), 0);

    parent.set(1);
    // Old child cleaned up; new child ran once.
    assert_eq!(*cleanups.borrow(), 1);
    assert_eq!(*child_runs.borrow(), 2);

    parent.set(2);
    assert_eq!(*cleanups.borrow(), 2);
    assert_eq!(*child_runs.borrow(), 3);
}

#[test]
fn memo_recomputes_when_source_signal_changes() {
    let a = Signal::new(1_i32);
    let compute_runs = Rc::new(RefCell::new(0_u32));
    let memo = create_memo({
        let compute_runs = compute_runs.clone();
        move || {
            *compute_runs.borrow_mut() += 1;
            a.get() * 2
        }
    });

    let view_runs = Rc::new(RefCell::new(0_u32));
    create_effect({
        let view_runs = view_runs.clone();
        move || {
            *view_runs.borrow_mut() += 1;
            let _ = memo.get();
        }
    });

    assert_eq!(memo.get(), 2);
    assert_eq!(*view_runs.borrow(), 1);

    a.set(2);
    assert_eq!(memo.get(), 4);
    assert_eq!(*view_runs.borrow(), 2);
}

#[test]
fn memo_propagates_to_downstream_effect_when_source_changes() {
    let inner = Signal::new(0_i32);
    let memo = create_memo(move || inner.get());
    let outer_runs = Rc::new(RefCell::new(0_u32));
    create_effect({
        let outer_runs = outer_runs.clone();
        move || {
            *outer_runs.borrow_mut() += 1;
            let _ = memo.get();
        }
    });

    assert_eq!(*outer_runs.borrow(), 1);
    inner.set(1);
    assert_eq!(*outer_runs.borrow(), 2);
}

#[test]
fn memo_diamond_sees_consistent_values() {
    let a = Signal::new(1_i32);
    let left = create_memo(move || a.get() + 10);
    let right = create_memo(move || a.get() + 100);
    let seen = Rc::new(RefCell::new(Vec::new()));
    create_effect({
        let seen = seen.clone();
        move || {
            seen.borrow_mut().push((left.get(), right.get()));
        }
    });

    assert_eq!(*seen.borrow(), vec![(11, 101)]);
    a.set(2);
    // Must not observe (12, 101) or (11, 102) — only the consistent pair.
    assert_eq!(*seen.borrow(), vec![(11, 101), (12, 102)]);
}

#[test]
fn batch_runs_subscribed_effect_once_for_multiple_sets() {
    let a = Signal::new(0_i32);
    let b = Signal::new(0_i32);
    let runs = Rc::new(RefCell::new(0_u32));

    create_effect({
        let runs = runs.clone();
        move || {
            *runs.borrow_mut() += 1;
            let _ = a.get();
            let _ = b.get();
        }
    });

    assert_eq!(*runs.borrow(), 1);
    batch(|| {
        a.set(1);
        b.set(2);
    });
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn disposal_does_not_rerun_effects_with_removed_context() {
    let open = Signal::new(true);
    let internal_runs = Rc::new(Cell::new(0));
    let external_runs = Rc::new(Cell::new(0));
    let (_external_owner, ()) = create_root({
        let runs = external_runs.clone();
        move || {
            create_effect(move || {
                let _ = open.get();
                runs.set(runs.get() + 1);
            });
        }
    });
    let (owner, ()) = create_root({
        let runs = internal_runs.clone();
        move || {
            create_effect(move || {
                let _ = open.get();
                runs.set(runs.get() + 1);
            });
            on_cleanup(move || open.set(false));
        }
    });
    owner.dispose();
    assert_eq!(internal_runs.get(), 1);
    assert_eq!(external_runs.get(), 2);
}

#[test]
fn batch_end_flush_can_set_another_signal() {
    // Gallery `batch { mark_invalid.set }` / Command activate: effects flushed at
    // batch-end often `set` other signals. BatchGuard must not hold `batch_depth`
    // mutably across that flush (`RefCell already mutably borrowed` at notify).
    let src = Signal::new(0_i32);
    let derived = Signal::new(0_i32);
    create_effect(move || {
        let v = src.get();
        derived.set(v + 1);
    });
    assert_eq!(derived.peek(), 1);
    batch(|| {
        src.set(10);
    });
    assert_eq!(derived.peek(), 11);
}

#[test]
fn nested_batch_flushes_once_on_outermost_end() {
    let a = Signal::new(0_i32);
    let runs = Rc::new(RefCell::new(0_u32));

    create_effect({
        let runs = runs.clone();
        move || {
            *runs.borrow_mut() += 1;
            let _ = a.get();
        }
    });

    assert_eq!(*runs.borrow(), 1);
    batch(|| {
        batch(|| {
            a.set(1);
        });
        assert_eq!(*runs.borrow(), 1);
    });
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn signal_with_and_update() {
    let s = Signal::new(vec![1, 2, 3]);
    let sum = s.with(|v| v.iter().sum::<i32>());
    assert_eq!(sum, 6);
    s.update(|v| v.push(4));
    assert_eq!(s.get(), vec![1, 2, 3, 4]);
}

#[test]
fn create_root_and_on_cleanup() {
    let cleaned = Rc::new(Cell::new(0_u32));
    let (handle, sig) = create_root({
        let cleaned = cleaned.clone();
        move || {
            let s = Signal::new(1_i32);
            on_cleanup({
                let cleaned = cleaned.clone();
                move || cleaned.set(cleaned.get() + 1)
            });
            s
        }
    });
    assert_eq!(sig.peek(), 1);
    assert_eq!(cleaned.get(), 0);
    handle.dispose();
    assert_eq!(cleaned.get(), 1);
}

#[test]
fn on_mount_runs_once_not_on_signal_change() {
    let mounts = Rc::new(Cell::new(0_u32));
    let (_owner, sig) = create_root({
        let mounts = mounts.clone();
        move || {
            let s = Signal::new(0_i32);
            on_mount({
                let mounts = mounts.clone();
                move || mounts.set(mounts.get() + 1)
            });
            // A separate effect that tracks the signal — must not re-trigger on_mount.
            create_effect({
                move || {
                    let _ = s.get();
                }
            });
            s
        }
    });
    assert_eq!(mounts.get(), 1);
    sig.set(1);
    sig.set(2);
    assert_eq!(
        mounts.get(),
        1,
        "on_mount must not re-run on signal changes"
    );
}

#[test]
fn context_provide_and_use_walks_owners() {
    let ctx = create_context::<String>();
    let (_root, value) = create_root(|| {
        provide_context(ctx, "hello".to_string());
        create_effect(move || {
            // nested owner under effect
        });
        use_context(ctx)
    });
    assert_eq!(value.as_deref(), Some("hello"));
}

#[test]
fn sibling_context_isolated_with_child_owner() {
    let ctx = create_context::<String>();
    let (_root, (a, b)) = create_root(|| {
        let seen_a = Rc::new(RefCell::new(None::<String>));
        let seen_b = Rc::new(RefCell::new(None::<String>));
        with_child_owner({
            let seen_a = seen_a.clone();
            move || {
                provide_context(ctx, "hour".to_string());
                create_effect(move || {
                    *seen_a.borrow_mut() = use_context(ctx);
                });
            }
        });
        with_child_owner({
            let seen_b = seen_b.clone();
            move || {
                provide_context(ctx, "second".to_string());
                create_effect(move || {
                    *seen_b.borrow_mut() = use_context(ctx);
                });
            }
        });
        (seen_a, seen_b)
    });
    assert_eq!(a.borrow().as_deref(), Some("hour"));
    assert_eq!(b.borrow().as_deref(), Some("second"));
}

#[test]
fn use_scoped_signal_is_owner_relative() {
    let (_h1, a) = create_root(|| use_scoped_signal("counter", || 1_i32));
    let (_h2, b) = create_root(|| use_scoped_signal("counter", || 99_i32));
    // Different roots → independent slots despite same local id.
    assert_eq!(a.peek(), 1);
    assert_eq!(b.peek(), 99);
    a.set(2);
    assert_eq!(a.peek(), 2);
    assert_eq!(b.peek(), 99);
}

#[test]
fn scoped_template_and_focus_ids_differ_across_roots() {
    let (_h1, (t1, f1)) = create_root(|| (scoped_template_id("isle"), scoped_focus_id("btn")));
    let (_h2, (t2, f2)) = create_root(|| (scoped_template_id("isle"), scoped_focus_id("btn")));
    assert_ne!(t1, t2);
    assert_ne!(f1, f2);
    assert!(t1.contains("::tpl:isle"), "{t1}");
    assert!(f1.contains("::focus:btn"), "{f1}");
}

#[test]
fn clear_effects_disposes_root_without_refcell_panic() {
    let runs = Rc::new(Cell::new(0_u32));
    let (_handle, _) = create_root({
        let runs = runs.clone();
        move || {
            let s = Signal::new(0_i32);
            create_effect({
                let runs = runs.clone();
                move || {
                    let _ = s.get();
                    runs.set(runs.get() + 1);
                }
            });
        }
    });
    assert!(runs.get() >= 1);
    // Regression: nested `root_owner.borrow()` across `dispose_owner` used to panic
    // with `RefCell already borrowed` (Windows abort → 0xC0000409).
    clear_effects();
}

/// Gallery-style mount: many `create_effect` calls happen while `untrack` cleared
/// `current_effect` (e.g. `bind_prop` initial apply / memo refresh). Those must defer
/// onto the iterative flush queue — sync nesting overflows the Windows default stack.
#[test]
fn create_effect_under_untrack_defers_without_stack_overflow() {
    let runs = Rc::new(Cell::new(0_u32));
    let (_handle, _) = create_root({
        let runs = runs.clone();
        move || {
            create_effect({
                let runs = runs.clone();
                move || {
                    fn nest(left: u32, runs: &Rc<Cell<u32>>) {
                        if left == 0 {
                            return;
                        }
                        let runs = Rc::clone(runs);
                        untrack(|| {
                            create_effect(move || {
                                runs.set(runs.get() + 1);
                                nest(left - 1, &runs);
                            });
                        });
                    }
                    nest(2048, &runs);
                }
            });
        }
    });
    assert_eq!(runs.get(), 2048);
}

#[test]
fn effect_set_of_own_dep_does_not_reenter() {
    let runs = Rc::new(Cell::new(0_u32));
    let (_handle, value) = create_root({
        let runs = runs.clone();
        move || {
            let s = Signal::new(String::new());
            create_effect({
                let runs = runs.clone();
                move || {
                    runs.set(runs.get() + 1);
                    let cur = s.get();
                    if cur.is_empty() {
                        s.set("Shanghai".into());
                    }
                }
            });
            s.peek()
        }
    });
    assert_eq!(value, "Shanghai");
    assert_eq!(
        runs.get(),
        1,
        "writing a subscribed signal must not re-enter the same effect"
    );
}

#[test]
fn on_tracks_only_deps_and_passes_previous() {
    let a = Signal::new(1_i32);
    let b = Signal::new(100_i32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    on(move || a.get(), {
        let seen = seen.clone();
        move |cur, prev| {
            // Reading b inside body must not subscribe.
            let _ = b.peek();
            seen.borrow_mut().push((*cur, prev.copied()));
        }
    });
    assert_eq!(*seen.borrow(), vec![(1, None)]);
    b.set(200);
    assert_eq!(*seen.borrow(), vec![(1, None)]);
    a.set(2);
    assert_eq!(*seen.borrow(), vec![(1, None), (2, Some(1))]);
}

#[test]
fn create_selector_only_notifies_flipped_keys() {
    let selected = Signal::new("a".to_string());
    let selector = create_selector(move || selected.get());
    let a_runs = Rc::new(Cell::new(0_u32));
    let b_runs = Rc::new(Cell::new(0_u32));
    create_effect({
        let selector = selector.clone();
        let a_runs = a_runs.clone();
        move || {
            a_runs.set(a_runs.get() + 1);
            let _ = selector.is("a".to_string());
        }
    });
    create_effect({
        let selector = selector.clone();
        let b_runs = b_runs.clone();
        move || {
            b_runs.set(b_runs.get() + 1);
            let _ = selector.is("b".to_string());
        }
    });
    assert_eq!(a_runs.get(), 1);
    assert_eq!(b_runs.get(), 1);
    // a → c: a flips true→false; b stays false.
    selected.set("c".to_string());
    assert_eq!(a_runs.get(), 2);
    assert_eq!(b_runs.get(), 1);
    // c → b: b flips false→true; a stays false.
    selected.set("b".to_string());
    assert_eq!(a_runs.get(), 2);
    assert_eq!(b_runs.get(), 2);
}

#[test]
fn create_deferred_lags_one_flush() {
    let source = Signal::new(1_i32);
    let deferred = create_deferred(move || source.get());
    let seen = Rc::new(RefCell::new(Vec::new()));
    create_effect({
        let seen = seen.clone();
        move || {
            seen.borrow_mut().push(deferred.get());
        }
    });
    // Initial deferred catch-up already flushed after create_effect.
    assert_eq!(deferred.peek(), 1);
    assert_eq!(*seen.borrow(), vec![1]);

    source.set(2);
    // After full flush, deferred has caught up and effect observed 2.
    assert_eq!(deferred.peek(), 2);
    assert_eq!(*seen.borrow(), vec![1, 2]);
}

#[test]
fn effect_phases_computed_then_render_then_user() {
    let src = Signal::new(0_i32);
    let order = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    create_effect({
        let order = Rc::clone(&order);
        move || {
            order.borrow_mut().push("user");
            let _ = src.get();
        }
    });
    create_render_effect({
        let order = Rc::clone(&order);
        move || {
            order.borrow_mut().push("bind");
            let _ = src.get();
        }
    });
    create_computed({
        let order = Rc::clone(&order);
        move || {
            order.borrow_mut().push("computed");
            let _ = src.get();
        }
    });
    order.borrow_mut().clear();
    src.set(1);
    assert_eq!(
        *order.borrow(),
        vec!["computed", "bind", "user"],
        "I-12: Computed → bind_* → user create_effect"
    );
}

#[test]
fn reactivity_debug_snapshot_reports_edges_and_counts() {
    clear_effects();
    let (_root, src) = create_root(|| {
        let src = Signal::new(1_i32);
        let _memo = create_memo(move || src.get() + 1);
        create_effect({
            move || {
                let _ = src.get();
            }
        });
        src
    });
    let snap = reactivity_debug_snapshot();
    assert!(snap.live_owners >= 1);
    assert!(snap.live_effects >= 1);
    assert!(snap.live_memos >= 1);
    assert!(
        snap.signal_effect_edges
            .iter()
            .any(|edge| !edge.effect_ids.is_empty()),
        "at least one signal should have effect subscribers"
    );
    assert!(
        snap.signal_memo_edges
            .iter()
            .any(|edge| !edge.memo_ids.is_empty()),
        "at least one signal should have memo subscribers"
    );
    src.set(2);
    let snap_after = reactivity_debug_snapshot();
    assert_eq!(snap_after.pending_effects, 0);
    assert_eq!(snap_after.pending_memos, 0);
    let dump = dump_reactivity_graph();
    assert!(dump.contains("owners="), "{dump}");
    assert!(dump.contains("e_edges="), "{dump}");
}

#[test]
fn trigger_notifies_subscribers_unconditionally() {
    clear_effects();
    let trigger = create_trigger();
    let count = Rc::new(Cell::new(0_u32));
    let count_c = Rc::clone(&count);
    let _root = create_root(|| {
        create_effect(move || {
            trigger.track();
            count_c.set(count_c.get() + 1);
        });
    });
    assert_eq!(count.get(), 1);
    trigger.notify();
    assert_eq!(count.get(), 2);
    trigger.notify();
    assert_eq!(count.get(), 3);
}

#[test]
fn start_transition_defers_update() {
    clear_effects();
    let sig = Signal::new(0_i32);
    let seen = Rc::new(RefCell::new(Vec::<i32>::new()));
    let seen_c = Rc::clone(&seen);
    let _root = create_root(|| {
        create_effect(move || {
            seen_c.borrow_mut().push(sig.get());
        });
    });
    assert_eq!(*seen.borrow(), vec![0]);
    start_transition(move || {
        sig.set(42);
    });
    assert_eq!(*seen.borrow(), vec![0, 42]);
}
