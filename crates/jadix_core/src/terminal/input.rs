//! Xterm key encoding shared by native terminal adapters.
pub fn special_key(key: &str, shift: bool, alt: bool, control: bool, application_cursor: bool) -> Option<String> {
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
    let final_byte = match key {
        "up" => Some('A'), "down" => Some('B'), "right" => Some('C'),
        "left" => Some('D'), "home" => Some('H'), "end" => Some('F'), _ => None,
    };
    if let Some(final_byte) = final_byte {
        return Some(if modifier == 1 { format!("\x1b{}{final_byte}", if application_cursor { 'O' } else { '[' }) }
            else { format!("\x1b[1;{modifier}{final_byte}") });
    }
    let number = match key {
        "insert" => 2, "delete" => 3, "page_up" => 5, "page_down" => 6,
        "f5" => 15, "f6" => 17, "f7" => 18, "f8" => 19, "f9" => 20, "f10" => 21, "f11" => 23, "f12" => 24, _ => 0,
    };
    if number != 0 { return Some(if modifier == 1 { format!("\x1b[{number}~") } else { format!("\x1b[{number};{modifier}~") }); }
    if let Some(final_byte) = match key { "f1" => Some('P'), "f2" => Some('Q'), "f3" => Some('R'), "f4" => Some('S'), _ => None } {
        return Some(if modifier == 1 { format!("\x1bO{final_byte}") } else { format!("\x1b[1;{modifier}{final_byte}") });
    }
    Some(match key {
        "tab" if shift => "\x1b[Z", "tab" if alt => "\x1b\t", "tab" => "\t",
        "return" if alt => "\x1b\r", "return" => "\r", "backspace" if alt => "\x1b\x7f",
        "backspace" => "\x7f", "escape" => "\x1b", _ => return None,
    }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_modes_and_modifiers_are_distinct() {
        assert_eq!(special_key("up", false, false, false, true).as_deref(), Some("\x1bOA"));
        assert_eq!(special_key("up", true, true, true, true).as_deref(), Some("\x1b[1;8A"));
        assert_eq!(special_key("f12", false, false, true, false).as_deref(), Some("\x1b[24;5~"));
        assert_eq!(special_key("tab", true, false, false, false).as_deref(), Some("\x1b[Z"));
        assert!(special_key("unknown", false, false, false, false).is_none());
    }
}
