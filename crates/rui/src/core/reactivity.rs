use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    static RUNTIME: RefCell<Rc<Runtime>> = RefCell::new(Runtime::new());
}

pub struct Runtime {
    pub current_effect: RefCell<Option<EffectId>>,
    pub signals: RefCell<HashMap<SignalId, Rc<RefCell<dyn Any>>>>,
    pub named_signals: RefCell<HashMap<String, SignalId>>,
    pub effects: RefCell<HashMap<EffectId, Rc<RefCell<dyn FnMut()>>>>,
    pub signal_subscribers: RefCell<HashMap<SignalId, HashSet<EffectId>>>,
    /// Last completed dependency set per effect (Solid-style dynamic tracking).
    pub effect_deps: RefCell<HashMap<EffectId, HashSet<SignalId>>>,
    /// Stack of dependency sets for nested effect runs (parent frame below child frame).
    dep_stack: RefCell<Vec<HashSet<SignalId>>>,
    /// Nesting depth for `batch`; >0 means `set` schedules effects instead of running immediately.
    batch_depth: RefCell<u32>,
    /// Effects queued by `set` while `batch_depth > 0` (order preserved, deduped).
    pending_effects: RefCell<VecDeque<EffectId>>,
    pending_effect_set: RefCell<HashSet<EffectId>>,
    /// `create_effect` called while another effect is running: run after that effect completes.
    deferred_effects: RefCell<Vec<EffectId>>,
}

