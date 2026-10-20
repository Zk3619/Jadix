//! Solid-style reactive core with an Owner / Scope tree.
//!
//! ## Ownership model
//!
//! Every [`create_signal`], [`create_effect`], [`create_memo`], [`on_cleanup`], and [`on_mount`]
//! attaches to the
//! **current owner**. When that owner is disposed (explicitly, or because its parent effect
//! re-runs), children owners, effects, owned signals, and cleanups cascade away.
//!
//! Effects each own a scope: before an effect body re-runs, its previous child scopes are disposed
//! so nested `create_effect` calls no longer leak across runs.
//!
//! ## Named vs owned signals
//!
//! - [`create_signal`] — owned by the current scope; disposed with the owner.
//! - [`Runtime::create_unowned_signal`] / [`Signal::new_unowned`] — not tied to an owner; for
//!   animation backends held in [`use_signal`] that must outlive the creating effect.
//! - [`use_signal`] — **global** keyed registry (intentionally survives rebuilds for Input focus,
//!   scroll offsets, etc.).
//! - [`use_scoped_signal`] — keyed relative to the current owner path; same local id under one
//!   owner reuses the slot (type mismatch panics in debug).

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

thread_local! {
    static RUNTIME: RefCell<Rc<Runtime>> = RefCell::new(Runtime::new());
    /// Non-zero while a secondary window is the active input/render scope.
    /// Remaps a small set of process-global `__jadix_*` keys so focus / IME stay per-window.
    static SIGNAL_KEY_SCOPE: Cell<u64> = const { Cell::new(0) };
}

/// Keys that must not be shared between the primary window and secondaries.
const WINDOW_LOCAL_SIGNAL_KEYS: &[&str] = &[
    "__jadix_button_focus_id",
    "__jadix_focus_visible",
    "__jadix_ime_active",
];

/// Set the active window signal-key suffix (`0` = primary / tests). Returns the previous value.
///
/// Uses `try_with` so TLS destructors (Owner / overlay Drop) cannot panic-abort
/// when `SIGNAL_KEY_SCOPE` is already torn down.
pub(crate) fn set_signal_key_scope(scope: u64) -> u64 {
    SIGNAL_KEY_SCOPE.try_with(|c| c.replace(scope)).unwrap_or(0)
}

/// Current window signal-key suffix (`0` = primary).
pub(crate) fn signal_key_scope() -> u64 {
    SIGNAL_KEY_SCOPE.try_with(|c| c.get()).unwrap_or(0)
}

/// Stamp the current owner (and future children inherit) with a secondary-window token.
pub fn bind_current_owner_window_key_scope(scope: u64) {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let Some(id) = *rt.current_owner.borrow() else {
            return;
        };
        let mut owners = rt.owners.borrow_mut();
        if let Some(node) = owners.get_mut(&id) {
            node.window_key_scope = scope;
        }
    });
}

fn owner_window_key_scope(rt: &Runtime, owner: OwnerId) -> u64 {
    rt.owners
        .borrow()
        .get(&owner)
        .map(|n| n.window_key_scope)
        .unwrap_or(0)
}

struct OwnerWindowScopeGuard {
    prev: u64,
}

impl Drop for OwnerWindowScopeGuard {
    fn drop(&mut self) {
        SIGNAL_KEY_SCOPE.set(self.prev);
    }
}

fn enter_owner_window_scope(rt: &Runtime, owner: OwnerId) -> OwnerWindowScopeGuard {
    let scope = owner_window_key_scope(rt, owner);
    OwnerWindowScopeGuard {
        prev: SIGNAL_KEY_SCOPE.replace(scope),
    }
}

fn remap_window_local_signal_key(id_str: &str) -> std::borrow::Cow<'_, str> {
    let scope = SIGNAL_KEY_SCOPE.get();
    if scope == 0 || !WINDOW_LOCAL_SIGNAL_KEYS.iter().any(|k| *k == id_str) {
        std::borrow::Cow::Borrowed(id_str)
    } else {
        std::borrow::Cow::Owned(format!("{id_str}__w{scope}"))
    }
}

/// `true` while the reactivity thread-local is still usable (not in TLS destructor).
pub(crate) fn runtime_tls_alive() -> bool {
    RUNTIME.try_with(|_| ()).is_ok()
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SignalId(usize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EffectId(usize);

thread_local! {
    /// Effects that missed `try_reconcile_subtree` during an in-flight flush.
    /// Drained on the next idle turn (AppState not borrowed) — never re-queued into the
    /// same `flush_deferred` drain (that 100k-looped: miss → schedule → miss).
    static IDLE_EFFECT_RETRIES: RefCell<Vec<EffectId>> = const { RefCell::new(Vec::new()) };
    static IDLE_EFFECT_RETRY_COUNTS: RefCell<HashMap<EffectId, u8>> =
        RefCell::new(HashMap::new());
}

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
    /// Anonymous UI binding occurrences in the current execution of this owner.
    binding_slots: HashMap<String, usize>,
    /// Non-zero = secondary window signal-key token (see [`crate::app::window_scope`]).
    /// Inherited by child owners so nested effects keep the window without an enter guard.
    window_key_scope: u64,
}

/// Flush order (I-12 / WP-R.9): memos, then Computed, then Render (`bind_*`), then User.
///
/// Exposed for [`take_flushed_effects`] (WP-G.1 diagnostic; not a SemVer surface).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum EffectPhase {
    Computed = 0,
    Render = 1,
    User = 2,
}

struct EffectState {
    owner: OwnerId,
    depth: u32,
    phase: EffectPhase,
    f: Rc<RefCell<dyn FnMut()>>,
}

