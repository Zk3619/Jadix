//! 受控 XOR 非受控的通用信号解析（WP-SI.2；扩 WP-3.7 [`toggle_state`]）。
//!
//! - **受控**：调用方 `Signal` 是唯一写入点。
//! - **非受控**：当前 Owner 下 [`use_scoped_signal`]，键为 `{instance_id}__{slot}`。
//!
//! 布尔专用 API 仍在 [`crate::utils::toggle_state`]。

use crate::core::reactivity::{use_scoped_signal, Signal};
use crate::utils::toggle_state::bool_slot_key;

/// 受控 XOR 非受控的 `Signal<String>`。
pub fn resolve_string_signal(
    controlled: Option<Signal<String>>,
    instance_id: &str,
    slot: &str,
    default: impl Into<String>,
) -> Signal<String> {
    match controlled {
        Some(sig) => sig,
        None => {
            let key = bool_slot_key(instance_id, slot);
            let default = default.into();
            use_scoped_signal(&key, move || default)
        }
    }
}

/// 受控 XOR 非受控的 `Signal<f32>`。
pub fn resolve_f32_signal(
    controlled: Option<Signal<f32>>,
    instance_id: &str,
    slot: &str,
    default: f32,
) -> Signal<f32> {
    match controlled {
        Some(sig) => sig,
        None => {
            let key = bool_slot_key(instance_id, slot);
            use_scoped_signal(&key, move || default)
        }
    }
}

/// 开合态：`Signal<bool>`；槽名建议 `"open"`。
///
/// 与 [`crate::utils::toggle_state::resolve_bool_signal`] 同形，语义上标明「浮层/面板开合」。
pub fn resolve_open_signal(
    controlled: Option<Signal<bool>>,
    instance_id: &str,
    default: bool,
) -> Signal<bool> {
    crate::utils::toggle_state::resolve_bool_signal(controlled, instance_id, "open", default)
}

#[cfg(test)]
#[path = "tests/controlled.rs"]
mod tests;
