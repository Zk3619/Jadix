use winit::keyboard::Key;

use crate::utils::text_field::caret_line_and_local;

/// winit 0.31 / keyboard-types 0.8: Space is `Key::Character(" ")`, not a named key.
pub fn is_space_key(key: &Key) -> bool {
    matches!(key, Key::Character(s) if s.as_str() == " ")
}

pub fn is_enter_or_space(key: &Key) -> bool {
    matches!(key, Key::Named(winit::keyboard::NamedKey::Enter)) || is_space_key(key)
}

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

/// ← at hard line start jumps to previous line end (skips `\n` stop).
pub fn caret_move_left_line_aware(s: &str, caret: usize) -> usize {
    let caret = s.floor_char_boundary(caret.min(s.len()));
    if caret == 0 {
        return 0;
    }
    let home = caret_line_home(s, caret);
    if caret == home && home > 0 {
        return caret_line_end(s, home - 1);
    }
    caret_move_left(s, caret)
}

/// → at hard line end jumps to next line start (skips `\n`).
pub fn caret_move_right_line_aware(s: &str, caret: usize) -> usize {
    let caret = s.floor_char_boundary(caret.min(s.len()));
    if caret >= s.len() {
        return s.len();
    }
    let end = caret_line_end(s, caret);
    if caret >= end {
        if s[caret..].starts_with('\n') {
            return (caret + 1).min(s.len());
        }
        return s.len();
    }
    caret_move_right(s, caret)
}

/// ← at soft visual line start jumps to previous visual line end.
pub fn caret_move_left_visual(s: &str, ranges: &[(usize, usize)], caret: usize) -> usize {
    let caret = s.floor_char_boundary(caret.min(s.len()));
    if caret == 0 {
        return 0;
    }
    if ranges.is_empty() {
        return caret_move_left_line_aware(s, caret);
    }
    let line_i = crate::text::caret_visual_line_index(ranges, caret);
    let (start, _) = ranges[line_i];
    if caret <= start && line_i > 0 {
        return ranges[line_i - 1].1;
    }
    caret_move_left(s, caret)
}

/// → at soft visual line end jumps to next visual line start.
pub fn caret_move_right_visual(s: &str, ranges: &[(usize, usize)], caret: usize) -> usize {
    let caret = s.floor_char_boundary(caret.min(s.len()));
    if caret >= s.len() {
        return s.len();
    }
    if ranges.is_empty() {
        return caret_move_right_line_aware(s, caret);
    }
    let line_i = crate::text::caret_visual_line_index(ranges, caret);
    let (_, end) = ranges[line_i];
    if caret >= end && line_i + 1 < ranges.len() {
        return ranges[line_i + 1].0;
    }
    caret_move_right(s, caret)
}

/// Multiline horizontal nav: soft-wrap visual lines when `ranges` is set.
pub fn caret_move_left_multiline(
    s: &str,
    caret: usize,
    visual_ranges: Option<&[(usize, usize)]>,
) -> usize {
    match visual_ranges {
        Some(r) if !r.is_empty() => caret_move_left_visual(s, r, caret),
        _ => caret_move_left_line_aware(s, caret),
    }
}

pub fn caret_move_right_multiline(
    s: &str,
    caret: usize,
    visual_ranges: Option<&[(usize, usize)]>,
) -> usize {
    match visual_ranges {
        Some(r) if !r.is_empty() => caret_move_right_visual(s, r, caret),
        _ => caret_move_right_line_aware(s, caret),
    }
}

/// PageUp/PageDown: move by `lines_per_page` visual or hard lines.
pub fn caret_move_page_vertical(
    s: &str,
    ranges: Option<&[(usize, usize)]>,
    caret_xs: Option<&[Vec<(usize, f32)>]>,
    caret: usize,
    delta_pages: i32,
    lines_per_page: usize,
    preferred_x: Option<f32>,
) -> usize {
    let delta_lines = delta_pages * lines_per_page.max(1) as i32;
    if let Some(ranges) = ranges.filter(|r| !r.is_empty()) {
        caret_move_vertical_visual_pref(s, ranges, caret_xs, caret, delta_lines, preferred_x)
    } else {
        caret_move_vertical(s, caret, delta_lines)
    }
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
    caret_move_vertical_visual_pref(s, ranges, None, caret, delta_lines, None)
}

/// ↑/↓ using Preferred X (px from line start) when provided; else character column.
pub fn caret_move_vertical_visual_pref(
    s: &str,
    ranges: &[(usize, usize)],
    caret_xs: Option<&[Vec<(usize, f32)>]>,
    caret: usize,
    delta_lines: i32,
    preferred_x: Option<f32>,
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
    if let (Some(px), Some(xs_rows)) = (preferred_x, caret_xs) {
        if let Some(xs) = xs_rows.get(target) {
            if !xs.is_empty() {
                let mut best = t_start;
                let mut best_d = f32::MAX;
                for &(b, x) in xs {
                    let d = (x - px).abs();
                    if d < best_d {
                        best_d = d;
                        best = b;
                    }
                }
                return best.clamp(t_start, t_end);
            }
        }
    }
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
#[path = "tests/keyboard.rs"]
mod tests;