struct MemoState {
    owner: OwnerId,
    depth: u32,
    dirty: Cell<bool>,
    verified_revision: Cell<u64>,
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
    /// Signals created with [`Self::create_unowned_signal`] (not listed on any owner).
    unowned_signals: RefCell<HashSet<SignalId>>,
    pub named_signals: RefCell<HashMap<String, SignalId>>,
    named_signal_types: RefCell<HashMap<String, TypeId>>,
    effects: RefCell<HashMap<EffectId, EffectState>>,
    memos: RefCell<HashMap<MemoId, MemoState>>,
    /// Backing signal -> producing memo, for pull validation through clean memo chains.
    signal_memos: RefCell<HashMap<SignalId, MemoId>>,
    /// Invalidates pull validation once per notifying write, not once per read.
    memo_revision: Cell<u64>,
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
    /// Runs after the current memo/effect flush drains (Solid `createDeferred` catch-up).
    deferred_fns: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// Guard so `run_effect` → `flush_deferred_effects` stays iterative (avoids O(depth) stack).
    flushing_deferred: Cell<bool>,
    /// Guard so `notify` → `flush_pending` → `run_effect` → `notify` stays iterative.
    flushing_pending: Cell<bool>,
    /// In-flight `run_effect` nesting for this runtime (thread-local). Used to defer
    /// `create_effect` while `current_effect` is temporarily cleared (`untrack` / memo).
    effect_run_depth: Cell<usize>,
    /// Next [`create_effect`] becomes the app UI root (lifecycle `app_builder`). WP-2.7.
    mark_next_as_ui_root: Cell<bool>,
    /// Effect id for the marked UI root; bare `Signal::get` deps on this effect are hygiene hits.
    ui_root_effect: Cell<Option<EffectId>>,
    /// Latched once per UI-root install so debug builds do not spam on every root re-run.
    ui_root_bare_get_warned: Cell<bool>,
    /// Effects actually executed by `flush_pending` since the last [`take_flushed_effects`].
    flushed_effects: RefCell<Vec<(EffectId, EffectPhase)>>,
    /// `isolate_reads` discarded a tracked `get` (D6 / WP-G.2).
    isolated_discarded_read_count: Cell<usize>,
    /// Debug warn once per signal id for discarded isolated reads.
    isolated_discarded_warned: RefCell<HashSet<SignalId>>,
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
            unowned_signals: RefCell::new(HashSet::new()),
            named_signals: RefCell::new(HashMap::new()),
            named_signal_types: RefCell::new(HashMap::new()),
            effects: RefCell::new(HashMap::new()),
            memos: RefCell::new(HashMap::new()),
            signal_memos: RefCell::new(HashMap::new()),
            memo_revision: Cell::new(1),
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
            deferred_fns: RefCell::new(Vec::new()),
            flushing_deferred: Cell::new(false),
            flushing_pending: Cell::new(false),
            effect_run_depth: Cell::new(0),
            mark_next_as_ui_root: Cell::new(false),
            ui_root_effect: Cell::new(None),
            ui_root_bare_get_warned: Cell::new(false),
            flushed_effects: RefCell::new(Vec::new()),
            isolated_discarded_read_count: Cell::new(0),
            isolated_discarded_warned: RefCell::new(HashSet::new()),
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

    /// Like [`Self::create_signal`], but the value is **not** disposed with any owner.
    ///
    /// Use for long-lived animation values stored inside a [`use_signal`]-backed handle
    /// (`CubicTween`): the creating effect may dispose while ticks still write.
    pub fn create_unowned_signal<T: 'static>(&self, initial_value: T) -> Signal<T> {
        let id = SignalId(NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed));
        self.signals
            .borrow_mut()
            .insert(id, Rc::new(RefCell::new(initial_value)));
        self.unowned_signals.borrow_mut().insert(id);
        Signal {
            id,
            marker: std::marker::PhantomData,
        }
    }

    pub fn use_signal<T: 'static>(&self, id_str: &str, init: impl FnOnce() -> T) -> Signal<T> {
        let type_id = TypeId::of::<T>();
        let exists = { self.named_signals.borrow().get(id_str).copied() };

        if let Some(signal_id) = exists {
            if self.signals.borrow().contains_key(&signal_id) {
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
            // Named slot survived but storage was disposed (root owner teardown).
            self.named_signals.borrow_mut().remove(id_str);
            self.named_signal_types.borrow_mut().remove(id_str);
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
            binding_slots: HashMap::new(),
            window_key_scope: 0,
        },
    );
    *rt.root_owner.borrow_mut() = Some(id);
    *rt.current_owner.borrow_mut() = Some(id);
    id
}

fn create_child_owner(rt: &Runtime, parent: OwnerId) -> OwnerId {
    let id = OwnerId(NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed));
    let (path, window_key_scope) = {
        let owners = rt.owners.borrow();
        let parent_node = owners.get(&parent);
        let parent_path = parent_node.map(|n| n.path.as_str()).unwrap_or("root");
        let window_key_scope = parent_node.map(|n| n.window_key_scope).unwrap_or(0);
        (format!("{parent_path}/{}", id.0), window_key_scope)
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
            binding_slots: HashMap::new(),
            window_key_scope,
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
    // Iterative teardown: Gallery mounts create tall owner trees; recursive dispose
    // STATUS_STACK_OVERFLOW on Windows default stacks.
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        let Some(mut node) = rt.owners.borrow_mut().remove(&id) else {
            continue;
        };

        if let Some(parent) = node.parent {
            if let Some(parent_node) = rt.owners.borrow_mut().get_mut(&parent) {
                parent_node.children.retain(|c| *c != id);
            }
        }

        while let Some(cleanup) = node.cleanups.pop() {
            cleanup();
        }

        stack.extend(std::mem::take(&mut node.children));

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
            rt.named_signals
                .borrow_mut()
                .retain(|_, id| *id != signal_id);
            rt.named_signal_types
                .borrow_mut()
                .retain(|k, _| rt.named_signals.borrow().contains_key(k));
        }

        let clear_current = *rt.current_owner.borrow() == Some(id);
        if clear_current {
            *rt.current_owner.borrow_mut() = node.parent;
        }
        let clear_root = *rt.root_owner.borrow() == Some(id);
        if clear_root {
            *rt.root_owner.borrow_mut() = None;
        }
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
    rt.signal_memos.borrow_mut().remove(&state.signal);
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
    // Writing a subscribed signal from inside this body notifies us. Re-entering
    // would `borrow_mut` the same `FnMut` (ComboboxInput label sync).
    if *rt.current_effect.borrow() == Some(effect_id) {
        return;
    }
    let depth = rt.effect_run_depth.get() + 1;
    rt.effect_run_depth.set(depth);
    if depth > 512 {
        log::error!(
            "run_effect stack depth={depth} effect_id={} — likely infinite effect re-entry",
            effect_id.0
        );
        rt.effect_run_depth.set(depth - 1);
        panic!("run_effect recursion limit (depth={depth})");
    }
    let Some(state) = rt
        .effects
        .borrow()
        .get(&effect_id)
        .map(|s| (s.owner, s.phase, Rc::clone(&s.f)))
    else {
        rt.effect_run_depth.set(depth - 1);
        return;
    };
    let (owner, phase, effect_fn) = state;
    // Mount-time `run_effect` (create) is not a flush; only record `flush_pending` executions.
    if rt.flushing_pending.get() {
        rt.flushed_effects.borrow_mut().push((effect_id, phase));
    }

    // Solid: dispose previous child scopes before re-running.
    dispose_owner_children(rt, owner);
    if let Some(node) = rt.owners.borrow_mut().get_mut(&owner) {
        node.binding_slots.clear();
    }

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
    let _window_scope = enter_owner_window_scope(rt, owner);

    // GPUI `#[stacksafe]` analog: only grow when remaining < 64 KiB.
    with_ui_build_stack(|| (*effect_fn.borrow_mut())());

    let new_deps = rt
        .dep_stack
        .borrow_mut()
        .pop()
        .expect("dep_stack: empty after effect run");
    let dep_count = new_deps.len();
    rt.effect_deps.borrow_mut().insert(effect_id, new_deps);

    if rt.ui_root_effect.get() == Some(effect_id) && dep_count > 0 {
        #[cfg(debug_assertions)]
        if !rt.ui_root_bare_get_warned.get() {
            rt.ui_root_bare_get_warned.set(true);
            log::warn!(
                "root app_builder subscribed {dep_count} signal(s) via bare Signal::get() \
                 — prefer bind_* / Show / For / reactive! \
                 (see docs/ops/reactivity_and_hot_reload.md; diagnose with ui_root_effect_dep_count)"
            );
        }
    }

    *rt.current_effect.borrow_mut() = prev_effect;
    *rt.current_owner.borrow_mut() = prev_owner;
    flush_deferred_effects(rt);
    rt.effect_run_depth.set(depth - 1);
}

