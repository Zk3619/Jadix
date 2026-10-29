//! Shared ExitGate + TLS cell for collapse / floating / modal.
//!
//! Domain extras live in [`PanelPresenceCell`]'s `Deref` target. Sync / tick
//! live here so those engines do not copy a fourth registry.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::core::reactivity::{current_owner, on_cleanup, use_signal, Signal};
use crate::utils::presence::{ExitGate, PresencePhase};

thread_local! {
    /// `(cell ptr, owner)` that already has an unregister cleanup. Named `use_signal`
    /// outlives a child owner; without this, parent-overlay rebuilds unregister the
    /// cell and the next `use_*` returns the same Rc without re-binding the registry.
    static REGISTRY_CLEANUP_OWNERS: RefCell<HashSet<(usize, crate::core::reactivity::OwnerId)>> =
        RefCell::new(HashSet::new());
}

#[derive(Clone, Copy, Debug)]
pub struct SyncPolicy {
    /// Collapse: first Present after create skips enter keyframes.
    pub skip_first_enter: bool,
    /// Modal: `mounted.set_silent` so opening does not rebuild the view twice.
    pub silent_mounted: bool,
    /// Floating: exclusive dismiss already cleared `mounted` while gate is Exiting.
    pub mutex_kick: bool,
}

impl SyncPolicy {
    pub const COLLAPSE: Self = Self {
        skip_first_enter: true,
        silent_mounted: false,
        mutex_kick: false,
    };
    pub const FLOATING: Self = Self {
        skip_first_enter: false,
        silent_mounted: false,
        mutex_kick: true,
    };
    pub const MODAL: Self = Self {
        skip_first_enter: false,
        silent_mounted: true,
        mutex_kick: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncEvent {
    Enter,
    ExitArm,
    MutexAbsent,
}

pub struct PanelPresenceCell<E> {
    pub mounted: Signal<bool>,
    pub gate: ExitGate,
    pub needs_enter: bool,
    pub enter_active: bool,
    pub exit_armed: bool,
    pub needs_exit_anim: bool,
    pub initialized: bool,
    extra: E,
}

impl<E> Deref for PanelPresenceCell<E> {
    type Target = E;
    fn deref(&self) -> &E {
        &self.extra
    }
}

impl<E> DerefMut for PanelPresenceCell<E> {
    fn deref_mut(&mut self) -> &mut E {
        &mut self.extra
    }
}

impl<E> PanelPresenceCell<E> {
    pub fn new(mounted: Signal<bool>, exit_duration: Duration, extra: E) -> Self {
        Self {
            mounted,
            gate: ExitGate::new(false, exit_duration),
            needs_enter: false,
            enter_active: false,
            exit_armed: false,
            needs_exit_anim: false,
            initialized: false,
            extra,
        }
    }

    pub fn extra(&self) -> &E {
        &self.extra
    }

    pub fn extra_mut(&mut self) -> &mut E {
        &mut self.extra
    }
}

pub struct PanelPresence<E> {
    pub cell: Rc<RefCell<PanelPresenceCell<E>>>,
}

impl<E> Clone for PanelPresence<E> {
    fn clone(&self) -> Self {
        Self {
            cell: Rc::clone(&self.cell),
        }
    }
}

impl<E> PanelPresence<E> {
    pub fn mounted(&self) -> Signal<bool> {
        self.cell.borrow().mounted
    }

    pub fn phase(&self) -> PresencePhase {
        self.cell.borrow().gate.phase()
    }

    pub fn needs_enter(&self) -> bool {
        self.cell.borrow().needs_enter
    }

    pub fn exit_armed(&self) -> bool {
        self.cell.borrow().exit_armed
    }

    pub fn begin_frame_flags(&self, open_visual: bool, exiting: bool) -> (bool, bool) {
        let mut c = self.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let needs_exit_anim = c.needs_exit_anim;
        if needs_enter && open_visual {
            c.enter_active = true;
        }
        if needs_exit_anim && exiting {
            c.needs_exit_anim = false;
        }
        (needs_enter, needs_exit_anim)
    }

    pub fn clear_needs_enter(&self) {
        self.cell.borrow_mut().needs_enter = false;
    }

    pub fn clear_enter_active(&self) {
        self.cell.borrow_mut().enter_active = false;
    }

    pub fn force_absent_flags(&self) {
        let mut c = self.cell.borrow_mut();
        c.gate.force_absent();
        c.exit_armed = false;
        c.needs_exit_anim = false;
        c.enter_active = false;
        c.needs_enter = false;
    }
}

pub fn use_panel_presence<E: 'static>(
    cell_key: &str,
    mounted_key: &str,
    exit_duration: Duration,
    extra: impl FnOnce() -> E,
    register: impl FnOnce(Rc<RefCell<PanelPresenceCell<E>>>),
    unregister: impl FnOnce(Rc<RefCell<PanelPresenceCell<E>>>) + 'static,
) -> PanelPresence<E> {
    let cell = use_signal(cell_key, || {
        let mounted = use_signal(mounted_key, || false);
        Rc::new(RefCell::new(PanelPresenceCell::new(
            mounted,
            exit_duration,
            extra(),
        )))
    });
    let rc = cell.try_peek().expect("panel presence cell");
    // `use_signal` is process-global. A nested Select's child owner can dispose
    // (ColorPicker overlay rebuild) and run on_cleanup unregister while the named
    // cell survives. Re-bind every call so flush still sees the live panel.
    register(Rc::clone(&rc));
    if let Some(owner) = current_owner() {
        let ptr = Rc::as_ptr(&rc) as usize;
        let fresh = REGISTRY_CLEANUP_OWNERS.with(|s| s.borrow_mut().insert((ptr, owner)));
        if fresh {
            let rc_cleanup = Rc::clone(&rc);
            on_cleanup(move || {
                REGISTRY_CLEANUP_OWNERS.with(|s| {
                    s.borrow_mut().remove(&(ptr, owner));
                });
                unregister(rc_cleanup);
            });
        }
    }
    PanelPresence { cell: rc }
}

pub fn sync_panel_presence<E>(
    handle: &PanelPresence<E>,
    want: bool,
    policy: SyncPolicy,
    mut on_event: impl FnMut(SyncEvent, &mut PanelPresenceCell<E>),
) -> PresencePhase {
    let mounted_sig = handle.mounted();
    let (phase, mounted, set_mounted) = {
        let mut c = handle.cell.borrow_mut();
        if policy.mutex_kick
            && !want
            && c.mounted.try_peek() == Some(false)
            && c.gate.phase() == PresencePhase::Exiting
        {
            c.gate.force_absent();
            c.exit_armed = false;
            c.needs_exit_anim = false;
            c.enter_active = false;
            c.needs_enter = false;
            on_event(SyncEvent::MutexAbsent, &mut c);
            return PresencePhase::Absent;
        }
        let was = c.gate.phase();
        let changed = c.gate.set_present(want, Instant::now());
        let phase = c.gate.phase();
        let mounted = c.gate.is_mounted();
        if want
            && matches!(phase, PresencePhase::Present)
            && (changed || was == PresencePhase::Absent)
        {
            let skip = policy.skip_first_enter && !c.initialized;
            c.needs_enter = !skip;
            c.enter_active = false;
            c.exit_armed = false;
            c.needs_exit_anim = false;
            on_event(SyncEvent::Enter, &mut c);
        }
        c.initialized = true;
        if matches!(phase, PresencePhase::Exiting) && !c.exit_armed {
            c.exit_armed = true;
            c.needs_enter = false;
            c.enter_active = false;
            c.needs_exit_anim = true;
            on_event(SyncEvent::ExitArm, &mut c);
        }
        let set_mounted = if c.mounted.is_alive() {
            c.mounted
                .try_peek()
                .map(|was| was != mounted)
                .unwrap_or(false)
        } else {
            false
        };
        (phase, mounted, set_mounted)
    };
    if set_mounted {
        if policy.silent_mounted {
            mounted_sig.set_silent(mounted);
        } else {
            mounted_sig.set(mounted);
        }
    }
    phase
}

pub struct TickOutcome {
    pub needs_frame: bool,
    pub unmount: Vec<Signal<bool>>,
}

pub fn tick_panel_cells<E>(
    cells: &[Rc<RefCell<PanelPresenceCell<E>>>],
    now: Instant,
    mut hold_exit: impl FnMut(&PanelPresenceCell<E>) -> bool,
    mut on_unmount: impl FnMut(&mut PanelPresenceCell<E>),
    hold_while_needs_enter: bool,
) -> TickOutcome {
    let mut needs_frame = false;
    let mut unmount = Vec::new();
    for cell in cells {
        let maybe = {
            let mut c = cell.borrow_mut();
            if c.gate.phase() == PresencePhase::Exiting {
                if hold_exit(&c) {
                    needs_frame = true;
                    None
                } else if c.gate.tick(now) {
                    c.needs_enter = false;
                    c.enter_active = false;
                    c.exit_armed = false;
                    c.needs_exit_anim = false;
                    on_unmount(&mut c);
                    Some(c.mounted)
                } else {
                    needs_frame = true;
                    None
                }
            } else if c.gate.phase() == PresencePhase::Present
                && (c.enter_active || (hold_while_needs_enter && c.needs_enter))
            {
                needs_frame = true;
                None
            } else {
                None
            }
        };
        if let Some(sig) = maybe {
            unmount.push(sig);
        }
    }
    TickOutcome {
        needs_frame,
        unmount,
    }
}
