//! Toggle / Checkbox / Switch 共用的布尔状态原语（WP-3.7）。
//!
//! - **受控**：调用方 `Signal<bool>` 是唯一写入点，组件不得再建第二份 store。
//! - **非受控**：当前 Owner 下 [`use_scoped_signal`]（WP-2.8 / 3.14），不再用全局 `{id}__…`。
//! - **disabled**：激活路径不写入、不抢焦点。
//!
//! Chrome（勾选字形、拇指位移、token 色）留在 `components/`。本模块禁止写圆角/配色。

use crate::core::reactivity::{use_scoped_signal, use_signal, Signal};

/// Toggle 非受控槽（与 Checkbox/Switch 同实例键时互不撞 store）。
pub const SLOT_TOGGLE: &str = "toggle";
/// Checkbox 非受控槽。
pub const SLOT_CHECKBOX: &str = "checkbox";
/// Switch 非受控槽。
pub const SLOT_SWITCH: &str = "switch";

/// Owner 作用域局部键：`{instance_id}__{slot}`。
pub fn bool_slot_key(instance_id: &str, slot: &str) -> String {
    format!("{instance_id}__{slot}")
}

/// 受控 XOR 非受控的单一布尔信号。
///
/// `controlled = Some` 时原样返回，**不**再 `use_scoped_signal`。
/// 否则按 [`bool_slot_key`] 在当前 Owner 下取/建信号，初值为 `default`。
pub fn resolve_bool_signal(
    controlled: Option<Signal<bool>>,
    instance_id: &str,
    slot: &str,
    default: bool,
) -> Signal<bool> {
    match controlled {
        Some(sig) => sig,
        None => {
            let key = bool_slot_key(instance_id, slot);
            use_scoped_signal(&key, move || default)
        }
    }
}

/// 翻转 `signal`；`disabled` 时为 no-op。用 [`Signal::peek`] 以免点击回调误订订阅。
pub fn write_bool_toggle(signal: Signal<bool>, disabled: bool) {
    if disabled || !signal.is_alive() {
        return;
    }
    signal.set(!signal.peek());
}

/// 激活：写入焦点键并翻转。`disabled` 时两者都不做。
pub fn bool_on_click(
    signal: Signal<bool>,
    disabled: bool,
    focus_id: String,
) -> impl FnMut() + 'static {
    move || {
        if disabled {
            return;
        }
        use_signal("__jadix_button_focus_id", String::new).set(focus_id.clone());
        write_bool_toggle(signal, false);
    }
}

#[cfg(test)]
#[path = "tests/toggle_state.rs"]
mod tests;