fn flush_deferred_effects(rt: &Runtime) {
    // Nested `run_effect` must not recursively flush: a tall create_effect chain
    // (gallery / many bind_text) would STATUS_STACK_OVERFLOW on Windows defaults.
    if rt.flushing_deferred.get() {
        return;
    }
    rt.flushing_deferred.set(true);
    let mut rounds = 0usize;
    // Each id at most once per drain. `reactive_anchor` reconcile-miss used to
    // `schedule_effect` into this same Vec; with `flushing_deferred` true that
    // re-ran forever (`flush_deferred_effects exceeded 100k rounds`).
    let mut ran: HashSet<EffectId> = HashSet::new();
    loop {
        let mut ids: Vec<EffectId> = std::mem::take(&mut *rt.deferred_effects.borrow_mut());
        ids.retain(|id| ran.insert(*id));
        if ids.is_empty() {
            break;
        }
        rounds += 1;
        if rounds == 1 || rounds % 50 == 0 {
            log::debug!("flush_deferred: round={rounds} batch={}", ids.len());
        }
        if rounds > 100_000 {
            panic!("flush_deferred_effects exceeded 100k rounds — likely effect create loop");
        }
        ids.sort_by_key(|id| effect_phase_depth(rt, *id));
        for id in ids {
            run_effect(rt, id);
        }
    }
    if rounds > 0 {
        log::debug!("flush_deferred: done rounds={rounds}");
    }
    rt.flushing_deferred.set(false);
}

fn enqueue_pending_effect(rt: &Runtime, id: EffectId) {
    let mut set = rt.pending_effect_set.borrow_mut();
    if set.insert(id) {
        rt.pending_effects.borrow_mut().push_back(id);
    }
}

const MAX_IDLE_EFFECT_RETRIES: u8 = 8;

/// Retry an in-flight effect after the current flush (and AppState borrow) ends.
///
/// Used when `try_reconcile_subtree` misses (borrow held / placeholder not in the live
/// tree yet). Must not use [`schedule_effect`] during `flush_deferred` — that re-queues
/// the same id into the drain that is already running.
pub(crate) fn queue_running_effect_for_idle_retry() {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let Some(id) = *rt.current_effect.borrow() else {
            return;
        };
        let allow = IDLE_EFFECT_RETRY_COUNTS.with(|c| {
            let mut c = c.borrow_mut();
            let n = c.entry(id).or_insert(0);
            if *n >= MAX_IDLE_EFFECT_RETRIES {
                return false;
            }
            *n = n.saturating_add(1);
            true
        });
        if !allow {
            log::debug!("idle effect retry cap reached effect_id={}", id.0);
            return;
        }
        IDLE_EFFECT_RETRIES.with(|q| {
            let mut q = q.borrow_mut();
            if !q.contains(&id) {
                q.push(id);
            }
        });
    });
}

pub(crate) fn clear_idle_effect_retry(effect_id: EffectId) {
    IDLE_EFFECT_RETRY_COUNTS.with(|c| {
        c.borrow_mut().remove(&effect_id);
    });
}

pub(crate) fn clear_idle_effect_retry_queues() {
    IDLE_EFFECT_RETRIES.with(|q| q.borrow_mut().clear());
    IDLE_EFFECT_RETRY_COUNTS.with(|c| c.borrow_mut().clear());
}

/// Run effects queued by [`queue_running_effect_for_idle_retry`]. Call only when AppState
/// is not borrowed (same sites as `drain_deferred_view_remounts`).
pub fn drain_idle_effect_retries() {
    let ids = IDLE_EFFECT_RETRIES.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for id in ids {
        schedule_effect(id);
    }
}

/// Re-queue an effect for the next flush (e.g. remount a reactive island).
pub(crate) fn schedule_effect(effect_id: EffectId) {
    if !effect_is_alive(effect_id) {
        return;
    }
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        if rt.current_effect.borrow().is_some()
            || rt.flushing_deferred.get()
            || rt.flushing_pending.get()
            || rt.effect_run_depth.get() > 0
        {
            // Nested under an in-flight effect: run after the current effect finishes
            // (same path as `create_render_effect` registration during mount).
            rt.deferred_effects.borrow_mut().push(effect_id);
        } else {
            enqueue_pending_effect(&rt, effect_id);
            drop(rt);
            flush_pending(&rt_rc.borrow());
        }
    });
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
    let _window_scope = enter_owner_window_scope(rt, owner);

    let changed = (*compute.borrow_mut())();

    let new_deps = rt.dep_stack.borrow_mut().pop().unwrap_or_default();
    rt.memo_deps.borrow_mut().insert(memo_id, new_deps);

    if let Some(memo) = rt.memos.borrow().get(&memo_id) {
        memo.dirty.set(false);
        memo.verified_revision.set(rt.memo_revision.get());
    }

    *rt.current_memo.borrow_mut() = prev_memo;
    *rt.current_owner.borrow_mut() = prev_owner;
    *rt.current_effect.borrow_mut() = prev_effect;

    if changed {
        // A pull inside batch must notify just like a queued recomputation. Do
        // not flush here: the caller may still be collecting memo/effect deps.
        let effects = rt
            .signal_subscribers
            .borrow()
            .get(&signal_id)
            .cloned()
            .unwrap_or_default();
        let running = *rt.current_effect.borrow();
        for effect in effects {
            if running != Some(effect) {
                enqueue_pending_effect(rt, effect);
            }
        }
        mark_memos_dirty_from_signal(rt, signal_id);
    }
    changed
}

