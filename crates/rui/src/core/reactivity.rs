use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    static RUNTIME: RefCell<Rc<Runtime>> = RefCell::new(Runtime::new());
}

pub struct EffectState {
    pub effect_fn: Rc<RefCell<dyn FnMut()>>,
    pub dependencies: HashSet<SignalId>,
}

pub struct Runtime {
    pub current_effect: RefCell<Option<EffectId>>,
    pub signals: RefCell<HashMap<SignalId, Rc<RefCell<dyn Any>>>>,
    pub named_signals: RefCell<HashMap<String, SignalId>>,
    pub effects: RefCell<HashMap<EffectId, Rc<RefCell<dyn FnMut()>>>>,
    pub signal_subscribers: RefCell<HashMap<SignalId, HashSet<EffectId>>>,
}

impl Runtime {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            current_effect: RefCell::new(None),
            signals: RefCell::new(HashMap::new()),
            named_signals: RefCell::new(HashMap::new()),
            effects: RefCell::new(HashMap::new()),
            signal_subscribers: RefCell::new(HashMap::new()),
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

        for effect_id in effects_to_run {
            let effect_fn = RUNTIME.with(|rt_rc| rt_rc.borrow().effects.borrow().get(&effect_id).cloned());

            if let Some(effect_fn) = effect_fn {
                let prev_effect = RUNTIME.with(|rt_rc| {
                    let rt = rt_rc.borrow();
                    let mut curr = rt.current_effect.borrow_mut();
                    let prev = *curr;
                    *curr = Some(effect_id);
                    prev
                });

                (&mut *effect_fn.borrow_mut())();

                RUNTIME.with(|rt_rc| {
                    *rt_rc.borrow().current_effect.borrow_mut() = prev_effect;
                });
            }
        }
    }
}

pub fn create_effect<F>(f: F)
where
    F: FnMut() + 'static,
{
    let id = EffectId(NEXT_EFFECT_ID.fetch_add(1, Ordering::Relaxed));
    let effect_rc: Rc<RefCell<dyn FnMut()>> = Rc::new(RefCell::new(f));

    let prev_effect = RUNTIME.with(|rt_rc| {
        let rt = rt_rc.borrow();
        rt.effects.borrow_mut().insert(id, Rc::clone(&effect_rc));
        let mut curr = rt.current_effect.borrow_mut();
        let prev = *curr;
        *curr = Some(id);
        prev
    });

    (&mut *effect_rc.borrow_mut())();

    RUNTIME.with(|rt_rc| {
        *rt_rc.borrow().current_effect.borrow_mut() = prev_effect;
    });
}

pub fn create_signal<T: 'static>(initial_value: T) -> Signal<T> {
    RUNTIME.with(|rt_rc| rt_rc.borrow().create_signal(initial_value))
}

pub fn use_signal<T: 'static>(id_str: &str, init: impl FnOnce() -> T) -> Signal<T> {
    RUNTIME.with(|rt_rc| rt_rc.borrow().use_signal(id_str, init))
}

pub use crate::style::{tick_animations, use_spring, Spring, SpringState};
