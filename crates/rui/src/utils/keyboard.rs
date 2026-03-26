/// `caret` 为 UTF-8 字符边界上的字节下标（插入点）。
pub fn caret_move_left(s: &str, caret: usize) -> usize {
    let caret = caret.min(s.len());
    let caret = s.floor_char_boundary(caret);
    if caret == 0 {
        return 0;
    }
    let mut prev = 0;
    for (i, _) in s.char_indices() {
        if i < caret {
            prev = i;
        } else {
            break;
        }
    }
    prev
}

pub fn caret_move_right(s: &str, caret: usize) -> usize {
    let caret = caret.min(s.len());
    let caret = s.floor_char_boundary(caret);
    if caret >= s.len() {
        return s.len();
    }
    let c = s[caret..].chars().next().unwrap();
    caret + c.len_utf8()
}

/// 展示串：`preedit` 插入 `committed` 的 `ime_anchor`。
pub fn input_display_text(committed: &str, preedit: &str, ime_anchor: Option<usize>) -> String {
    if preedit.is_empty() {
        return committed.to_string();
    }
    let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
    a = committed.floor_char_boundary(a);
    format!("{}{}{}", &committed[..a], preedit, &committed[a..])
}

pub fn delete_char_forward(s: &mut String, caret: usize) -> bool {
    let caret = s.floor_char_boundary(caret.min(s.len()));
    if caret >= s.len() {
        return false;
    }
    let ch = s[caret..].chars().next().unwrap();
    let end = caret + ch.len_utf8();
    s.replace_range(caret..end, "");
    true
}