fn ensure_memo_fresh(rt: &Runtime, memo_id: MemoId) {
    if rt
        .memos
        .borrow()
        .get(&memo_id)
        .is_none_or(|m| !m.dirty.get() && m.verified_revision.get() == rt.memo_revision.get())
    {
        return;
    }
    // An upstream memo may be dirty while this memo is still clean. Validate
    // producers first; their equality check decides whether we need to compute.
    let producers: Vec<MemoId> = {
        let deps = rt.memo_deps.borrow();
        let producers = rt.signal_memos.borrow();
        deps.get(&memo_id)
            .into_iter()
            .flatten()
            .filter_map(|signal| producers.get(signal).copied())
            .collect()
    };
    for producer in producers {
        ensure_memo_fresh(rt, producer);
    }
    let dirty = rt
        .memos
        .borrow()
        .get(&memo_id)
        .map(|m| m.dirty.get())
        .unwrap_or(false);
    if dirty {
        let _ = run_memo(rt, memo_id);
    }
    if let Some(memo) = rt.memos.borrow().get(&memo_id) {
        memo.verified_revision.set(rt.memo_revision.get());
    }
}

fn effect_phase_depth(rt: &Runtime, id: EffectId) -> (u8, u32) {
    rt.effects
        .borrow()
        .get(&id)
        .map(|e| (e.phase as u8, e.depth))
        .unwrap_or((u8::MAX, u32::MAX))
}

fn effect_phase_of(rt: &Runtime, id: EffectId) -> EffectPhase {
    rt.effects
        .borrow()
        .get(&id)
        .map(|e| e.phase)
        .unwrap_or(EffectPhase::User)
}

/// Drain pending memos, then effects by phase (Computed → Render → User), each by depth.
fn flush_pending(rt: &Runtime) {
    // Re-entrant notify during an in-flight flush must only enqueue; the outer loop drains.
    if rt.flushing_pending.get() {
        return;
    }
    rt.flushing_pending.set(true);
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
                ensure_memo_fresh(rt, memo_id);
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
                let jobs = std::mem::take(&mut *rt.deferred_fns.borrow_mut());
                if jobs.is_empty() {
                    rt.flushing_pending.set(false);
                    return;
                }
                drop(q);
                drop(set);
                for job in jobs {
                    with_ui_build_stack(job);
                }
                continue;
            }
            let drained: Vec<EffectId> = q.drain(..).collect();
            set.clear();
            drained
        };

        let min_phase = effect_chunk
            .iter()
            .map(|id| effect_phase_of(rt, *id))
            .min()
            .unwrap_or(EffectPhase::User);

        let mut now = Vec::new();
        for id in effect_chunk {
            if effect_phase_of(rt, id) == min_phase {
                now.push(id);
            } else {
                enqueue_pending_effect(rt, id);
            }
        }
        now.sort_by_key(|id| {
            rt.effects
                .borrow()
                .get(id)
                .map(|e| e.depth)
                .unwrap_or(u32::MAX)
        });
        for effect_id in now {
            run_effect(rt, effect_id);
        }
        // Effects may have dirtied more memos / effects.
        if rt.pending_memos.borrow().is_empty() && rt.pending_effects.borrow().is_empty() {
            let jobs = std::mem::take(&mut *rt.deferred_fns.borrow_mut());
            if jobs.is_empty() {
                rt.flushing_pending.set(false);
                return;
            }
            for job in jobs {
                with_ui_build_stack(job);
            }
            // Deferred writes may enqueue more work — continue the loop.
        }
    }
}

fn enqueue_deferred_fn(rt: &Runtime, f: Box<dyn FnOnce()>) {
    rt.deferred_fns.borrow_mut().push(f);
}

fn notify_signal_subscribers(rt: &Runtime, signal_id: SignalId) {
    rt.memo_revision.set(rt.memo_revision.get().wrapping_add(1));
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
    // Skip the in-flight effect — it already sees the write; flushing it now re-enters.
    let running = *rt.current_effect.borrow();
    for effect_id in effects {
        if running == Some(effect_id) {
            continue;
        }
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

impl<T> Signal<T> {
    pub(crate) fn id(self) -> SignalId {
        self.id
    }
}

impl<T: Clone + 'static> Signal<T> {
    pub fn new(value: T) -> Self {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            rt.create_signal(value)
        })
    }

    /// Create a signal that is not disposed with the current owner (see
    /// [`Runtime::create_unowned_signal`]).
    pub fn new_unowned(value: T) -> Self {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            rt.create_unowned_signal(value)
        })
    }

    /// `true` while this id still has storage in the runtime.
    pub fn is_alive(&self) -> bool {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let alive = rt.signals.borrow().contains_key(&self.id);
            alive
        })
    }

    /// Read and subscribe the current effect/memo. The returned `T` is an ordinary
    /// value: passing it to a Builder does not create a separate leaf subscription.
    /// Pass the signal or a closure to the Builder; `ui!` can lower tracked reads.
    pub fn get(&self) -> T {
        self.try_get().expect("Signal::get on disposed signal")
    }

    /// Subscribe the current effect / memo and read. `None` if already disposed.
    pub fn try_get(&self) -> Option<T> {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            track_signal_read(&rt, self.id);
            let any_val_rc = rt.signals.borrow().get(&self.id).cloned()?;
            let any_val = any_val_rc.borrow();
            Some(
                any_val
                    .downcast_ref::<T>()
                    .expect("Signal type mismatch")
                    .clone(),
            )
        })
    }

    /// Read without subscribing. Returns `None` if this id was already disposed.
    pub fn try_peek(&self) -> Option<T> {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt.signals.borrow().get(&self.id).cloned()?;
            let any_val = any_val_rc.borrow();
            Some(
                any_val
                    .downcast_ref::<T>()
                    .expect("Signal type mismatch")
                    .clone(),
            )
        })
    }

    /// Read without subscribing the current effect / memo.
    pub fn peek(&self) -> T {
        self.try_peek().expect("Signal::peek on disposed signal")
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
        if !self.try_set_silent(new_value) {
            panic!("Signal::set_silent on disposed signal");
        }
    }

    /// Like [`Self::set_silent`], but returns `false` if the signal was already disposed.
    pub fn try_set_silent(&self, new_value: T) -> bool {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let Some(any_val_rc) = rt.signals.borrow().get(&self.id).cloned() else {
                return false;
            };
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().expect("Signal type mismatch");
            if *typed_val != new_value {
                *typed_val = new_value;
            }
            true
        })
    }

    pub fn set(&self, new_value: T) {
        if !self.try_set(new_value) {
            panic!("Signal::set on disposed signal");
        }
    }

    /// Like [`Self::set`], but returns `false` if the signal was already disposed.
    pub fn try_set(&self, new_value: T) -> bool {
        let changed = RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            let any_val_rc = rt.signals.borrow().get(&self.id).cloned()?;
            let mut any_val = any_val_rc.borrow_mut();
            let typed_val = any_val.downcast_mut::<T>().expect("Signal type mismatch");
            if *typed_val == new_value {
                Some(false)
            } else {
                *typed_val = new_value;
                Some(true)
            }
        });
        match changed {
            None => false,
            Some(false) => true,
            Some(true) => {
                RUNTIME.with(|rt_rc| {
                    let rt = rt_rc.borrow();
                    notify_signal_subscribers(&rt, self.id);
                });
                true
            }
        }
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
    create_effect_with_phase(EffectPhase::User, f)
}

