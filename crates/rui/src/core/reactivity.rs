//! Solid-style reactive core with an Owner / Scope tree.
//!
//! ## Ownership model
//!
//! Every [`create_signal`], [`create_effect`], [`create_memo`], and [`on_cleanup`] attaches to the
//! **current owner**. When that owner is disposed (explicitly, or because its parent effect
//! re-runs), children owners, effects, owned signals, and cleanups cascade away.
//!
//! Effects each own a scope: before an effect body re-runs, its previous child scopes are disposed
//! so nested `create_effect` calls no longer leak across runs.
//!
//! ## Named vs owned signals
//!
//! - [`create_signal`] — owned by the current scope; disposed with the owner.
//! - [`use_signal`] — **global** keyed registry (intentionally survives rebuilds for Input focus,
//!   scroll offsets, etc.).
//! - [`use_scoped_signal`] — keyed relative to the current owner path; same local id under one
//!   owner reuses the slot (type mismatch panics in debug).

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    static RUNTIME: RefCell<Rc<Runtime>> = RefCell::new(Runtime::new());
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SignalId(usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EffectId(usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct OwnerId(usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MemoId(usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ContextId(usize);

static NEXT_SIGNAL_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_EFFECT_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_OWNER_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_MEMO_ID: AtomicUsize = AtomicUsize::new(1);
static NEXT_CONTEXT_ID: AtomicUsize = AtomicUsize::new(1);

struct OwnerNode {
    parent: Option<OwnerId>,
    /// Stable path used for scoped signal keys (`root3/7`).
    path: String,
    children: Vec<OwnerId>,
    effects: Vec<EffectId>,
    signals: Vec<SignalId>,
    memos: Vec<MemoId>,
    cleanups: Vec<Box<dyn FnOnce()>>,
    contexts: HashMap<ContextId, Rc<dyn Any>>,
    /// Local-id → (SignalId, TypeId) for [`use_scoped_signal`].
    scoped_signals: HashMap<String, (SignalId, TypeId)>,
}

struct EffectState {
    owner: OwnerId,
    depth: u32,
    f: Rc<RefCell<dyn FnMut()>>,
}

struct MemoState {
    owner: OwnerId,
    depth: u32,
    dirty: Cell<bool>,
    /// Backing signal that memo readers subscribe to.
    signal: SignalId,
    /// Recompute: reads sources (tracked), writes backing signal via `set` without notifying
    /// (silent) then notifies if changed — handled inside `run_memo`.
    compute: Rc<RefCell<dyn FnMut() -> bool>>,
}

pub struct Runtime {
    pub current_effect: RefCell<Option<EffectId>>,
    current_owner: RefCell<Option<OwnerId>>,
    /// When running a memo compute, which memo is collecting deps.
    current_memo: RefCell<Option<MemoId>>,
    root_owner: RefCell<Option<OwnerId>>,
    owners: RefCell<HashMap<OwnerId, OwnerNode>>,
    pub signals: RefCell<HashMap<SignalId, Rc<RefCell<dyn Any>>>>,
    pub named_signals: RefCell<HashMap<String, SignalId>>,
    named_signal_types: RefCell<HashMap<String, TypeId>>,
    effects: RefCell<HashMap<EffectId, EffectState>>,
    memos: RefCell<HashMap<MemoId, MemoState>>,
    /// signal → subscribed effects
    pub signal_subscribers: RefCell<HashMap<SignalId, HashSet<EffectId>>>,
    /// signal → memos that read it
    signal_memo_subscribers: RefCell<HashMap<SignalId, HashSet<MemoId>>>,
    /// Last completed dependency set per effect.
    pub effect_deps: RefCell<HashMap<EffectId, HashSet<SignalId>>>,
    memo_deps: RefCell<HashMap<MemoId, HashSet<SignalId>>>,
    /// Nested dep collection frames (parent below child).
    dep_stack: RefCell<Vec<HashSet<SignalId>>>,
    batch_depth: RefCell<u32>,
    pending_effects: RefCell<VecDeque<EffectId>>,
    pending_effect_set: RefCell<HashSet<EffectId>>,
    pending_memos: RefCell<VecDeque<MemoId>>,
    pending_memo_set: RefCell<HashSet<MemoId>>,
    deferred_effects: RefCell<Vec<EffectId>>,
}

impl Runtime {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            current_effect: RefCell::new(None),
            current_owner: RefCell::new(None),
            current_memo: RefCell::new(None),
            root_owner: RefCell::new(None),
            owners: RefCell::new(HashMap::new()),
            signals: RefCell::new(HashMap::new()),
            named_signals: RefCell::new(HashMap::new()),
            named_signal_types: RefCell::new(HashMap::new()),
            effects: RefCell::new(HashMap::new()),
            memos: RefCell::new(HashMap::new()),
            signal_subscribers: RefCell::new(HashMap::new()),
            signal_memo_subscribers: RefCell::new(HashMap::new()),
            effect_deps: RefCell::new(HashMap::new()),
            memo_deps: RefCell::new(HashMap::new()),
            dep_stack: RefCell::new(Vec::new()),
            batch_depth: RefCell::new(0),
            pending_effects: RefCell::new(VecDeque::new()),
            pending_effect_set: RefCell::new(HashSet::new()),
            pending_memos: RefCell::new(VecDeque::new()),
            pending_memo_set: RefCell::new(HashSet::new()),
            deferred_effects: RefCell::new(Vec::new()),
        })
    }

    pub fn create_signal<T: 'static>(&self, initial_value: T) -> Signal<T> {
        let owner = ensure_owner(self);
        let id = SignalId(NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed));
        self.signals
            .borrow_mut()
            .insert(id, Rc::new(RefCell::new(initial_value)));
        if let Some(node) = self.owners.borrow_mut().get_mut(&owner) {
            node.signals.push(id);
        }
        Signal {
            id,
            marker: std::marker::PhantomData,
        }
    }

    pub fn use_signal<T: 'static>(&self, id_str: &str, init: impl FnOnce() -> T) -> Signal<T> {
        let type_id = TypeId::of::<T>();
        let exists = { self.named_signals.borrow().get(id_str).copied() };

        if let Some(signal_id) = exists {
            let prev = self.named_signal_types.borrow().get(id_str).copied();
            debug_assert!(
                prev == Some(type_id),
                "use_signal key collision with different type: `{id_str}`"
            );
            return Signal {
                id: signal_id,
                marker: std::marker::PhantomData,
            };
        }

        let val = init();
        let signal_id = SignalId(NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed));
        self.signals
            .borrow_mut()
            .insert(signal_id, Rc::new(RefCell::new(val)));
        self.named_signals
            .borrow_mut()
            .insert(id_str.to_string(), signal_id);
        self.named_signal_types
            .borrow_mut()
            .insert(id_str.to_string(), type_id);
        Signal {
            id: signal_id,
            marker: std::marker::PhantomData,
        }
    }
}

