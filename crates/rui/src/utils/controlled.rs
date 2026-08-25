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
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, use_scoped_signal, use_signal};

    #[test]
    fn string_controlled_ignores_default() {
        let caller = use_signal("__rui_si2_str", || "a".to_string());
        let resolved = resolve_string_signal(Some(caller), "same", "tabs", "b");
        assert_eq!(resolved.peek(), "a");
        resolved.set("c".into());
        assert_eq!(caller.peek(), "c");
    }

    #[test]
    fn string_uncontrolled_is_owner_scoped() {
        let (_o, a) = create_root(|| {
            resolve_string_signal(None, "inst", "tabs", "x").peek()
        });
        assert_eq!(a, "x");
        let (_o2, b) = create_root(|| {
            let key = bool_slot_key("inst", "tabs");
            use_scoped_signal(&key, || "y".to_string()).peek()
        });
        // Separate roots → separate Owner stores; both may init independently.
        assert_eq!(b, "y");
    }

    #[test]
    fn f32_and_open_resolve() {
        let v = resolve_f32_signal(None, "sl", "value", 0.5);
        assert!((v.peek() - 0.5).abs() < f32::EPSILON);
        let open = resolve_open_signal(None, "dp", false);
        assert!(!open.peek());
        open.set(true);
        assert!(open.peek());
    }
}