/// Render-phase effect used by `bind_*` (and structure islands). Runs after memos /
/// [`create_computed`] and **before** user [`create_effect`] (I-12).
pub(crate) fn create_render_effect<F>(f: F) -> EffectId
where
    F: FnMut() + 'static,
{
    create_effect_with_phase(EffectPhase::Render, f)
}

/// Eager computed (Solid `createComputed`): same-frame, **before** `bind_*` and user effects.
///
/// Use this to synchronously write other signals. Prefer [`create_memo`] when the result is a
/// derived value readers should subscribe to. Does not introduce a `RenderEffect` type.
pub fn create_computed<F>(f: F) -> EffectId
where
    F: FnMut() + 'static,
{
    create_effect_with_phase(EffectPhase::Computed, f)
}

fn create_effect_with_phase<F>(phase: EffectPhase, f: F) -> EffectId
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
                phase,
                f: Rc::clone(&effect_rc),
            },
        );
        if let Some(node) = rt.owners.borrow_mut().get_mut(&owner) {
            node.effects.push(id);
        }

        if phase == EffectPhase::User && rt.mark_next_as_ui_root.get() {
            rt.mark_next_as_ui_root.set(false);
            rt.ui_root_effect.set(Some(id));
            rt.ui_root_bare_get_warned.set(false);
        }

        // Defer when nested under an in-flight effect flush — including windows where
        // `current_effect` is temporarily `None` (`untrack` / `run_memo`). Sync nesting
        // there used to STATUS_STACK_OVERFLOW on Windows when Gallery registered hundreds
        // of bind_/reactive_view effects during mount.
        let should_defer = rt.current_effect.borrow().is_some()
            || rt.flushing_deferred.get()
            || rt.flushing_pending.get()
            || rt.effect_run_depth.get() > 0;
        if should_defer {
            rt.deferred_effects.borrow_mut().push(id);
        } else {
            run_effect(&rt, id);
            // `create_deferred` may have queued catch-up writes during the initial run.
            if !rt.deferred_fns.borrow().is_empty() {
                flush_pending(&rt);
            }
        }
        id
    })
}

/// Mark the next [`create_effect`] as the app UI root (`app_builder` in lifecycle).
///
/// Bare `Signal::get()` while that effect runs subscribe the **whole tree**. Prefer
/// `bind_*` / Show / For / `reactive!`. Diagnose with [`ui_root_effect_dep_count`].
pub fn mark_next_effect_as_ui_root() {
    RUNTIME.with(|rt_rc| {
        rt_rc.borrow().mark_next_as_ui_root.set(true);
    });
}

/// Clear the UI-root marker (tests / soft restart). Does not dispose the effect.
pub fn clear_ui_root_effect() {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        rt.ui_root_effect.set(None);
        rt.mark_next_as_ui_root.set(false);
        rt.ui_root_bare_get_warned.set(false);
    });
}

/// Number of signals currently subscribed by the marked UI-root effect (0 = hygienic).
///
/// After a completed root run: bare `get()` in `app_builder` → `> 0`; bind / Show / For only → `0`.
pub fn ui_root_effect_dep_count() -> usize {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let Some(id) = rt.ui_root_effect.get() else {
            return 0;
        };
        let n = rt
            .effect_deps
            .borrow()
            .get(&id)
            .map(|deps| deps.len())
            .unwrap_or(0);
        n
    })
}

/// Drain `(effect, phase)` pairs executed by `flush_pending` since the previous take (WP-G.1).
///
/// Diagnostic hook for “this `set` ran whom”. Not a SemVer surface. Mount-time
/// `create_effect` runs are excluded (they are not a flush).
pub fn take_flushed_effects() -> Vec<(EffectId, EffectPhase)> {
    RUNTIME.with(|rt_rc| std::mem::take(&mut *rt_rc.borrow().flushed_effects.borrow_mut()))
}

/// How many tracked reads [`isolate_reads`] discarded (D6 / WP-G.2).
pub fn isolated_discarded_read_count() -> usize {
    RUNTIME.with(|rt_rc| rt_rc.borrow().isolated_discarded_read_count.get())
}

/// Clear the WP-G.2 discarded-read counter (tests).
pub fn reset_isolated_discarded_read_count() {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        rt.isolated_discarded_read_count.set(0);
        rt.isolated_discarded_warned.borrow_mut().clear();
    });
}

/// Alias of [`ui_root_effect_dep_count`] (WP-2.7 / R-WP-2 diagnostic hook).
pub fn root_bare_get_count() -> usize {
    ui_root_effect_dep_count()
}

/// Whether `id` still refers to a live effect slot (not yet disposed).
pub fn effect_is_alive(id: EffectId) -> bool {
    RUNTIME.with(|rt_rc| rt_rc.borrow().effects.borrow().contains_key(&id))
}

/// Dispose one effect and its owner scope (cascades to nested effects / signals / cleanups).
pub fn dispose_effect(id: EffectId) {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        if rt.ui_root_effect.get() == Some(id) {
            rt.ui_root_effect.set(None);
            rt.ui_root_bare_get_warned.set(false);
        }
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
        // Copy the Option out first — an `if let` temporary from `borrow()` would
        // stay alive across `dispose_owner`, which itself mutates `root_owner`.
        let root = *rt.root_owner.borrow();
        if let Some(root) = root {
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
        rt.signal_memos.borrow_mut().clear();
        rt.memo_deps.borrow_mut().clear();
        rt.signal_subscribers.borrow_mut().clear();
        rt.signal_memo_subscribers.borrow_mut().clear();
        rt.pending_effects.borrow_mut().clear();
        rt.pending_effect_set.borrow_mut().clear();
        rt.pending_memos.borrow_mut().clear();
        rt.pending_memo_set.borrow_mut().clear();
        rt.deferred_effects.borrow_mut().clear();
        rt.deferred_fns.borrow_mut().clear();
        clear_idle_effect_retry_queues();
        rt.flushing_deferred.set(false);
        rt.flushing_pending.set(false);
        rt.effect_run_depth.set(0);
        rt.mark_next_as_ui_root.set(false);
        rt.ui_root_effect.set(None);
        rt.ui_root_bare_get_warned.set(false);
        rt.flushed_effects.borrow_mut().clear();
        rt.isolated_discarded_read_count.set(0);
        rt.isolated_discarded_warned.borrow_mut().clear();
        *rt.current_effect.borrow_mut() = None;
        *rt.current_memo.borrow_mut() = None;
        *rt.current_owner.borrow_mut() = None;
        *rt.root_owner.borrow_mut() = None;
        // Owned signals were removed with owners; keep named + unowned (animation) slots.
        let named: HashSet<SignalId> = rt.named_signals.borrow().values().copied().collect();
        let unowned = rt.unowned_signals.borrow().clone();
        rt.signals
            .borrow_mut()
            .retain(|id, _| named.contains(id) || unowned.contains(id));
    });
}