fn ensure_owner(rt: &Runtime) -> OwnerId {
    if let Some(id) = *rt.current_owner.borrow() {
        return id;
    }
    if let Some(root) = *rt.root_owner.borrow() {
        *rt.current_owner.borrow_mut() = Some(root);
        return root;
    }
    let id = OwnerId(NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed));
    rt.owners.borrow_mut().insert(
        id,
        OwnerNode {
            parent: None,
            path: format!("root{}", id.0),
            children: Vec::new(),
            effects: Vec::new(),
            signals: Vec::new(),
            memos: Vec::new(),
            cleanups: Vec::new(),
            contexts: HashMap::new(),
            scoped_signals: HashMap::new(),
        },
    );
    *rt.root_owner.borrow_mut() = Some(id);
    *rt.current_owner.borrow_mut() = Some(id);
    id
}

fn create_child_owner(rt: &Runtime, parent: OwnerId) -> OwnerId {
    let id = OwnerId(NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed));
    let path = {
        let owners = rt.owners.borrow();
        let parent_path = owners
            .get(&parent)
            .map(|n| n.path.as_str())
            .unwrap_or("root");
        format!("{parent_path}/{}", id.0)
    };
    rt.owners.borrow_mut().insert(
        id,
        OwnerNode {
            parent: Some(parent),
            path,
            children: Vec::new(),
            effects: Vec::new(),
            signals: Vec::new(),
            memos: Vec::new(),
            cleanups: Vec::new(),
            contexts: HashMap::new(),
            scoped_signals: HashMap::new(),
        },
    );
    if let Some(parent_node) = rt.owners.borrow_mut().get_mut(&parent) {
        parent_node.children.push(id);
    }
    id
}

fn owner_depth(rt: &Runtime, id: OwnerId) -> u32 {
    let mut depth = 0u32;
    let mut cur = Some(id);
    while let Some(oid) = cur {
        depth = depth.saturating_add(1);
        cur = rt.owners.borrow().get(&oid).and_then(|n| n.parent);
        if depth > 10_000 {
            break;
        }
    }
    depth
}

