//! Presence / ExitGate — 退场播完再卸树（自定义组件原语 · Wave Prim）。
//!
//! 目标见仓库 `docs/components/custom_components_suggestions.md`：复杂浮层不应各自手搓
//! `exiting` + 墙钟；先消费本模块，再写产品壳。
//!
//! # 心智
//!
//! - **Present**：希望显示，已挂载
//! - **Exiting**：希望隐藏，但仍挂载以播退场（CSS transition / 手势 mid）
//! - **Absent**：退场结束，可卸树
//!
//! Toast 等队列组件可只吃 [`exit_elapsed`] / [`ExitGate`] 状态机；带 UI 的
//! [`presence`] 包装器在 reconcile / portal 同 key 契约对齐后暴露（WP-PRIM.5）。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::reactive::reactive_view;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::utils::panel_presence::{
    sync_panel_presence, tick_panel_cells, use_panel_presence, PanelPresence, PanelPresenceCell,
    SyncPolicy,
};

/// 挂载相位。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresencePhase {
    Present,
    Exiting,
    Absent,
}

impl PresencePhase {
    /// Present 或 Exiting 时仍应保留在树上。
    #[inline]
    pub fn is_mounted(self) -> bool {
        matches!(self, Self::Present | Self::Exiting)
    }
}

/// 单实例退场门：`want` 变 false → Exiting →（时长到）Absent。
#[derive(Clone, Debug)]
pub struct ExitGate {
    phase: PresencePhase,
    exit_started: Option<Instant>,
    exit_duration: Duration,
}

impl ExitGate {
    pub fn new(present: bool, exit_duration: Duration) -> Self {
        Self {
            phase: if present {
                PresencePhase::Present
            } else {
                PresencePhase::Absent
            },
            exit_started: None,
            exit_duration,
        }
    }

    #[inline]
    pub fn phase(&self) -> PresencePhase {
        self.phase
    }

    #[inline]
    pub fn is_mounted(&self) -> bool {
        self.phase.is_mounted()
    }

    #[inline]
    pub fn exit_duration(&self) -> Duration {
        self.exit_duration
    }

    #[inline]
    pub fn exit_started(&self) -> Option<Instant> {
        self.exit_started
    }

    /// 设置希望的可见性。返回相位是否变化。
    pub fn set_present(&mut self, want: bool, now: Instant) -> bool {
        match (want, self.phase) {
            (true, PresencePhase::Present) => false,
            (true, PresencePhase::Exiting | PresencePhase::Absent) => {
                self.phase = PresencePhase::Present;
                self.exit_started = None;
                true
            }
            (false, PresencePhase::Absent) => false,
            (false, PresencePhase::Exiting) => false,
            (false, PresencePhase::Present) => {
                if cfg!(feature = "animation-base") {
                    self.phase = PresencePhase::Exiting;
                    self.exit_started = Some(now);
                } else {
                    self.phase = PresencePhase::Absent;
                    self.exit_started = None;
                }
                true
            }
        }
    }

    /// 推进墙钟；Exiting 超时 → Absent。返回是否刚变为 Absent。
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.phase != PresencePhase::Exiting {
            return false;
        }
        let Some(started) = self.exit_started else {
            return false;
        };
        if now.duration_since(started) < self.exit_duration {
            return false;
        }
        self.phase = PresencePhase::Absent;
        self.exit_started = None;
        true
    }

    /// 立刻卸树（跳过退场）。
    pub fn force_absent(&mut self) {
        self.phase = PresencePhase::Absent;
        self.exit_started = None;
    }
}

/// 墙钟是否已超过退场时长（队列式组件常用，无需整颗 [`ExitGate`]）。
#[inline]
pub fn exit_elapsed(started: Instant, duration: Duration) -> bool {
    !cfg!(feature = "animation-base") || started.elapsed() >= duration
}

/// 与 [`exit_elapsed`] 相同，但用显式 `now`（便于测）。
#[inline]
pub fn exit_elapsed_at(started: Instant, duration: Duration, now: Instant) -> bool {
    !cfg!(feature = "animation-base") || now.duration_since(started) >= duration
}

