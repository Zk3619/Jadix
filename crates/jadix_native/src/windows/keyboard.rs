use super::*;
use crate::presentation::keyboard::{windows_scancode_to_hid, KeyEvent, KeyModifiers, KeyPhase};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardState, GetKeyState, ToUnicodeEx};

/// Observers do not consume native editing. Only a declared shortcut does.
pub(super) fn dispatch(context: &WindowContext, message: &MSG) -> bool {
    if !matches!(message.message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
        || context.compositions.borrow().contains_key(&(message.hwnd as usize))
        || unsafe { IsWindowEnabled(message.hwnd) == 0 || IsWindowVisible(message.hwnd) == 0 }
    {
        return false;
    }
    let Some(control) = context.target(message.hwnd) else { return false; };
    let policies = context.keyboard.borrow();
    let Some(policy) = policies.get(&control) else { return false; };
    let Some(event) = key_event(message) else { return false; };
    if event.phase == KeyPhase::Down {
        if let Some(shortcut) = policy.shortcuts.iter().find(|shortcut|
            shortcut.key == event.key && shortcut.modifiers == event.modifiers)
        {
            context.push(RawEvent::Shortcut { control, id: shortcut.id.clone() });
            return true;
        }
    }
    if policy.accepts(&event) {
        context.push(RawEvent::Key { control, event });
    }
    false
}

fn key_event(message: &MSG) -> Option<KeyEvent> {
    let scan = ((message.lParam as usize >> 16) & 255) as u8;
    let extended = message.lParam as usize & (1 << 24) != 0;
    let code = match message.wParam {
        0x13 => 72,
        0x7c..=0x87 => 104 + (message.wParam - 0x7c) as u16,
        _ => windows_scancode_to_hid(scan, extended)?,
    };
    let pressed = |key| unsafe { GetKeyState(key) < 0 };
    let modifiers = KeyModifiers {
        shift: pressed(0x10), control: pressed(0x11), alt: pressed(0x12),
        meta: pressed(0x5b) || pressed(0x5c), caps_lock: unsafe { GetKeyState(0x14) & 1 != 0 },
    };
    let special = match message.wParam {
        0x08 => "Backspace", 0x09 => "Tab", 0x0d => "Enter", 0x13 => "Pause",
        0x14 => "CapsLock", 0x1b => "Escape", 0x21 => "PageUp", 0x22 => "PageDown",
        0x23 => "End", 0x24 => "Home", 0x25 => "ArrowLeft", 0x26 => "ArrowUp",
        0x27 => "ArrowRight", 0x28 => "ArrowDown", 0x2c => "PrintScreen",
        0x2d => "Insert", 0x2e => "Delete", 0x10 | 0xa0 | 0xa1 => "Shift",
        0x11 | 0xa2 | 0xa3 => "Control", 0x12 | 0xa4 | 0xa5 => "Alt",
        0x5b | 0x5c => "Meta", 0x5d => "ContextMenu", 0x90 => "NumLock", 0x91 => "ScrollLock",
        _ => "",
    };
    let key = if !special.is_empty() { special.into() }
        else if (0x70..=0x87).contains(&message.wParam) { format!("F{}", message.wParam - 0x6f) }
        else {
            let mut state = [0u8; 256];
            if unsafe { GetKeyboardState(state.as_mut_ptr()) } == 0 { return None; }
            // Ctrl must not turn Ctrl+S into U+0013. Keep Ctrl+Alt for AltGr.
            if modifiers.control && !modifiers.alt { state[0x11] = 0; state[0xa2] = 0; state[0xa3] = 0; }
            let mut units = [0u16; 16];
            // Flag 4 leaves the dead-key buffer untouched (Windows 10+).
            let length = unsafe { ToUnicodeEx(message.wParam as u32, u32::from(scan), state.as_ptr(), units.as_mut_ptr(), units.len() as i32, 4, GetKeyboardLayout(0)) };
            if length <= 0 { return None; }
            String::from_utf16(&units[..usize::try_from(length).ok()?.min(units.len())]).ok()?
        };
    let event = KeyEvent { phase: if matches!(message.message, WM_KEYUP | WM_SYSKEYUP) { KeyPhase::Up } else { KeyPhase::Down },
        key, code, modifiers, repeat: matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) && message.lParam as usize & (1 << 30) != 0 };
    event.is_valid().then_some(event)
}