fn dispose_owner(rt: &Runtime, id: OwnerId) {
    let Some(mut node) = rt.owners.borrow_mut().remove(&id) else {
        return;
    };

    if let Some(parent) = node.parent {
        if let Some(parent_node) = rt.owners.borrow_mut().get_mut(&parent) {
            parent_node.children.retain(|c| *c != id);
        }
    }

    while let Some(cleanup) = node.cleanups.pop() {
        cleanup();
    }

    for child in std::mem::take(&mut node.children) {
        dispose_owner(rt, child);
    }

    for effect_id in std::mem::take(&mut node.effects) {
        dispose_effect_inner(rt, effect_id, false);
    }

    for memo_id in std::mem::take(&mut node.memos) {
        dispose_memo_inner(rt, memo_id);
    }

    for signal_id in std::mem::take(&mut node.signals) {
        rt.signals.borrow_mut().remove(&signal_id);
        rt.signal_subscribers.borrow_mut().remove(&signal_id);
        rt.signal_memo_subscribers.borrow_mut().remove(&signal_id);
    }

    if *rt.current_owner.borrow() == Some(id) {
        *rt.current_owner.borrow_mut() = node.parent;
    }
    if *rt.root_owner.borrow() == Some(id) {
        *rt.root_owner.borrow_mut() = None;
    }
}

fn dispose_owner_children(rt: &Runtime, id: OwnerId) {
    let children = rt
        .owners
        .borrow()
        .get(&id)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    for child in children {
        dispose_owner(rt, child);
    }
}

fn dispose_effect_inner(rt: &Runtime, id: EffectId, also_dispose_owner: bool) {
    let Some(state) = rt.effects.borrow_mut().remove(&id) else {
        return;
    };
    let deps = rt.effect_deps.borrow_mut().remove(&id).unwrap_or_default();
    for signal_id in deps {
        if let Some(subscribers) = rt.signal_subscribers.borrow_mut().get_mut(&signal_id) {
            subscribers.remove(&id);
        }
    }
    rt.pending_effects
        .borrow_mut()
        .retain(|queued| *queued != id);
    rt.pending_effect_set.borrow_mut().remove(&id);
    rt.deferred_effects
        .borrow_mut()
        .retain(|queued| *queued != id);

    if also_dispose_owner {
        dispose_owner(rt, state.owner);
    }
}

fn dispose_memo_inner(rt: &Runtime, id: MemoId) {
    let Some(state) = rt.memos.borrow_mut().remove(&id) else {
        return;
    };
    let deps = rt.memo_deps.borrow_mut().remove(&id).unwrap_or_default();
    for signal_id in deps {
        if let Some(set) = rt.signal_memo_subscribers.borrow_mut().get_mut(&signal_id) {
            set.remove(&id);
        }
    }
    rt.pending_memos.borrow_mut().retain(|queued| *queued != id);
    rt.pending_memo_set.borrow_mut().remove(&id);
    rt.signals.borrow_mut().remove(&state.signal);
    rt.signal_subscribers.borrow_mut().remove(&state.signal);
}

fn track_signal_read(rt: &Runtime, signal_id: SignalId) {
    if let Some(effect_id) = *rt.current_effect.borrow() {
        rt.signal_subscribers
            .borrow_mut()
            .entry(signal_id)
            .or_default()
            .insert(effect_id);
        if let Some(frame) = rt.dep_stack.borrow_mut().last_mut() {
            frame.insert(signal_id);
        }
    } else if let Some(memo_id) = *rt.current_memo.borrow() {
        rt.signal_memo_subscribers
            .borrow_mut()
            .entry(signal_id)
            .or_default()
            .insert(memo_id);
        if let Some(frame) = rt.dep_stack.borrow_mut().last_mut() {
            frame.insert(signal_id);
        }
    }
}

fn run_effect(rt: &Runtime, effect_id: EffectId) {
    let Some(state) = rt
        .effects
        .borrow()
        .get(&effect_id)
        .map(|s| (s.owner, Rc::clone(&s.f)))
    else {
        return;
    };
    let (owner, effect_fn) = state;

    // Solid: dispose previous child scopes before re-running.
    dispose_owner_children(rt, owner);

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

    let prev_effect = *rt.current_effect.borrow();
    let prev_owner = *rt.current_owner.borrow();
    *rt.current_effect.borrow_mut() = Some(effect_id);
    *rt.current_owner.borrow_mut() = Some(owner);

    (*effect_fn.borrow_mut())();

    let new_deps = rt
        .dep_stack
        .borrow_mut()
        .pop()
        .expect("dep_stack: empty after effect run");
    rt.effect_deps.borrow_mut().insert(effect_id, new_deps);

    *rt.current_effect.borrow_mut() = prev_effect;
    *rt.current_owner.borrow_mut() = prev_owner;
    flush_deferred_effects(rt);
}

