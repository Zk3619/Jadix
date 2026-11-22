use super::*;
use jadix_core::presentation::keyboard::{xkb_keycode_to_hid, KeyEvent, KeyModifiers, KeyPhase};

pub(super) fn modifiers(value: gdk::ModifierType) -> KeyModifiers {
    KeyModifiers {
        shift: value.contains(gdk::ModifierType::SHIFT_MASK),
        control: value.contains(gdk::ModifierType::CONTROL_MASK),
        alt: value.contains(gdk::ModifierType::ALT_MASK),
        meta: value.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK),
        caps_lock: value.contains(gdk::ModifierType::LOCK_MASK),
    }
}

fn key_name(key: gdk::Key) -> Option<String> {
    let name = match key {
        gdk::Key::Left => "ArrowLeft",
        gdk::Key::Right => "ArrowRight",
        gdk::Key::Up => "ArrowUp",
        gdk::Key::Down => "ArrowDown",
        gdk::Key::Return | gdk::Key::KP_Enter => "Enter",
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => "Tab",
        gdk::Key::Escape => "Escape",
        gdk::Key::BackSpace => "Backspace",
        gdk::Key::Delete => "Delete",
        gdk::Key::Insert => "Insert",
        gdk::Key::Home => "Home",
        gdk::Key::End => "End",
        gdk::Key::Page_Up => "PageUp",
        gdk::Key::Page_Down => "PageDown",
        gdk::Key::Shift_L | gdk::Key::Shift_R => "Shift",
        gdk::Key::Control_L | gdk::Key::Control_R => "Control",
        gdk::Key::Alt_L | gdk::Key::Alt_R => "Alt",
        gdk::Key::Super_L | gdk::Key::Super_R | gdk::Key::Meta_L | gdk::Key::Meta_R => "Meta",
        gdk::Key::Caps_Lock => "CapsLock",
        gdk::Key::Num_Lock => "NumLock",
        gdk::Key::Scroll_Lock => "ScrollLock",
        gdk::Key::Pause => "Pause",
        gdk::Key::Print => "PrintScreen",
        gdk::Key::Menu => "ContextMenu",
        _ => {
            if let Some(character) = key.to_unicode().filter(|c| !c.is_control()) {
                return Some(character.to_string());
            }
            let name = key.name()?;
            return (name.starts_with('F')
                && name[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)))
            .then(|| name.to_string());
        }
    };
    Some(name.into())
}

pub(super) fn bind(widget: &gtk::Widget, events: Rc<Events>, control: Rc<Control>) {
    let pressed = Rc::new(RefCell::new(std::collections::BTreeSet::new()));
    let keys = gtk::EventControllerKey::new();
    let (queue, state, held) = (events.clone(), control.clone(), pressed.clone());
    keys.connect_key_pressed(move |_, key, hardware, flags| {
        let repeat = !held.borrow_mut().insert(hardware);
        let descriptor = state.descriptor.borrow();
        if !descriptor.accepts_keyboard() {
            return glib::Propagation::Proceed;
        }
        let modifiers = modifiers(flags);
        if let Some(key) = key_name(key) {
            if let Some(shortcut) = descriptor
                .keyboard
                .shortcuts
                .iter()
                .find(|s| s.key == key && s.modifiers == modifiers)
            {
                queue.push(
                    &state,
                    EventPayload::Shortcut {
                        id: shortcut.id.clone(),
                    },
                );
                return glib::Propagation::Stop;
            }
            if let Some(code) = xkb_keycode_to_hid(hardware) {
                let event = KeyEvent {
                    phase: KeyPhase::Down,
                    key,
                    code,
                    modifiers,
                    repeat,
                };
                if descriptor.keyboard.accepts(&event) {
                    queue.push(&state, EventPayload::Key(event));
                }
            }
        }
        if modifiers.alt || modifiers.meta {
            return glib::Propagation::Proceed;
        }
        let navigation = match key {
            gdk::Key::Left => NavigationKey::Left,
            gdk::Key::Right => NavigationKey::Right,
            gdk::Key::Up => NavigationKey::Up,
            gdk::Key::Down => NavigationKey::Down,
            gdk::Key::Home => NavigationKey::Home,
            gdk::Key::End => NavigationKey::End,
            gdk::Key::Page_Up => NavigationKey::PageUp,
            gdk::Key::Page_Down => NavigationKey::PageDown,
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab => NavigationKey::Tab,
            gdk::Key::Return | gdk::Key::KP_Enter => NavigationKey::Enter,
            gdk::Key::Escape => NavigationKey::Escape,
            _ => return glib::Propagation::Proceed,
        };
        if !descriptor.navigation_keys.contains(&navigation) {
            return glib::Propagation::Proceed;
        }
        queue.push(
            &state,
            EventPayload::Navigate {
                key: navigation,
                shift: modifiers.shift,
                control: modifiers.control,
            },
        );
        glib::Propagation::Stop
    });
    let (queue, state, held) = (events, control, pressed.clone());
    keys.connect_key_released(move |_, key, hardware, flags| {
        held.borrow_mut().remove(&hardware);
        let descriptor = state.descriptor.borrow();
        if !descriptor.accepts_keyboard() {
            return;
        }
        if let (Some(key), Some(code)) = (key_name(key), xkb_keycode_to_hid(hardware)) {
            let event = KeyEvent {
                phase: KeyPhase::Up,
                key,
                code,
                modifiers: modifiers(flags),
                repeat: false,
            };
            if descriptor.keyboard.accepts(&event) {
                queue.push(&state, EventPayload::Key(event));
            }
        }
    });
    widget.add_controller(keys);
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| pressed.borrow_mut().clear());
    widget.add_controller(focus);
}