impl Runtime {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            current_effect: RefCell::new(None),
            signals: RefCell::new(HashMap::new()),
            named_signals: RefCell::new(HashMap::new()),
            effects: RefCell::new(HashMap::new()),
            signal_subscribers: RefCell::new(HashMap::new()),
            effect_deps: RefCell::new(HashMap::new()),
            dep_stack: RefCell::new(Vec::new()),
            batch_depth: RefCell::new(0),
            pending_effects: RefCell::new(VecDeque::new()),
            pending_effect_set: RefCell::new(HashSet::new()),
            deferred_effects: RefCell::new(Vec::new()),
        })
    }

    pub fn create_signal<T: 'static>(&self, initial_value: T) -> Signal<T> {
        let mut signals = self.signals.borrow_mut();
        let id = SignalId(NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed));
        signals.insert(id, Rc::new(RefCell::new(initial_value)));

        Signal {
            id,
            marker: std::marker::PhantomData,
        }
    }

    pub fn use_signal<T: 'static>(&self, id_str: &str, init: impl FnOnce() -> T) -> Signal<T> {
        let exists = { self.named_signals.borrow().get(id_str).copied() };

        if let Some(signal_id) = exists {
            Signal {
                id: signal_id,
                marker: std::marker::PhantomData,
            }
        } else {
            let val = init();
            let mut signals = self.signals.borrow_mut();
            let signal_id = SignalId(NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed));
            signals.insert(signal_id, Rc::new(RefCell::new(val)));
            self.named_signals
                .borrow_mut()
                .insert(id_str.to_string(), signal_id);
            Signal {
                id: signal_id,
                marker: std::marker::PhantomData,
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SignalId(usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EffectId(usize);

static NEXT_SIGNAL_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_EFFECT_ID: AtomicUsize = AtomicUsize::new(1);

/// Run an effect: drop previous subscriptions, collect new deps from `get()` calls, then store them.
fn run_effect(rt: &Runtime, effect_id: EffectId, effect_fn: &Rc<RefCell<dyn FnMut()>>) {
    let old_deps = rt
        .effect_deps
        .borrow_mut()
        .remove(&effect_id)
        .unwrap_or_default();
    for sig_id in &old_deps {
        if let Some(set) = rt.signal_subscribers.borrow_mut().get_mut(sig_id) {
            set.remove(&effect_id);
        }
    }

    rt.dep_stack.borrow_mut().push(HashSet::new());

    let prev = *rt.current_effect.borrow();
    *rt.current_effect.borrow_mut() = Some(effect_id);

    (&mut *effect_fn.borrow_mut())();

    let new_deps = rt
        .dep_stack
        .borrow_mut()
        .pop()
        .expect("dep_stack: empty after effect run");
    rt.effect_deps
        .borrow_mut()
        .insert(effect_id, new_deps);

    *rt.current_effect.borrow_mut() = prev;
    flush_deferred_effects(rt);
}

fn flush_deferred_effects(rt: &Runtime) {
    loop {
        let ids: Vec<EffectId> = std::mem::take(&mut *rt.deferred_effects.borrow_mut());
        if ids.is_empty() {
            break;
        }
        for id in ids {
            let effect_fn = rt.effects.borrow().get(&id).cloned();
            if let Some(effect_fn) = effect_fn {
                run_effect(rt, id, &effect_fn);
            }
        }
    }
}

fn enqueue_pending_effect(rt: &Runtime, id: EffectId) {
    let mut set = rt.pending_effect_set.borrow_mut();
    if set.insert(id) {
        rt.pending_effects.borrow_mut().push_back(id);
    }
}

/// Drain queued effects until empty (runs may enqueue more).
fn flush_pending_effects(rt: &Runtime) {
    loop {
        let chunk: Vec<EffectId> = {
            let mut q = rt.pending_effects.borrow_mut();
            let mut set = rt.pending_effect_set.borrow_mut();
            if q.is_empty() {
                return;
            }
            let drained: Vec<EffectId> = q.drain(..).collect();
            set.clear();
            drained
        };
        for effect_id in chunk {
            let effect_fn = rt.effects.borrow().get(&effect_id).cloned();
            if let Some(effect_fn) = effect_fn {
                run_effect(rt, effect_id, &effect_fn);
            }
        }
    }
}

pub struct Signal<T> {
    id: SignalId,
    marker: std::marker::PhantomData<T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Signal<T> {}

impl<T: Clone + 'static> Signal<T> {
    pub fn new(value: T) -> Self {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            rt.create_signal(value)
        })
    }

    pub fn get(&self) -> T {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();

            if let Some(effect_id) = *rt.current_effect.borrow() {
                rt.signal_subscribers
                    .borrow_mut()
                    .entry(self.id)
                    .or_insert_with(HashSet::new)
                    .insert(effect_id);
                if let Some(frame) = rt.dep_stack.borrow_mut().last_mut() {
                    frame.insert(self.id);
                }
            }

            let any_val_rc = rt.signals.borrow().get(&self.id).cloned().unwrap();
            let any_val = any_val_rc.borrow();
            let typed_val = any_val.downcast_ref::<T>().unwrap();
            typed_val.clone()
        })
    }

    /// Read the signal value without subscribing the current effect.
    /// Use this for “animation/scroll state” reads that must not trigger full UI rebuilds.
    pub fn peek(&self) -> T {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt.signals.borrow().get(&self.id).cloned().unwrap();
            let any_val = any_val_rc.borrow();
            let typed_val = any_val.downcast_ref::<T>().unwrap();
            typed_val.clone()
        })
    }
}

impl<T: Clone + PartialEq + 'static> Signal<T> {
    /// Update value without scheduling dependent effects.
    /// Useful for high-frequency internal state (e.g. per-frame scroll offsets)
    /// where render reads via `peek()` and reactive rebuild would be pure overhead.
    pub fn set_silent(&self, new_value: T) {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt.signals.borrow().get(&self.id).cloned().unwrap();
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().unwrap();
            if *typed_val != new_value {
                *typed_val = new_value;
            }
        });
    }

    pub fn set(&self, new_value: T) {
        let effects_to_run = RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();

            let any_val_rc = rt.signals.borrow().get(&self.id).cloned().unwrap();
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().unwrap();
            if *typed_val == new_value {
                None
            } else {
                *typed_val = new_value;
                let subs = rt.signal_subscribers.borrow();
                Some(subs.get(&self.id).cloned().unwrap_or_default())
            }
        });

        let Some(effects_to_run) = effects_to_run else {
            return;
        };

        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            if *rt.batch_depth.borrow() > 0 {
                for effect_id in effects_to_run {
                    enqueue_pending_effect(&rt, effect_id);
                }
                return;
            }

            for effect_id in effects_to_run {
                let effect_fn = rt.effects.borrow().get(&effect_id).cloned();
                if let Some(effect_fn) = effect_fn {
                    run_effect(&rt, effect_id, &effect_fn);
                }
            }
        });
    }
}