fn flush_deferred_effects(rt: &Runtime) {
    loop {
        let ids: Vec<EffectId> = std::mem::take(&mut *rt.deferred_effects.borrow_mut());
        if ids.is_empty() {
            break;
        }
        for id in ids {
            run_effect(rt, id);
        }
    }
}

fn enqueue_pending_effect(rt: &Runtime, id: EffectId) {
    let mut set = rt.pending_effect_set.borrow_mut();
    if set.insert(id) {
        rt.pending_effects.borrow_mut().push_back(id);
    }
}

fn enqueue_pending_memo(rt: &Runtime, id: MemoId) {
    let mut set = rt.pending_memo_set.borrow_mut();
    if set.insert(id) {
        rt.pending_memos.borrow_mut().push_back(id);
    }
}

fn mark_memos_dirty_from_signal(rt: &Runtime, signal_id: SignalId) {
    let memos = rt
        .signal_memo_subscribers
        .borrow()
        .get(&signal_id)
        .cloned()
        .unwrap_or_default();
    for memo_id in memos {
        if let Some(memo) = rt.memos.borrow().get(&memo_id) {
            memo.dirty.set(true);
            enqueue_pending_memo(rt, memo_id);
        }
    }
}

/// Recompute a dirty memo. Returns whether the backing value changed.
fn run_memo(rt: &Runtime, memo_id: MemoId) -> bool {
    let Some((owner, compute, signal_id, was_dirty)) = rt
        .memos
        .borrow()
        .get(&memo_id)
        .map(|m| (m.owner, Rc::clone(&m.compute), m.signal, m.dirty.get()))
    else {
        return false;
    };
    if !was_dirty {
        return false;
    }

    let old_deps = rt
        .memo_deps
        .borrow_mut()
        .remove(&memo_id)
        .unwrap_or_default();
    for sig_id in &old_deps {
        if let Some(set) = rt.signal_memo_subscribers.borrow_mut().get_mut(sig_id) {
            set.remove(&memo_id);
        }
    }

    rt.dep_stack.borrow_mut().push(HashSet::new());
    let prev_memo = *rt.current_memo.borrow();
    let prev_owner = *rt.current_owner.borrow();
    let prev_effect = *rt.current_effect.borrow();
    *rt.current_memo.borrow_mut() = Some(memo_id);
    *rt.current_owner.borrow_mut() = Some(owner);
    // Memo compute should not attach to an outer effect.
    *rt.current_effect.borrow_mut() = None;

    let changed = (*compute.borrow_mut())();

    let new_deps = rt.dep_stack.borrow_mut().pop().unwrap_or_default();
    rt.memo_deps.borrow_mut().insert(memo_id, new_deps);

    if let Some(memo) = rt.memos.borrow().get(&memo_id) {
        memo.dirty.set(false);
    }

    *rt.current_memo.borrow_mut() = prev_memo;
    *rt.current_owner.borrow_mut() = prev_owner;
    *rt.current_effect.borrow_mut() = prev_effect;

    let _ = signal_id;
    changed
}

fn ensure_memo_fresh(rt: &Runtime, memo_id: MemoId) {
    // Pull-based: if this memo is dirty, refresh it (and transitively its dirty memo deps via get).
    let dirty = rt
        .memos
        .borrow()
        .get(&memo_id)
        .map(|m| m.dirty.get())
        .unwrap_or(false);
    if dirty {
        let _ = run_memo(rt, memo_id);
    }
}