/// Stable `template_id` suffix for [`presence`] islands (`{id}__presence`).
pub const PRESENCE_SUFFIX: &str = "__presence";

/// `{scope_id}__presence` — reconcile / keyed motion hook target.
pub fn presence_template_id(scope_id: &str) -> String {
    format!("{scope_id}{PRESENCE_SUFFIX}")
}

pub fn is_presence_wrapper_tid(tid: Option<&str>) -> bool {
    tid.is_some_and(|t| t.ends_with(PRESENCE_SUFFIX))
}

thread_local! {
    static GENERIC_PRESENCE_REGISTRY: RefCell<Vec<Rc<RefCell<PanelPresenceCell<()>>>>> =
        const { RefCell::new(Vec::new()) };
}

/// Shared ExitGate cell for generic show/hide wrappers (not collapse / floating / modal).
pub type GenericPresenceHandle = PanelPresence<()>;

pub fn use_generic_presence(scope_id: &str, exit_ms: u64) -> GenericPresenceHandle {
    use_panel_presence(
        &format!("__jadix_generic_presence:{scope_id}"),
        &format!("__jadix_generic_mounted:{scope_id}"),
        Duration::from_millis(exit_ms.max(1)),
        || (),
        |rc| {
            GENERIC_PRESENCE_REGISTRY.with(|r| {
                let mut v = r.borrow_mut();
                if !v.iter().any(|c| Rc::ptr_eq(c, &rc)) {
                    v.push(rc);
                }
            });
        },
        |rc| {
            GENERIC_PRESENCE_REGISTRY.with(|r| {
                r.borrow_mut().retain(|c| !Rc::ptr_eq(c, &rc));
            });
        },
    )
}

pub fn sync_generic_presence(handle: &GenericPresenceHandle, want: bool) -> PresencePhase {
    sync_panel_presence(
        handle,
        want,
        SyncPolicy {
            skip_first_enter: false,
            silent_mounted: false,
            mutex_kick: false,
        },
        |_, _| {},
    )
}

/// UI-thread tick: Exiting → Absent, clears mounted. Returns true when a redraw is needed.
pub fn tick_generic_presence() -> bool {
    let now = Instant::now();
    let outcome = GENERIC_PRESENCE_REGISTRY
        .with(|r| tick_panel_cells(&r.borrow(), now, |_| false, |_| {}, false));
    // Set mounted after dropping registry/cell borrows — `set` flushes readers
    // (`presence` subscribes to `mounted`) which re-enter `sync_generic_presence`.
    for sig in &outcome.unmount {
        if sig.is_alive() {
            sig.set(false);
        }
    }
    outcome.needs_frame || !outcome.unmount.is_empty()
}

pub fn generic_presence_next_wake_at() -> Option<Instant> {
    GENERIC_PRESENCE_REGISTRY.with(|r| {
        let mut next = None::<Instant>;
        for cell in r.borrow().iter() {
            let c = cell.borrow();
            if c.gate.phase() != PresencePhase::Exiting {
                continue;
            }
            let Some(started) = c.gate.exit_started() else {
                continue;
            };
            let end = started + c.gate.exit_duration();
            next = Some(next.map_or(end, |t| t.min(end)));
        }
        next
    })
}

/// Element wrapper: `show=false` keeps the subtree mounted until [`exit_ms`] elapses.
pub fn presence(
    scope_id: impl Into<String>,
    show: Signal<bool>,
    exit_ms: u64,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let scope_id = scope_id.into();
    let handle = use_generic_presence(&scope_id, exit_ms);
    let view_id = presence_template_id(&scope_id);
    reactive_view(view_id, move || {
        let phase = sync_generic_presence(&handle, show.get());
        // Tick clears `mounted` after exit_ms; subscribe so this island rebuilds to Absent.
        let mounted = handle.mounted().get();
        if phase == PresencePhase::Absent || !mounted {
            return Element::new();
        }
        body()
    })
}

#[cfg(test)]
#[path = "tests/presence.rs"]
mod tests;
