//! Toast 可暂停计时（产品层，随 toast 目录）。
//!
//! 暂停勿写回 queue（避免 hover 闪断）。冻结时剩余时长不低于 [`TOAST_PAUSE_FLOOR`]。

use std::cell::RefCell;
use std::collections::HashMap;
use web_time::{Duration, Instant};

/// 冻结时剩余时长的下限。正好在到期瞬间悬停，`saturating_sub` 会把剩余削到 0；
/// 收起后该卡会在指针刚移开时原地蒸发。留一点余量，让「收起→接续→退场」看得见。
pub const TOAST_PAUSE_FLOOR: Duration = Duration::from_millis(120);

#[derive(Clone, Debug)]
pub struct ToastTimer {
    pub remaining: Duration,
    pub running_since: Option<Instant>,
}

thread_local! {
    static TIMERS: RefCell<HashMap<u64, ToastTimer>> = RefCell::new(HashMap::new());
}

pub fn arm(id: u64, duration: Duration, running: bool) {
    TIMERS.with(|m| {
        m.borrow_mut().insert(
            id,
            ToastTimer {
                remaining: duration,
                running_since: running.then(Instant::now),
            },
        );
    });
}

pub fn clear(id: u64) {
    TIMERS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

pub fn pause(id: u64) {
    TIMERS.with(|m| {
        if let Some(t) = m.borrow_mut().get_mut(&id) {
            if let Some(since) = t.running_since.take() {
                t.remaining = t
                    .remaining
                    .saturating_sub(since.elapsed())
                    .max(TOAST_PAUSE_FLOOR);
            }
        }
    });
}

pub fn resume(id: u64) {
    TIMERS.with(|m| {
        if let Some(t) = m.borrow_mut().get_mut(&id) {
            // 勿因 `remaining` 已归零就拒绝接续：那样 `running_since` 永远是 None，
            // `expired` 恒 false，这条通知就再也不会自动退场了。
            if t.running_since.is_none() {
                t.running_since = Some(Instant::now());
            }
        }
    });
}

/// 到期但退场预算已满：顺延这条的到期时刻。
pub fn defer(id: u64, delay: Duration, running: bool) {
    TIMERS.with(|m| {
        if let Some(t) = m.borrow_mut().get_mut(&id) {
            t.remaining = delay;
            t.running_since = running.then(Instant::now);
        }
    });
}

pub fn expired(id: u64) -> bool {
    TIMERS.with(|m| {
        let map = m.borrow();
        let Some(t) = map.get(&id) else {
            return false;
        };
        match t.running_since {
            Some(since) => since.elapsed() >= t.remaining,
            None => false,
        }
    })
}

pub fn deadline(id: u64) -> Option<Instant> {
    TIMERS.with(|m| {
        let map = m.borrow();
        let t = map.get(&id)?;
        let since = t.running_since?;
        Some(since + t.remaining)
    })
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/timer.rs"]
mod tests;