/// Drain pending memos (by depth ascending) then effects (by depth ascending).
fn flush_pending(rt: &Runtime) {
    loop {
        let memo_chunk: Vec<MemoId> = {
            let mut q = rt.pending_memos.borrow_mut();
            let mut set = rt.pending_memo_set.borrow_mut();
            if q.is_empty() {
                Vec::new()
            } else {
                let drained: Vec<MemoId> = q.drain(..).collect();
                set.clear();
                drained
            }
        };

        if !memo_chunk.is_empty() {
            let mut ordered = memo_chunk;
            ordered.sort_by_key(|id| {
                rt.memos
                    .borrow()
                    .get(id)
                    .map(|m| m.depth)
                    .unwrap_or(u32::MAX)
            });
            for memo_id in ordered {
                let changed = run_memo(rt, memo_id);
                if changed {
                    // Notify effect subscribers of the memo's backing signal.
                    let signal_id = rt.memos.borrow().get(&memo_id).map(|m| m.signal);
                    if let Some(signal_id) = signal_id {
                        let effects = rt
                            .signal_subscribers
                            .borrow()
                            .get(&signal_id)
                            .cloned()
                            .unwrap_or_default();
                        for effect_id in effects {
                            enqueue_pending_effect(rt, effect_id);
                        }
                        // Downstream memos that read this memo's signal.
                        mark_memos_dirty_from_signal(rt, signal_id);
                    }
                }
            }
            // More memos may have been enqueued; continue loop.
            if !rt.pending_memos.borrow().is_empty() {
                continue;
            }
        }

        let effect_chunk: Vec<EffectId> = {
            let mut q = rt.pending_effects.borrow_mut();
            let mut set = rt.pending_effect_set.borrow_mut();
            if q.is_empty() {
                return;
            }
            let drained: Vec<EffectId> = q.drain(..).collect();
            set.clear();
            drained
        };

        let mut ordered = effect_chunk;
        ordered.sort_by_key(|id| {
            rt.effects
                .borrow()
                .get(id)
                .map(|e| e.depth)
                .unwrap_or(u32::MAX)
        });
        for effect_id in ordered {
            run_effect(rt, effect_id);
        }
        // Effects may have dirtied more memos / effects.
        if rt.pending_memos.borrow().is_empty() && rt.pending_effects.borrow().is_empty() {
            return;
        }
    }
}

fn notify_signal_subscribers(rt: &Runtime, signal_id: SignalId) {
    mark_memos_dirty_from_signal(rt, signal_id);

    let effects = rt
        .signal_subscribers
        .borrow()
        .get(&signal_id)
        .cloned()
        .unwrap_or_default();

    if *rt.batch_depth.borrow() > 0 {
        for effect_id in effects {
            enqueue_pending_effect(rt, effect_id);
        }
        return;
    }

    // Outside batch: still go through pending queue so memos refresh in depth order first.
    for effect_id in effects {
        enqueue_pending_effect(rt, effect_id);
    }
    flush_pending(rt);
}

// ---------------------------------------------------------------------------
// Signal
// ---------------------------------------------------------------------------

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
            track_signal_read(&rt, self.id);
            let any_val_rc = rt
                .signals
                .borrow()
                .get(&self.id)
                .cloned()
                .expect("Signal::get on disposed signal");
            let any_val = any_val_rc.borrow();
            any_val
                .downcast_ref::<T>()
                .expect("Signal type mismatch")
                .clone()
        })
    }

    /// Read without subscribing the current effect / memo.
    pub fn peek(&self) -> T {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt
                .signals
                .borrow()
                .get(&self.id)
                .cloned()
                .expect("Signal::peek on disposed signal");
            let any_val = any_val_rc.borrow();
            any_val
                .downcast_ref::<T>()
                .expect("Signal type mismatch")
                .clone()
        })
    }

    /// Borrow-style read without cloning. Still tracks the current effect / memo.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            track_signal_read(&rt, self.id);
            let any_val_rc = rt
                .signals
                .borrow()
                .get(&self.id)
                .cloned()
                .expect("Signal::with on disposed signal");
            let any_val = any_val_rc.borrow();
            let typed = any_val.downcast_ref::<T>().expect("Signal type mismatch");
            f(typed)
        })
    }
}

impl<T: Clone + PartialEq + 'static> Signal<T> {
    /// Update value without scheduling dependent effects / memos.
    pub fn set_silent(&self, new_value: T) {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt
                .signals
                .borrow()
                .get(&self.id)
                .cloned()
                .expect("Signal::set_silent on disposed signal");
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().expect("Signal type mismatch");
            if *typed_val != new_value {
                *typed_val = new_value;
            }
        });
    }

    pub fn set(&self, new_value: T) {
        let changed = RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt
                .signals
                .borrow()
                .get(&self.id)
                .cloned()
                .expect("Signal::set on disposed signal");
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().expect("Signal type mismatch");
            if *typed_val == new_value {
                false
            } else {
                *typed_val = new_value;
                true
            }
        });
        if !changed {
            return;
        }
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            notify_signal_subscribers(&rt, self.id);
        });
    }

    /// In-place update; notifies subscribers only when the new value differs.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let changed = RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt
                .signals
                .borrow()
                .get(&self.id)
                .cloned()
                .expect("Signal::update on disposed signal");
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().expect("Signal type mismatch");
            let before = typed_val.clone();
            f(typed_val);
            *typed_val != before
        });
        if !changed {
            return;
        }
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            notify_signal_subscribers(&rt, self.id);
        });
    }
}