/// `true` if a named slot is currently bound (does **not** create one).
pub fn named_signal_exists(id_str: &str) -> bool {
    RUNTIME
        .try_with(|rt_rc| rt_rc.borrow().named_signals.borrow().contains_key(id_str))
        .unwrap_or(false)
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

/// Run `f` once when the current owner is first set up (Solid `onMount`).
///
/// Implemented as a no-dep effect that takes the callback on the first tick, so it does **not**
/// re-run when unrelated signals change. Reading signals inside `f` is untracked.
///
/// Re-mounting (owner disposed and a new owner created, or a parent effect re-creating children)
/// schedules a fresh `on_mount`.
pub fn on_mount<F>(f: F)
where
    F: FnOnce() + 'static,
{
    let slot = RefCell::new(Some(f));
    create_effect(move || {
        if let Some(cb) = slot.borrow_mut().take() {
            untrack(cb);
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
        if !runtime_tls_alive() {
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

/// GPUI `#[stacksafe]` analog: grow the call stack only when remaining space is
/// inside a small red zone (not a PE `/STACK` bump, not a per-event 8 MiB wrap).
///
/// `stacker`'s documented pattern is `maybe_grow(32 KiB, 1 MiB, …)`. Jadix uses
/// **64 KiB** red zone / **1 MiB** segment so a wide MSVC-debug `ui!` prologue
/// still fits after `Element` became a pointer, without forcing grow on every
/// click of a 1 MiB GUI thread (the old 1 MiB red zone always tripped).
///
/// Framework hotspots: `run_effect` user body, [`component_root`], Taffy
/// `compute_layout`, each `run_on_ui_thread` job, file-drop callback, HMR
/// `app_builder`. Gallery `run_on_gallery_build_stack` still wraps construction.
#[inline(never)]
pub fn with_ui_build_stack<R>(f: impl FnOnce() -> R) -> R {
    const RED_ZONE: usize = 64 * 1024;
    const GROW: usize = 1024 * 1024;
    stacker::maybe_grow(RED_ZONE, GROW, f)
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
                binding_slots: HashMap::new(),
                window_key_scope: 0,
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

/// Effect currently executing, if any.
pub(crate) fn current_effect_id() -> Option<EffectId> {
    RUNTIME.with(|rt_rc| *rt_rc.borrow().current_effect.borrow())
}

/// True while a `bind_*` / `reactive_view` render effect is running.
/// `mounted.set` inside that island would re-flush `body()`; use `set_silent`.
pub(crate) fn current_effect_is_render_phase() -> bool {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let Some(id) = *rt.current_effect.borrow() else {
            return false;
        };
        effect_phase_of(&rt, id) == EffectPhase::Render
    })
}

/// Live owner nodes in the runtime graph (WP-R.5 / leak diagnostics).
pub fn live_owner_count() -> usize {
    RUNTIME.with(|rt_rc| rt_rc.borrow().owners.borrow().len())
}

/// WP-R.8 debug edge: one signal and its current effect subscribers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalEffectEdge {
    pub signal_id: usize,
    pub effect_ids: Vec<usize>,
}

/// WP-R.8 debug edge: one signal and memos that read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalMemoEdge {
    pub signal_id: usize,
    pub memo_ids: Vec<usize>,
}

/// WP-R.8 debug entry: one named signal key and its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedSignalDebugEntry {
    pub key: String,
    pub signal_id: usize,
}

/// WP-R.8 snapshot hook (non-DevTools UI): runtime graph counts + adjacency edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactivityDebugSnapshot {
    pub live_owners: usize,
    pub live_effects: usize,
    pub live_memos: usize,
    pub live_signals: usize,
    pub named_signals: usize,
    pub pending_effects: usize,
    pub pending_memos: usize,
    pub ui_root_dep_count: usize,
    pub named_signal_entries: Vec<NamedSignalDebugEntry>,
    pub signal_effect_edges: Vec<SignalEffectEdge>,
    pub signal_memo_edges: Vec<SignalMemoEdge>,
}

/// Collect a structured reactivity graph snapshot for leak / dependency diagnostics.
pub fn reactivity_debug_snapshot() -> ReactivityDebugSnapshot {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let live_owners = rt.owners.borrow().len();
        let live_effects = rt.effects.borrow().len();
        let live_memos = rt.memos.borrow().len();
        let live_signals = rt.signals.borrow().len();
        let named_signals = rt.named_signals.borrow().len();
        let pending_effects = rt.pending_effects.borrow().len();
        let pending_memos = rt.pending_memos.borrow().len();
        let ui_root_dep_count = ui_root_effect_dep_count();
        let mut named_signal_entries: Vec<NamedSignalDebugEntry> = rt
            .named_signals
            .borrow()
            .iter()
            .map(|(key, signal_id)| NamedSignalDebugEntry {
                key: key.clone(),
                signal_id: signal_id.0,
            })
            .collect();
        named_signal_entries.sort_by(|a, b| a.key.cmp(&b.key).then(a.signal_id.cmp(&b.signal_id)));

        let mut signal_effect_edges: Vec<SignalEffectEdge> = rt
            .signal_subscribers
            .borrow()
            .iter()
            .map(|(signal_id, effects)| {
                let mut effect_ids: Vec<usize> = effects.iter().map(|id| id.0).collect();
                effect_ids.sort_unstable();
                SignalEffectEdge {
                    signal_id: signal_id.0,
                    effect_ids,
                }
            })
            .collect();
        signal_effect_edges.sort_by_key(|edge| edge.signal_id);

        let mut signal_memo_edges: Vec<SignalMemoEdge> = rt
            .signal_memo_subscribers
            .borrow()
            .iter()
            .map(|(signal_id, memos)| {
                let mut memo_ids: Vec<usize> = memos.iter().map(|id| id.0).collect();
                memo_ids.sort_unstable();
                SignalMemoEdge {
                    signal_id: signal_id.0,
                    memo_ids,
                }
            })
            .collect();
        signal_memo_edges.sort_by_key(|edge| edge.signal_id);

        ReactivityDebugSnapshot {
            live_owners,
            live_effects,
            live_memos,
            live_signals,
            named_signals,
            pending_effects,
            pending_memos,
            ui_root_dep_count,
            named_signal_entries,
            signal_effect_edges,
            signal_memo_edges,
        }
    })
}

