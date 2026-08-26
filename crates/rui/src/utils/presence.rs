//! Presence / ExitGate — 退场播完再卸树（自定义组件原语 · Wave Prim）。
//!
//! 目标见仓库 `docs/custom_components_suggestions.md`：复杂浮层不应各自手搓
//! `exiting` + 墙钟；先消费本模块，再写产品壳。
//!
//! # 心智
//!
//! - **Present**：希望显示，已挂载
//! - **Exiting**：希望隐藏，但仍挂载以播退场（CSS transition / 手势 mid）
//! - **Absent**：退场结束，可卸树
//!
//! Toast 等队列组件可只吃 [`exit_elapsed`] / [`ExitGate`] 状态机；带 UI 的
//! `presence(...)` 包装器跟进（需与 reconcile / portal 同 key 契约对齐后再暴露）。

use std::time::{Duration, Instant};

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
                self.phase = PresencePhase::Exiting;
                self.exit_started = Some(now);
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
    started.elapsed() >= duration
}

/// 与 [`exit_elapsed`] 相同，但用显式 `now`（便于测）。
#[inline]
pub fn exit_elapsed_at(started: Instant, duration: Duration, now: Instant) -> bool {
    now.duration_since(started) >= duration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_gate_present_exit_absent() {
        let t0 = Instant::now();
        let mut g = ExitGate::new(true, Duration::from_millis(100));
        assert_eq!(g.phase(), PresencePhase::Present);
        assert!(g.set_present(false, t0));
        assert_eq!(g.phase(), PresencePhase::Exiting);
        assert!(!g.tick(t0 + Duration::from_millis(50)));
        assert!(g.is_mounted());
        assert!(g.tick(t0 + Duration::from_millis(100)));
        assert_eq!(g.phase(), PresencePhase::Absent);
        assert!(!g.is_mounted());
    }

    #[test]
    fn exit_gate_reenter_cancels_exit() {
        let t0 = Instant::now();
        let mut g = ExitGate::new(true, Duration::from_millis(200));
        g.set_present(false, t0);
        assert!(g.set_present(true, t0 + Duration::from_millis(10)));
        assert_eq!(g.phase(), PresencePhase::Present);
        assert!(g.exit_started().is_none());
    }

    #[test]
    fn exit_elapsed_helpers() {
        let t0 = Instant::now();
        assert!(!exit_elapsed_at(t0, Duration::from_millis(50), t0));
        assert!(exit_elapsed_at(
            t0,
            Duration::from_millis(50),
            t0 + Duration::from_millis(50)
        ));
    }
}