// ---------------------------------------------------------------------------
// Effects / owners / cleanup
// ---------------------------------------------------------------------------

pub fn create_effect<F>(f: F) -> EffectId
where
    F: FnMut() + 'static,
{
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let parent = ensure_owner(&rt);
        let owner = create_child_owner(&rt, parent);
        let depth = owner_depth(&rt, owner);
        let id = EffectId(NEXT_EFFECT_ID.fetch_add(1, Ordering::Relaxed));
        let effect_rc: Rc<RefCell<dyn FnMut()>> = Rc::new(RefCell::new(f));

        rt.effects.borrow_mut().insert(
            id,
            EffectState {
                owner,
                depth,
                f: Rc::clone(&effect_rc),
            },
        );
        if let Some(node) = rt.owners.borrow_mut().get_mut(&owner) {
            node.effects.push(id);
        }
        // Also record on parent? The effect's own owner holds it; parent holds the owner as child.

        if rt.current_effect.borrow().is_some() {
            rt.deferred_effects.borrow_mut().push(id);
        } else {
            run_effect(&rt, id);
        }
        id
    })
}

/// Dispose one effect and its owner scope (cascades to nested effects / signals / cleanups).
pub fn dispose_effect(id: EffectId) {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner = rt.effects.borrow().get(&id).map(|s| s.owner);
        dispose_effect_inner(&rt, id, false);
        if let Some(owner) = owner {
            dispose_owner(&rt, owner);
        }
    });
}

/// Drop all effects, owners, and owned signals. Named (`use_signal`) bindings are kept so
/// Input/scroll state can survive a soft restart; call [`clear_named_signals`] if a full wipe is needed.
pub fn clear_effects() {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        if let Some(root) = *rt.root_owner.borrow() {
            dispose_owner(&rt, root);
        }
        // Orphan owners (should be none).
        let remaining: Vec<OwnerId> = rt.owners.borrow().keys().copied().collect();
        for id in remaining {
            dispose_owner(&rt, id);
        }
        rt.effects.borrow_mut().clear();
        rt.effect_deps.borrow_mut().clear();
        rt.memos.borrow_mut().clear();
        rt.memo_deps.borrow_mut().clear();
        rt.signal_subscribers.borrow_mut().clear();
        rt.signal_memo_subscribers.borrow_mut().clear();
        rt.pending_effects.borrow_mut().clear();
        rt.pending_effect_set.borrow_mut().clear();
        rt.pending_memos.borrow_mut().clear();
        rt.pending_memo_set.borrow_mut().clear();
        rt.deferred_effects.borrow_mut().clear();
        *rt.current_effect.borrow_mut() = None;
        *rt.current_memo.borrow_mut() = None;
        *rt.current_owner.borrow_mut() = None;
        *rt.root_owner.borrow_mut() = None;
        // Owned signals were removed with owners; drop any stragglers that aren't named.
        let named: HashSet<SignalId> = rt.named_signals.borrow().values().copied().collect();
        rt.signals.borrow_mut().retain(|id, _| named.contains(id));
    });
}

/// Remove a named signal binding and drop its value storage (e.g. Navigator Host 卸载).
pub fn remove_named_signal(id_str: &str) {
    let _ = RUNTIME.try_with(|rt_rc| {
        let rt = rt_rc.borrow();
        let Some(signal_id) = rt.named_signals.borrow_mut().remove(id_str) else {
            return;
        };
        rt.named_signal_types.borrow_mut().remove(id_str);
        rt.signals.borrow_mut().remove(&signal_id);
        rt.signal_subscribers.borrow_mut().remove(&signal_id);
        rt.signal_memo_subscribers.borrow_mut().remove(&signal_id);
    });
}

/// Register a cleanup that runs when the current owner is disposed (LIFO among siblings).
pub fn on_cleanup<F>(f: F)
where
    F: FnOnce() + 'static,
{
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner = ensure_owner(&rt);
        let mut owners = rt.owners.borrow_mut();
        if let Some(node) = owners.get_mut(&owner) {
            node.cleanups.push(Box::new(f));
        }
    });
}

/// Handle returned by [`create_root`]; dropping or calling [`OwnerHandle::dispose`] tears down the
/// root scope and everything created under it.
pub struct OwnerHandle {
    id: OwnerId,
    disposed: Cell<bool>,
}

impl OwnerHandle {
    pub fn dispose(self) {
        self.dispose_inner();
    }

    pub fn id(&self) -> OwnerId {
        self.id
    }

    fn dispose_inner(&self) {
        if self.disposed.replace(true) {
            return;
        }
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            dispose_owner(&rt, self.id);
        });
    }
}

