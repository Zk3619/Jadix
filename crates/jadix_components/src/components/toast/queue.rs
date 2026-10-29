//! Toast 队列溢出 / 退场预算常量（产品层）。
//!
//! `displace` 退场勿 snap 正在播的非挤出退场；顺延用 [`TOAST_EXIT_GAP`]。

use std::time::Duration;

/// 顺延到期时留出的间隙：目标是落在上一段退场被 finalize 摘掉之后，
/// 否则同一帧里预算仍是满的，会被反复顺延。
pub const TOAST_EXIT_GAP: Duration = Duration::from_millis(32);

/// 同时做完整退场动画的上限（非挤出：手动关闭/到期）。挤出淡出另计。
pub const MAX_CONCURRENT_EXITING: usize = 1;

/// 非挤出退场是否已占满预算。
pub fn non_displace_budget_full(non_displace_exiting: usize) -> bool {
    non_displace_exiting >= MAX_CONCURRENT_EXITING
}

/// 挤出淡出上限（与可见 limit 联动）；至少 2。
pub fn max_displace_fades(visible_limit: usize) -> usize {
    visible_limit.saturating_add(1).max(2)
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/queue.rs"]
mod tests;
