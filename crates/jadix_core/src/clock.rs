//! UI-thread clock and owner-bound, reusable one-shot timers.
//! Hosts sleep until `next_wake_at`; tests advance a manual clock and call `flush`.
use std::{cell::{Cell, RefCell}, collections::BTreeMap, rc::{Rc, Weak}};
use web_time::{Duration, Instant};
use crate::reactivity::{current_owner, on_cleanup, OwnerId};

#[derive(Clone)]
pub struct Clock(Rc<dyn Fn() -> Instant>);
impl Clock {
    pub fn new(sample: impl Fn() -> Instant + 'static) -> Self { Self(Rc::new(sample)) }
    pub fn now(&self) -> Instant { (self.0)() }
}
impl Default for Clock { fn default() -> Self { Self::new(Instant::now) } }

#[derive(Clone)]
pub struct ManualClock(Rc<Cell<Instant>>);
impl Default for ManualClock { fn default() -> Self { Self::new(Instant::now()) } }
impl ManualClock {
    pub fn new(origin: Instant) -> Self { Self(Rc::new(Cell::new(origin))) }
    pub fn clock(&self) -> Clock { let value = self.0.clone(); Clock::new(move || value.get()) }
    pub fn advance(&self, elapsed: Duration) -> Result<(), &'static str> {
        let next = self.0.get().checked_add(elapsed).ok_or("clock overflow")?;
        self.0.set(next); Ok(())
    }
}
thread_local! {
    static CLOCK: RefCell<Clock> = RefCell::new(Clock::default());
    static TIMERS: RefCell<BTreeMap<u64, Weak<TimerSlot>>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
    static POLLING: Cell<bool> = const { Cell::new(false) };
}
pub fn now() -> Instant { let clock = CLOCK.with(|slot| slot.borrow().clone()); clock.now() }

/// A scope restores the previous clock even when its body unwinds. Timers must be
/// armed and pumped in the same clock domain; platform media clocks stay native.
pub fn with_clock<R>(clock: Clock, body: impl FnOnce() -> R) -> R {
    struct Restore(Option<Clock>);
    impl Drop for Restore { fn drop(&mut self) { if let Some(previous) = self.0.take() { CLOCK.with(|slot| slot.replace(previous)); } } }
    let _restore = Restore(Some(CLOCK.with(|slot| slot.replace(clock))));
    body()
}
struct Pending { at: Instant, generation: u64, callback: Box<dyn FnOnce()> }
struct TimerSlot { owner: OwnerId, alive: Cell<bool>, generation: Cell<u64>, pending: RefCell<Option<Pending>> }

/// Clones share one slot. Dropping the last handle or disposing its owner cancels
/// pending work. Rearming reuses both the slot and its single owner cleanup.
#[derive(Clone)]
pub struct Timer(Rc<TimerSlot>);
impl Timer {
    pub fn new() -> Result<Self, &'static str> {
        let owner = current_owner().ok_or("timer requires a reactive owner")?;
        let slot = Rc::new(TimerSlot { owner, alive: Cell::new(true), generation: Cell::new(0), pending: RefCell::new(None) });
        TIMERS.with(|timers| {
            let mut timers = timers.borrow_mut(); timers.retain(|_, slot| slot.upgrade().is_some_and(|slot| slot.alive.get()));
            if timers.len() >= 1024 { return Err("timer budget exceeded"); }
            let id = NEXT_ID.with(|next| -> Result<_, &'static str> { let id = next.get().checked_add(1).ok_or("timer identity exhausted")?; next.set(id); Ok(id) })?;
            timers.insert(id, Rc::downgrade(&slot)); Ok(())
        })?;
        let weak = Rc::downgrade(&slot);
        on_cleanup(move || { if let Some(slot) = weak.upgrade() { slot.alive.set(false); slot.pending.borrow_mut().take(); } });
        Ok(Self(slot))
    }
    pub fn arm(&self, delay: Duration, callback: impl FnOnce() + 'static) -> Result<(), &'static str> {
        if !self.0.alive.get() { return Err("timer owner disposed"); }
        let at = now().checked_add(delay).ok_or("timer deadline overflow")?;
        let generation = self.0.generation.get().checked_add(1).ok_or("timer generation exhausted")?;
        self.0.generation.set(generation);
        self.0.pending.replace(Some(Pending { at, generation, callback: Box::new(callback) })); Ok(())
    }
    pub fn cancel(&self) { self.0.pending.borrow_mut().take(); }
    pub fn remaining(&self) -> Option<Duration> { self.0.pending.borrow().as_ref().map(|pending| pending.at.saturating_duration_since(now())) }
}

pub fn next_wake_at() -> Option<Instant> {
    TIMERS.with(|timers| {
        let mut timers = timers.borrow_mut();
        timers.retain(|_, slot| slot.upgrade().is_some_and(|slot| slot.alive.get()));
        timers.values().filter_map(Weak::upgrade).filter_map(|slot| slot.pending.borrow().as_ref().map(|pending| pending.at)).min()
    })
}
/// Run the due set once, ordered by deadline then creation order. Work rearmed
/// by a callback waits for the next turn, including zero-duration timers.
pub fn tick() -> usize {
    if POLLING.replace(true) { return 0; }
    struct Restore;
    impl Drop for Restore { fn drop(&mut self) { POLLING.set(false); } }
    let _restore = Restore;
    let sample = now();
    let mut due = TIMERS.with(|timers| timers.borrow().iter().filter_map(|(id, weak)| {
        let slot = weak.upgrade()?;
        let pending = slot.pending.borrow();
        let pending = pending.as_ref()?;
        (slot.alive.get() && pending.at <= sample).then_some((pending.at, *id, pending.generation, weak.clone()))
    }).collect::<Vec<_>>());
    due.sort_by_key(|(at, id, _, _)| (*at, *id));
    let mut fired = 0;
    for (_, _, generation, weak) in due {
        let Some(slot) = weak.upgrade() else { continue; };
        if !slot.alive.get() { continue; }
        // Earlier callbacks may cancel or postpone another due slot.
        if !slot.pending.borrow().as_ref().is_some_and(|pending| pending.generation == generation) { continue; }
        let pending = slot.pending.borrow_mut().take();
        if let Some(pending) = pending { if crate::error_boundary::run_owned_callback(slot.owner, pending.callback).is_some() { fired += 1; } }
    }
    fired
}
pub fn next_wake_delay() -> Option<Duration> {
    next_wake_at().into_iter().chain(crate::hover_intent::hover_intent_next_wake_at()).min().map(|at| at.saturating_duration_since(now()))
}
/// Settle one timer turn, shared hover deadlines and queued reactive work.
pub fn flush() -> usize {
    let count = tick() + usize::from(crate::hover_intent::tick_hover_intent());
    crate::reactivity::flush(); count
}