impl Drop for OwnerHandle {
    fn drop(&mut self) {
        self.dispose_inner();
    }
}

/// Create a detached owner, run `f` under it, and return a handle that disposes the whole tree.
pub fn create_root<F, R>(f: F) -> (OwnerHandle, R)
where
    F: FnOnce() -> R,
{
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let id = OwnerId(NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed));
        rt.owners.borrow_mut().insert(
            id,
            OwnerNode {
                parent: None,
                path: format!("root{}", id.0),
                children: Vec::new(),
                effects: Vec::new(),
                signals: Vec::new(),
                memos: Vec::new(),
                cleanups: Vec::new(),
                contexts: HashMap::new(),
                scoped_signals: HashMap::new(),
            },
        );
        let prev_owner = *rt.current_owner.borrow();
        let prev_root = *rt.root_owner.borrow();
        // Nested create_root: don't overwrite global root unless none exists.
        if prev_root.is_none() {
            *rt.root_owner.borrow_mut() = Some(id);
        }
        *rt.current_owner.borrow_mut() = Some(id);
        let out = f();
        *rt.current_owner.borrow_mut() = prev_owner;
        if prev_root.is_none() {
            // Keep this as root_owner until disposed.
        } else {
            *rt.root_owner.borrow_mut() = prev_root;
        }
        (
            OwnerHandle {
                id,
                disposed: Cell::new(false),
            },
            out,
        )
    })
}

/// Current owner id, if any.
pub fn current_owner() -> Option<OwnerId> {
    RUNTIME.with(|rt_rc| *rt_rc.borrow().current_owner.borrow())
}

/// Owner path string used for scoped keys (e.g. `root1/4/9`).
pub fn owner_path_str(id: OwnerId) -> String {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owners = rt.owners.borrow();
        owners
            .get(&id)
            .map(|n| n.path.clone())
            .unwrap_or_else(|| format!("owner{}", id.0))
    })
}

/// Scoped named signal: key is `{owner_path}::{local_id}`. Re-entry with the same local id under
/// the same owner returns the existing signal; a type mismatch panics in debug builds.
pub fn use_scoped_signal<T: 'static>(local_id: &str, init: impl FnOnce() -> T) -> Signal<T> {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner = ensure_owner(&rt);
        let type_id = TypeId::of::<T>();

        {
            let owners = rt.owners.borrow();
            if let Some(node) = owners.get(&owner) {
                if let Some(&(signal_id, prev_ty)) = node.scoped_signals.get(local_id) {
                    debug_assert!(
                        prev_ty == type_id,
                        "use_scoped_signal key collision with different type: `{local_id}` under {}",
                        node.path
                    );
                    return Signal {
                        id: signal_id,
                        marker: std::marker::PhantomData,
                    };
                }
            }
        }

        let signal = rt.create_signal(init());
        {
            let mut owners = rt.owners.borrow_mut();
            if let Some(node) = owners.get_mut(&owner) {
                node.scoped_signals
                    .insert(local_id.to_string(), (signal.id, type_id));
            }
        }
        signal
    })
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Type-safe context key. Create once with [`create_context`], provide with [`provide_context`],
/// read with [`use_context`] (walks the owner parent chain).
pub struct Context<T> {
    id: ContextId,
    marker: std::marker::PhantomData<T>,
}

impl<T> Clone for Context<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Context<T> {}

pub fn create_context<T: 'static>() -> Context<T> {
    Context {
        id: ContextId(NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed)),
        marker: std::marker::PhantomData,
    }
}

pub fn provide_context<T: 'static>(ctx: Context<T>, value: T) {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner = ensure_owner(&rt);
        let mut owners = rt.owners.borrow_mut();
        if let Some(node) = owners.get_mut(&owner) {
            node.contexts.insert(ctx.id, Rc::new(value));
        }
    });
}

pub fn use_context<T: Clone + 'static>(ctx: Context<T>) -> Option<T> {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let mut cur = *rt.current_owner.borrow();
        while let Some(oid) = cur {
            let (parent, found) = {
                let owners = rt.owners.borrow();
                let Some(node) = owners.get(&oid) else {
                    break;
                };
                let found = node
                    .contexts
                    .get(&ctx.id)
                    .and_then(|rc| rc.downcast_ref::<T>().cloned());
                (node.parent, found)
            };
            if found.is_some() {
                return found;
            }
            cur = parent;
        }
        None
    })
}

// ---------------------------------------------------------------------------
// Tracking helpers
// ---------------------------------------------------------------------------

