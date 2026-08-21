use crate::utils::text_field::caret_line_and_local;

/// Hard-`\n` lines as `(byte_start, line_body)` — trailing empty line after final `\n` included.
fn hard_line_slices(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            out.push((start, &text[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &text[start..]));
    out
}

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

/// Home：当前 hard 行行首。
pub fn caret_line_home(s: &str, caret: usize) -> usize {
    let (line_i, _) = caret_line_and_local(s, caret);
    hard_line_slices(s)
        .get(line_i)
        .map(|(start, _)| *start)
        .unwrap_or(0)
}

/// End：当前 hard 行行尾（不含 `\n`）。
pub fn caret_line_end(s: &str, caret: usize) -> usize {
    let (line_i, _) = caret_line_and_local(s, caret);
    hard_line_slices(s)
        .get(line_i)
        .map(|(start, body)| start + body.len())
        .unwrap_or_else(|| s.len())
}

/// Soft-wrap visual-line Home（`ranges` from [`crate::text::soft_visual_line_byte_ranges`]).
pub fn caret_visual_line_home(ranges: &[(usize, usize)], caret: usize) -> usize {
    let i = crate::text::caret_visual_line_index(ranges, caret);
    ranges.get(i).map(|(s, _)| *s).unwrap_or(0)
}

/// Soft-wrap visual-line End.
pub fn caret_visual_line_end(ranges: &[(usize, usize)], caret: usize) -> usize {
    let i = crate::text::caret_visual_line_index(ranges, caret);
    ranges.get(i).map(|(_, e)| *e).unwrap_or(0)
}

/// ↑/↓：按 hard 行上下移动，尽量保持字符列；已在首/末行则原地不动。
pub fn caret_move_vertical(s: &str, caret: usize, delta_lines: i32) -> usize {
    if delta_lines == 0 || s.is_empty() {
        return s.floor_char_boundary(caret.min(s.len()));
    }
    let lines = hard_line_slices(s);
    let (line_i, local) = caret_line_and_local(s, caret);
    let Some((start, body)) = lines.get(line_i).copied() else {
        return s.floor_char_boundary(caret.min(s.len()));
    };
    let col = s[start..start + local.min(body.len())].chars().count();
    let target = if delta_lines < 0 {
        line_i.saturating_sub((-delta_lines) as usize)
    } else {
        (line_i + delta_lines as usize).min(lines.len().saturating_sub(1))
    };
    if target == line_i {
        return s.floor_char_boundary(caret.min(s.len()));
    }
    let (t_start, t_body) = lines[target];
    let mut n = 0usize;
    for (i, _) in t_body.char_indices() {
        if n == col {
            return t_start + i;
        }
        n += 1;
    }
    t_start + t_body.len()
}

/// ↑/↓ along soft-wrap visual lines; preserves character column within the visual row.
pub fn caret_move_vertical_visual(
    s: &str,
    ranges: &[(usize, usize)],
    caret: usize,
    delta_lines: i32,
) -> usize {
    if delta_lines == 0 || s.is_empty() || ranges.is_empty() {
        return s.floor_char_boundary(caret.min(s.len()));
    }
    let line_i = crate::text::caret_visual_line_index(ranges, caret);
    let (start, end) = ranges[line_i];
    let local = caret.saturating_sub(start).min(end.saturating_sub(start));
    let col = s[start..start + local].chars().count();
    let target = if delta_lines < 0 {
        line_i.saturating_sub((-delta_lines) as usize)
    } else {
        (line_i + delta_lines as usize).min(ranges.len().saturating_sub(1))
    };
    if target == line_i {
        return s.floor_char_boundary(caret.min(s.len()));
    }
    let (t_start, t_end) = ranges[target];
    let t_body = &s[t_start..t_end];
    let mut n = 0usize;
    for (i, _) in t_body.char_indices() {
        if n == col {
            return t_start + i;
        }
        n += 1;
    }
    t_end
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

/// Select-all range `(0, len)` on UTF-8 char boundaries (always valid for paint/clipboard).
pub fn select_all_range(s: &str) -> Option<(usize, usize)> {
    if s.is_empty() {
        return None;
    }
    Some((0, s.len()))
}

/// Hard-`\n` line containing `caret`: `(home, end)` with `home < end`, or `None` if empty.
pub fn select_line_range(s: &str, caret: usize) -> Option<(usize, usize)> {
    let home = caret_line_home(s, caret);
    let end = caret_line_end(s, caret);
    if home < end {
        Some((home, end))
    } else {
        None
    }
}

/// Shift-extend: keep the fixed anchor, move the active end to `new_caret`.
///
/// When there is no selection yet, `old_caret` becomes the anchor.
pub fn extend_selection(
    old_caret: usize,
    existing: Option<(usize, usize)>,
    new_caret: usize,
) -> (usize, usize) {
    let anchor = match existing {
        Some((a, b)) if a != b => {
            let lo = a.min(b);
            let hi = a.max(b);
            if old_caret == hi {
                lo
            } else if old_caret == lo {
                hi
            } else {
                old_caret
            }
        }
        _ => old_caret,
    };
    (anchor, new_caret)
}

/// Normalize an unordered selection to `(lo, hi)` with `lo < hi`, or `None` if empty/invalid.
pub fn normalized_selection(s: &str, sel: Option<(usize, usize)>) -> Option<(usize, usize)> {
    let (a, b) = sel?;
    let lo = a.min(b).min(s.len());
    let hi = a.max(b).min(s.len());
    let lo = s.floor_char_boundary(lo);
    let hi = s.floor_char_boundary(hi);
    if lo < hi {
        Some((lo, hi))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_all_and_extend_selection() {
        assert_eq!(select_all_range(""), None);
        assert_eq!(select_all_range("你好"), Some((0, "你好".len())));
        // New selection: anchor at old caret
        assert_eq!(extend_selection(3, None, 1), (3, 1));
        // Extend from existing: caret was at hi → keep lo
        assert_eq!(extend_selection(5, Some((2, 5)), 7), (2, 7));
        // Extend from existing: caret was at lo → keep hi
        assert_eq!(extend_selection(2, Some((2, 5)), 0), (5, 0));
        assert_eq!(normalized_selection("abcd", Some((3, 1))), Some((1, 3)));
        assert_eq!(normalized_selection("abcd", Some((2, 2))), None);
    }

    #[test]
    fn select_line_range_covers_hard_line_only() {
        let s = "ab\n你好\ncd";
        let on_mid = "ab\n".len() + "你".len();
        assert_eq!(
            select_line_range(s, on_mid),
            Some(("ab\n".len(), "ab\n你好".len()))
        );
        assert_eq!(select_line_range(s, 0), Some((0, 2)));
        assert_eq!(select_line_range("", 0), None);
        assert_eq!(select_line_range("\n", 0), None);
    }

    #[test]
    fn home_end_stay_on_hard_line() {
        let s = "的\n啊\n我";
        let mid = "的\n".len() + "啊".len(); // end of line 1 "啊"
        assert_eq!(caret_line_home(s, mid), "的\n".len());
        assert_eq!(caret_line_end(s, "的\n".len()), "的\n".len() + "啊".len());
        assert_eq!(caret_line_home(s, 0), 0);
        assert_eq!(caret_line_end(s, s.len()), s.len());
    }

    #[test]
    fn arrow_vertical_moves_between_hard_lines() {
        let s = "的\n啊\n我";
        let on_second = "的\n".len(); // start of "啊"
        let up = caret_move_vertical(s, on_second, -1);
        assert_eq!(up, 0);
        let down = caret_move_vertical(s, on_second, 1);
        assert_eq!(down, "的\n啊\n".len());
        // First line Up stays; last line Down stays.
        assert_eq!(caret_move_vertical(s, 0, -1), 0);
        assert_eq!(caret_move_vertical(s, s.len(), 1), s.len());
    }

    #[test]
    fn arrow_vertical_preserves_char_column() {
        let s = "abc\nxy";
        let after_b = "ab".len();
        let down = caret_move_vertical(s, after_b, 1);
        assert_eq!(down, "abc\n".len() + "xy".len()); // col 2 clamps to end of "xy"
        let s2 = "ab\nxyz";
        let after_a = "a".len();
        let down2 = caret_move_vertical(s2, after_a, 1);
        assert_eq!(down2, "ab\n".len() + "x".len());
    }
}