impl ReactivityDebugSnapshot {
    /// Compact one-line dump for logs / tests (WP-R.8). Not a DevTools UI.
    pub fn dump(&self) -> String {
        format!(
            "owners={} effects={} memos={} signals={} named={} pending_e={} pending_m={} root_deps={} e_edges={} m_edges={}",
            self.live_owners,
            self.live_effects,
            self.live_memos,
            self.live_signals,
            self.named_signals,
            self.pending_effects,
            self.pending_memos,
            self.ui_root_dep_count,
            self.signal_effect_edges.len(),
            self.signal_memo_edges.len(),
        )
    }
}

/// Convenience alias of [`reactivity_debug_snapshot`]`.dump()`.
pub fn dump_reactivity_graph() -> String {
    reactivity_debug_snapshot().dump()
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

/// Owner-prefixed `template_id` (WP-2.4). Prefer for structural islands under a dynamic owner
/// so remounts under different roots do not collide in the global string space.
///
/// Without a current owner, falls back to `__jadix_tpl_global::{local}`.
pub fn scoped_template_id(local: impl AsRef<str>) -> String {
    let local = local.as_ref().trim();
    let local = if local.is_empty() { "anon" } else { local };
    match current_owner() {
        Some(id) => format!("{}::tpl:{}", owner_path_str(id), local),
        None => format!("__jadix_tpl_global::{local}"),
    }
}

/// One anonymous leaf per owner/site/occurrence. Effect reruns restart occurrences;
/// explicit template ids and keyed-list business keys remain separate identities.
pub(crate) fn next_binding_template_id(site: &str) -> String {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner = ensure_owner(&rt);
        let mut owners = rt.owners.borrow_mut();
        let node = owners.get_mut(&owner).expect("binding owner disposed");
        let occurrence = node.binding_slots.entry(site.to_string()).or_default();
        let id = format!("{}::bind:{site}:{}", node.path, *occurrence);
        *occurrence += 1;
        id
    })
}

/// Owner-prefixed `focus_id` (WP-2.4). Use for focusable chrome that must not share the
/// global focus namespace across independently disposed owners.
///
/// Without a current owner, falls back to `__jadix_focus_global::{local}`.
pub fn scoped_focus_id(local: impl AsRef<str>) -> String {
    let local = local.as_ref().trim();
    let local = if local.is_empty() { "anon" } else { local };
    match current_owner() {
        Some(id) => format!("{}::focus:{}", owner_path_str(id), local),
        None => format!("__jadix_focus_global::{local}"),
    }
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

struct OwnerRestore(Option<OwnerId>);

impl Drop for OwnerRestore {
    fn drop(&mut self) {
        let prev = self.0;
        let _ = RUNTIME.try_with(|rt_rc| {
            rt_rc.borrow().current_owner.replace(prev);
        });
    }
}

/// Allocate shared resources under their source owner rather than a transient reader.
pub(crate) fn with_owner<R>(owner: OwnerId, f: impl FnOnce() -> R) -> R {
    let prev = RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        assert!(rt.owners.borrow().contains_key(&owner), "owner disposed");
        rt.current_owner.replace(Some(owner))
    });
    let _restore = OwnerRestore(prev);
    f()
}

/// Run `f` as a **component-instance root** (Solid-style owner), then restore the previous owner.
///
/// [`macro@crate::component`] wraps each invocation in this automatically (WP-3.14).
/// Builder-only composite roots (Select / Combobox / RadioGroup) must still call it at the
/// instance root: Trigger, dropdown `reactive_anchor` / `create_effect`, and Item `use_context`
/// hang on this subtree so the menu belongs to that instance, not a parent DatePicker overlay.
///
/// Nested calls are cheap extra owners (correct isolation). Child owners stay on the tree and
/// dispose with the parent. If omitted on a **Builder** root, sibling `provide_context` calls
/// overwrite the same [`Context`] id (three TimePicker Comboboxes sharing the last menu).
pub fn component_root<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let prev = RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let parent = ensure_owner(&rt);
        let child = create_child_owner(&rt, parent);
        rt.current_owner.replace(Some(child))
    });
    let _restore = OwnerRestore(prev);
    with_ui_build_stack(f)
}

/// Alias of [`component_root`]. Prefer [`component_root`] at new Builder call sites.
pub fn with_child_owner<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    component_root(f)
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

/// Run `f` with `current_effect` / `current_memo` cleared ([`is_tracking`] is false).
///
/// Do **not** wrap a subtree that must call [`crate::app::reactive::reactive_anchor`]:
/// registration bails out when tracking is off. Use [`isolate_reads`] instead.
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
            let should_flush = {
                let mut depth = rt.batch_depth.borrow_mut();
                debug_assert!(*depth > 0, "batch_depth underflow");
                *depth -= 1;
                *depth == 0
            };
            if should_flush {
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
    let remapped = remap_window_local_signal_key(id_str);
    RUNTIME.with(|rt_rc| rt_rc.borrow().use_signal(remapped.as_ref(), init))
}

// ─── Trigger (Solid `createTrigger`) ────────────────────────────────────────

/// A zero-payload reactive primitive: `.track()` subscribes, `.notify()` fires subscribers.
///
/// Unlike `Signal<()>`, it never compares values — every `notify()` unconditionally schedules
/// dependents. Use for invalidation-only patterns (e.g. force re-render, cache bust).
#[derive(Clone, Copy)]
pub struct Trigger {
    id: SignalId,
}

impl Trigger {
    pub fn new() -> Self {
        let sig: Signal<()> = Signal::new(());
        Self { id: sig.id }
    }

    /// Subscribe the current effect / memo without reading a value.
    pub fn track(&self) {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            track_signal_read(&rt, self.id);
        });
    }

    /// Fire all subscribers unconditionally.
    pub fn notify(&self) {
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            notify_signal_subscribers(&rt, self.id);
        });
    }

    /// Whether the underlying signal slot is still alive.
    pub fn is_alive(&self) -> bool {
        RUNTIME.with(|rt_rc| rt_rc.borrow().signals.borrow().contains_key(&self.id))
    }
}

/// Create a [`Trigger`] (Solid `createTrigger`).
pub fn create_trigger() -> Trigger {
    Trigger::new()
}

// ─── Transition ─────────────────────────────────────────────────────────────