pub fn untrack<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    RUNTIME.with(|rt_rc| {
        let prev_effect = {
            let rt = rt_rc.borrow();
            let prev = *rt.current_effect.borrow();
            *rt.current_effect.borrow_mut() = None;
            prev
        };
        let prev_memo = {
            let rt = rt_rc.borrow();
            let prev = *rt.current_memo.borrow();
            *rt.current_memo.borrow_mut() = None;
            prev
        };
        let out = f();
        *rt_rc.borrow().current_effect.borrow_mut() = prev_effect;
        *rt_rc.borrow().current_memo.borrow_mut() = prev_memo;
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
                flush_pending(&rt);
            }
        });
    }
}

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

/// Run `f` while tracking stays on (`is_tracking()` true) but collected `get()` deps are discarded.
pub fn isolate_reads<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let effect_id = *rt.current_effect.borrow();
        rt.dep_stack.borrow_mut().push(HashSet::new());
        let out = f();
        let discarded = rt.dep_stack.borrow_mut().pop().unwrap_or_default();
        if let Some(effect_id) = effect_id {
            for sig_id in discarded {
                if let Some(set) = rt.signal_subscribers.borrow_mut().get_mut(&sig_id) {
                    set.remove(&effect_id);
                }
            }
        }
        out
    })
}

pub fn is_tracking() -> bool {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        rt.current_effect.borrow().is_some() || rt.current_memo.borrow().is_some()
    })
}

// ---------------------------------------------------------------------------
// Memo (lazy + dirty flag; flushed in depth order before effects)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Memo<T> {
    id: MemoId,
    signal: Signal<T>,
}

impl<T: Clone + PartialEq + 'static> Memo<T> {
    pub fn get(&self) -> T {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            ensure_memo_fresh(&rt, self.id);
        });
        self.signal.get()
    }

    pub fn peek(&self) -> T {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            ensure_memo_fresh(&rt, self.id);
        });
        self.signal.peek()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            ensure_memo_fresh(&rt, self.id);
        });
        self.signal.with(f)
    }
}

/// Lazy memo: marked dirty when sources change; recomputed on [`Memo::get`] or during flush
/// (before deeper effects), so diamond graphs do not observe intermediate values.
pub fn create_memo<T: Clone + PartialEq + 'static, F>(mut compute: F) -> Memo<T>
where
    F: FnMut() -> T + 'static,
{
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner_parent = ensure_owner(&rt);
        let owner = create_child_owner(&rt, owner_parent);
        let depth = owner_depth(&rt, owner);

        let initial = {
            let prev_owner = *rt.current_owner.borrow();
            *rt.current_owner.borrow_mut() = Some(owner);
            let v = untrack(&mut compute);
            *rt.current_owner.borrow_mut() = prev_owner;
            v
        };
        let signal = {
            // Create backing signal under the memo owner.
            let prev_owner = *rt.current_owner.borrow();
            *rt.current_owner.borrow_mut() = Some(owner);
            let s = rt.create_signal(initial);
            *rt.current_owner.borrow_mut() = prev_owner;
            s
        };

        let memo_id = MemoId(NEXT_MEMO_ID.fetch_add(1, Ordering::Relaxed));
        let signal_id = signal.id;

        let compute_rc: Rc<RefCell<dyn FnMut() -> bool>> = Rc::new(RefCell::new(move || {
            let next = compute();
            // Write backing signal without notifying (we notify explicitly if changed).
            let changed = RUNTIME.with(|rt_rc| {
                let rt = rt_rc.borrow();
                let any_val_rc = rt.signals.borrow().get(&signal_id).cloned().unwrap();
                let mut any_val = any_val_rc.borrow_mut();
                let typed = any_val.downcast_mut::<T>().unwrap();
                if *typed == next {
                    false
                } else {
                    *typed = next;
                    true
                }
            });
            changed
        }));

        rt.memos.borrow_mut().insert(
            memo_id,
            MemoState {
                owner,
                depth,
                dirty: Cell::new(true),
                signal: signal_id,
                compute: compute_rc,
            },
        );
        if let Some(node) = rt.owners.borrow_mut().get_mut(&owner) {
            node.memos.push(memo_id);
        }

        // First compute to establish deps.
        let _ = run_memo(&rt, memo_id);

        Memo {
            id: memo_id,
            signal,
        }
    })
}

pub use crate::style::{
    tick_animations, use_scrollbar_visibility_spring, use_spring, use_ui_transition_cubic,
    use_ui_transition_spring, AnimationTick, CubicTween, CubicTweenState, Spring, SpringState,
};

#[cfg(test)]
mod tests {
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
}
