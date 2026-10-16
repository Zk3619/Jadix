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
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, use_signal};

    #[test]
    fn controlled_is_the_only_write_path() {
        let caller = use_signal("__jadix_wp37_ctrl", || false);
        let resolved = resolve_bool_signal(Some(caller), "same", SLOT_CHECKBOX, true);
        assert!(
            !resolved.peek(),
            "controlled must ignore default; caller starts false"
        );
        write_bool_toggle(resolved, false);
        assert!(caller.peek());
    }

    #[test]
    fn uncontrolled_is_owner_scoped() {
        let key = bool_slot_key("same_local", SLOT_TOGGLE);
        let (h1, a) = create_root(|| {
            let sig = resolve_bool_signal(None, "same_local", SLOT_TOGGLE, false);
            sig.set(true);
            use_scoped_signal(&key, || false).peek()
        });
        assert!(a);

        let (h2, b) =
            create_root(|| resolve_bool_signal(None, "same_local", SLOT_TOGGLE, false).peek());
        assert!(!b, "fresh owner must not see the other root's store");

        h1.dispose();
        h2.dispose();
    }

    #[test]
    fn disabled_click_does_not_write_or_focus() {
        let caller = use_signal("__jadix_wp37_dis", || false);
        use_signal("__jadix_button_focus_id", String::new).set(String::new());
        let mut click = bool_on_click(caller, true, "focus-me".into());
        click();
        assert!(!caller.peek());
        assert_eq!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            ""
        );
    }

    #[test]
    fn enabled_click_toggles_and_sets_focus() {
        let caller = use_signal("__jadix_wp37_en", || false);
        let mut click = bool_on_click(caller, false, "focus-me".into());
        click();
        assert!(caller.peek());
        assert_eq!(
            use_signal("__jadix_button_focus_id", String::new).peek(),
            "focus-me"
        );
    }

    #[test]
    fn slots_do_not_collide_under_same_instance_id() {
        let cb = resolve_bool_signal(None, "foo", SLOT_CHECKBOX, false);
        let sw = resolve_bool_signal(None, "foo", SLOT_SWITCH, true);
        assert!(!cb.peek());
        assert!(sw.peek());
        cb.set(true);
        assert!(
            sw.peek() && cb.peek(),
            "independent stores; switch default stays true"
        );
    }
}