/// Run `f` inside a deferred batch: reactive updates inside `f` are committed after the
/// current flush completes (Solid `startTransition` analogy on a frame-synchronous runtime).
///
/// If no flush is in progress, the deferred fn executes immediately via a flush kick.
pub fn start_transition<F: FnOnce() + 'static>(f: F) {
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        enqueue_deferred_fn(&rt, Box::new(f));
        if !rt.flushing_pending.get() {
            flush_pending(&rt);
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────

/// Run `f` while tracking stays on ([`is_tracking`] is true) but collected `get()` deps are discarded.
///
/// Use this around a floating-panel / `reactive_anchor` subtree so child widgets can still
/// **register** anchors. [`untrack`] clears `current_effect`, so `reactive_anchor` returns
/// immediately (empty overlay). Contract test: `untrack_skips_anchor_but_isolate_reads_registers`.
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
                rt.isolated_discarded_read_count
                    .set(rt.isolated_discarded_read_count.get().saturating_add(1));
                #[cfg(debug_assertions)]
                {
                    let mut warned = rt.isolated_discarded_warned.borrow_mut();
                    if warned.insert(sig_id) {
                        log::warn!(
                            "isolate_reads discarded a tracked Signal::get() \
                             — this is a missed bind, not an update \
                             (see docs/ops/reactivity_and_hot_reload.md; \
                             diagnose with isolated_discarded_read_count)"
                        );
                    }
                }
            }
        }
        out
    })
}

/// Whether default `#[component]` bodies run under [`isolate_reads`] (D2 / N.3).
///
/// Default **on**. Set `JADIX_COMPONENT_ISOLATE_DEFAULT=0` / `false` / `off` to roll back
/// while a Gallery page still relies on a parent-effect subscription.
pub fn component_isolate_default() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("JADIX_COMPONENT_ISOLATE_DEFAULT") {
        Ok(v) => {
            let t = v.trim();
            !(t == "0" || t.eq_ignore_ascii_case("false") || t.eq_ignore_ascii_case("off"))
        }
        Err(_) => true,
    })
}

/// Run `f` under [`isolate_reads`] when [`component_isolate_default`] is on.
///
/// Used by the default `#[component]` expansion so a child body's naked `get()` does
/// not subscribe a parent effect. [`crate::app::reactive::reactive_component`] already
/// isolates its render closure — do not wrap that path again.
/// [`crate::app::reactive::reactive_component_live`] stays un-isolated.
pub fn component_isolated_body<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    if component_isolate_default() {
        isolate_reads(f)
    } else {
        f()
    }
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

pub struct Memo<T> {
    id: MemoId,
    signal: Signal<T>,
}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Memo<T> {}

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
                verified_revision: Cell::new(0),
                signal: signal_id,
                compute: compute_rc,
            },
        );
        if let Some(node) = rt.owners.borrow_mut().get_mut(&owner) {
            node.memos.push(memo_id);
        }

        rt.signal_memos.borrow_mut().insert(signal_id, memo_id);
        // First compute to establish deps.
        let _ = run_memo(&rt, memo_id);

        Memo {
            id: memo_id,
            signal,
        }
    })
}

// ---------------------------------------------------------------------------
// Solid-aligned helpers: on / createSelector / createDeferred
// ---------------------------------------------------------------------------

/// Run `f` when `deps` changes. Only `deps` is tracked; the body runs under [`untrack`]
/// (Solid `on(deps, fn)`).
///
/// `f` receives `(current, previous)`. On the first run `previous` is `None`.
pub fn on<D, F>(mut deps: impl FnMut() -> D + 'static, mut f: F) -> EffectId
where
    D: Clone + PartialEq + 'static,
    F: FnMut(&D, Option<&D>) + 'static,
{
    let prev: Rc<RefCell<Option<D>>> = Rc::new(RefCell::new(None));
    create_effect(move || {
        let input = deps();
        let previous = prev.borrow().clone();
        untrack(|| {
            f(&input, previous.as_ref());
        });
        *prev.borrow_mut() = Some(input);
    })
}

/// Fine-grained equality selector (Solid `createSelector`).
///
/// `selector.is(key)` returns whether `source()` currently equals `key`. Subscribers of a given
/// key only re-run when that key's selected boolean flips (not on every unrelated source change).
pub struct Selector<T: 'static> {
    selected: Memo<T>,
    /// Owner that created the selector — per-key memos attach here so reader effects can re-run
    /// without disposing the cache.
    owner: OwnerId,
    cache: Rc<RefCell<HashMap<T, Memo<bool>>>>,
}

impl<T> Clone for Selector<T> {
    fn clone(&self) -> Self {
        Self {
            selected: self.selected,
            owner: self.owner,
            cache: Rc::clone(&self.cache),
        }
    }
}

impl<T: Clone + Eq + std::hash::Hash + 'static> Selector<T> {
    /// Whether `key` is the current selection. Tracks a per-key memo.
    pub fn is(&self, key: T) -> bool {
        let memo = {
            let mut cache = self.cache.borrow_mut();
            if let Some(m) = cache.get(&key) {
                *m
            } else {
                let selected = self.selected;
                let owner = self.owner;
                let key_for_memo = key.clone();
                let memo = untrack(|| {
                    RUNTIME.with(|rt_rc| {
                        let rt = rt_rc.borrow();
                        let prev_owner = *rt.current_owner.borrow();
                        let prev_effect = *rt.current_effect.borrow();
                        *rt.current_owner.borrow_mut() = Some(owner);
                        // Don't attach the memo-creating work to the reader effect.
                        *rt.current_effect.borrow_mut() = None;
                        let m = create_memo(move || selected.with(|s| s == &key_for_memo));
                        *rt.current_owner.borrow_mut() = prev_owner;
                        *rt.current_effect.borrow_mut() = prev_effect;
                        m
                    })
                });
                cache.insert(key, memo);
                memo
            }
        };
        memo.get()
    }
}

/// Create a [`Selector`] over `source` (Solid `createSelector`).
pub fn create_selector<T, F>(source: F) -> Selector<T>
where
    T: Clone + Eq + std::hash::Hash + PartialEq + 'static,
    F: FnMut() -> T + 'static,
{
    RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        let owner = ensure_owner(&rt);
        Selector {
            selected: create_memo(source),
            owner,
            cache: Rc::new(RefCell::new(HashMap::new())),
        }
    })
}

/// Deferred view of `source` (Solid `createDeferred` MVP).
///
/// Readers see the previous value until the current reactive flush finishes; then the deferred
/// signal catches up. This is a single-flush deferral (not idle/timeout scheduling).
pub fn create_deferred<T, F>(mut source: F) -> Signal<T>
where
    T: Clone + PartialEq + 'static,
    F: FnMut() -> T + 'static,
{
    let initial = untrack(&mut source);
    let deferred = create_signal(initial);
    create_effect(move || {
        let next = source();
        RUNTIME.with(|rt_rc| {
            let rt = rt_rc.borrow();
            enqueue_deferred_fn(
                &rt,
                Box::new(move || {
                    if deferred.peek() != next {
                        deferred.set(next);
                    }
                }),
            );
        });
    });
    deferred
}

pub use crate::style::{
    any_ui_animation_active, tick_animations, use_ui_transition_cubic, AnimationTick, CubicTween,
    CubicTweenState,
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
}