pub fn create_effect<F>(f: F)
where
    F: FnMut() + 'static,
{
    let id = EffectId(NEXT_EFFECT_ID.fetch_add(1, Ordering::Relaxed));
    let effect_rc: Rc<RefCell<dyn FnMut()>> = Rc::new(RefCell::new(f));

    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        rt.effects.borrow_mut().insert(id, Rc::clone(&effect_rc));
        if rt.current_effect.borrow().is_some() {
            rt.deferred_effects.borrow_mut().push(id);
        } else {
            run_effect(&rt, id, &effect_rc);
        }
    });
}

/// Run `f` without tracking the current effect: `get()` inside `f` will not subscribe.
pub fn untrack<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    RUNTIME.with(|rt_rc| {
        let prev = {
            let rt = rt_rc.borrow();
            let prev = *rt.current_effect.borrow();
            *rt.current_effect.borrow_mut() = None;
            prev
        };
        let out = f();
        *rt_rc.borrow().current_effect.borrow_mut() = prev;
        out
    })
}

struct BatchGuard;

impl Drop for BatchGuard {
    fn drop(&mut self) {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let mut depth = rt.batch_depth.borrow_mut();
            debug_assert!(*depth > 0, "batch_depth underflow");
            *depth -= 1;
            if *depth == 0 {
                flush_pending_effects(&rt);
            }
        });
    }
}

/// Coalesce `set` notifications: while `f` runs, dependent effects are queued and run once when
/// the outermost `batch` ends (nested `batch` calls only flush when the stack unwinds to zero).
pub fn batch<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    RUNTIME.with(|rt_rc| {
        *rt_rc.borrow().batch_depth.borrow_mut() += 1;
    });
    let _guard = BatchGuard;
    f()
}

pub fn create_signal<T: 'static>(initial_value: T) -> Signal<T> {
    RUNTIME.with(|rt_rc| rt_rc.borrow().create_signal(initial_value))
}

pub fn use_signal<T: 'static>(id_str: &str, init: impl FnOnce() -> T) -> Signal<T> {
    RUNTIME.with(|rt_rc| rt_rc.borrow().use_signal(id_str, init))
}

/// True when code is currently executing inside a tracked effect context.
pub fn is_tracking() -> bool {
    RUNTIME.with(|rt_rc| rt_rc.borrow().current_effect.borrow().is_some())
}

/// Cached derived value: an inner effect re-runs `compute` when its dependencies change and writes
/// into a backing [`Signal`]. Callers use [`Memo::get`] to subscribe to the memo output only.
#[derive(Clone, Copy)]
pub struct Memo<T> {
    signal: Signal<T>,
}

impl<T: Clone + PartialEq + 'static> Memo<T> {
    #[inline]
    pub fn get(&self) -> T {
        self.signal.get()
    }

    #[inline]
    pub fn peek(&self) -> T {
        self.signal.peek()
    }
}

/// Solid-style memo: tracks dependencies inside `compute`, caches the result, and notifies memo
/// readers when the cached value changes.
pub fn create_memo<T: Clone + PartialEq + 'static, F>(mut compute: F) -> Memo<T>
where
    F: FnMut() -> T + 'static,
{
    let initial = untrack(|| compute());
    let signal = create_signal(initial);
    create_effect({
        move || {
            let v = compute();
            signal.set(v);
        }
    });
    Memo { signal }
}

pub use crate::style::{tick_animations, use_spring, Spring, SpringState};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
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

    /// Parent registers a child `create_effect` on first run; `dep_stack` must separate their deps.
    /// Do not re-run the parent here — each parent run would register another child effect.
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
}
