//! Alt+click extra carets for Input / Textarea.
//!
//! Primary caret stays in the existing caret/selection signals. Extra carets are
//! `(caret, anchor)` pairs. Typing, delete, and arrows apply to every cursor;
//! edits run right-to-left so earlier indices stay valid.

use crate::utils::keyboard::{delete_char_forward, extend_selection};
use crate::utils::text_field::caret_line_and_local;

/// One insertion point. Collapsed when `caret == anchor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextCursor {
    pub caret: usize,
    pub anchor: usize,
}

impl TextCursor {
    pub fn collapsed(at: usize) -> Self {
        Self {
            caret: at,
            anchor: at,
        }
    }

    pub fn selection(self) -> Option<(usize, usize)> {
        if self.caret == self.anchor {
            None
        } else {
            Some((self.anchor, self.caret))
        }
    }

    pub fn range(self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    fn clamp(self, text: &str) -> Self {
        let caret = text.floor_char_boundary(self.caret.min(text.len()));
        let anchor = text.floor_char_boundary(self.anchor.min(text.len()));
        Self { caret, anchor }
    }
}

/// Extra carets as `(caret, anchor)` tuples for the named signal.
pub type ExtraCaret = (usize, usize);

pub fn extras_from_cursors(cursors: &[TextCursor]) -> Vec<ExtraCaret> {
    cursors
        .iter()
        .skip(1)
        .map(|c| (c.caret, c.anchor))
        .collect()
}

pub fn assemble_cursors(
    text: &str,
    primary_caret: Option<usize>,
    primary_sel: Option<(usize, usize)>,
    extras: &[ExtraCaret],
) -> Vec<TextCursor> {
    let len = text.len();
    let caret = text.floor_char_boundary(primary_caret.unwrap_or(len).min(len));
    let primary = if let Some((a, b)) = primary_sel {
        let a = text.floor_char_boundary(a.min(len));
        let b = text.floor_char_boundary(b.min(len));
        if a != b {
            // Existing store is unordered `(lo, hi)` or `(anchor, caret)`. Prefer
            // caret-at-hi when the pair does not encode direction (lo < hi, caret
            // already at one end).
            if caret == a || caret == b {
                TextCursor {
                    caret,
                    anchor: if caret == a { b } else { a },
                }
            } else {
                TextCursor {
                    caret: b,
                    anchor: a,
                }
            }
        } else {
            TextCursor::collapsed(caret)
        }
    } else {
        TextCursor::collapsed(caret)
    };
    let mut out = Vec::with_capacity(1 + extras.len());
    out.push(primary);
    for &(c, a) in extras {
        out.push(
            TextCursor {
                caret: c,
                anchor: a,
            }
            .clamp(text),
        );
    }
    normalize(text, out)
}

pub fn split_primary(cursors: &[TextCursor]) -> (usize, Option<(usize, usize)>, Vec<ExtraCaret>) {
    let Some(first) = cursors.first() else {
        return (0, None, Vec::new());
    };
    (first.caret, first.selection(), extras_from_cursors(cursors))
}

/// Merge identical / overlapping cursors. Keeps left-to-right order. First cursor
/// after sort is the new primary (leftmost). Callers that need a stable primary
/// should pass it as index 0 and we preserve it when it does not overlap extras.
pub fn normalize(text: &str, cursors: Vec<TextCursor>) -> Vec<TextCursor> {
    if cursors.is_empty() {
        return vec![TextCursor::collapsed(0)];
    }
    let mut cs: Vec<TextCursor> = cursors.into_iter().map(|c| c.clamp(text)).collect();
    // Stable: original primary stays first among equal ranges so paint/IME keep it.
    let primary = cs[0];
    cs.sort_by_key(|c| (c.range().0, c.range().1, c.caret));
    let mut merged: Vec<TextCursor> = Vec::with_capacity(cs.len());
    for c in cs {
        if let Some(last) = merged.last_mut() {
            let (l0, l1) = last.range();
            let (c0, c1) = c.range();
            if ranges_touch_or_overlap(l0, l1, c0, c1) {
                let lo = l0.min(c0);
                let hi = l1.max(c1);
                if lo == hi {
                    *last = TextCursor::collapsed(lo);
                } else {
                    last.anchor = lo;
                    last.caret = hi;
                }
                continue;
            }
        }
        merged.push(c);
    }
    if let Some(i) = merged
        .iter()
        .position(|c| c.caret == primary.caret && c.anchor == primary.anchor)
    {
        merged.swap(0, i);
    }
    merged
}

fn ranges_touch_or_overlap(a0: usize, a1: usize, b0: usize, b1: usize) -> bool {
    if a0 == a1 && b0 == b1 {
        return a0 == b0;
    }
    a0 <= b1 && b0 <= a1
}

/// Alt+click: add a collapsed caret, or drop an extra that already sits here.
/// Returns `None` when the click toggled the last extra off (or hit primary with
/// no extras — no-op). `Some` is the cursors after the click; `added` is whether
/// a new extra was created (starts a multi drag).
pub fn add_or_toggle(text: &str, cursors: Vec<TextCursor>, at: usize) -> (Vec<TextCursor>, bool) {
    let at = text.floor_char_boundary(at.min(text.len()));
    let mut cs = normalize(text, cursors);
    if let Some(i) = cs.iter().position(|c| c.caret == at && c.anchor == at) {
        if i == 0 {
            return (cs, false);
        }
        cs.remove(i);
        return (normalize(text, cs), false);
    }
    cs.push(TextCursor::collapsed(at));
    (normalize(text, cs), true)
}

/// Carets for an Alt+drag gesture.
///
/// Same line → one extra with a selection. Crossing hard lines: collapsed carets at
/// the press column, or with `column_select` (Shift+Alt) a per-line selection from
/// the **origin** caret column to the pointer column. The I-beam stays on the origin
/// column when the end flips left/right. Lines that cannot hold both columns are
/// skipped (no clamp-to-EOL selection).
pub fn gesture_carets(text: &str, from: usize, to: usize, column_select: bool) -> Vec<TextCursor> {
    let from = text.floor_char_boundary(from.min(text.len()));
    let to = text.floor_char_boundary(to.min(text.len()));
    let (line_a, byte_from) = caret_line_and_local(text, from);
    let (line_b, _) = caret_line_and_local(text, to);
    let col_from = line_and_char_col(text, from).1;
    let col_to = line_and_char_col(text, to).1;
    if line_a == line_b {
        return vec![TextCursor {
            caret: if column_select { from } else { to },
            anchor: if column_select { to } else { from },
        }];
    }
    let lo = line_a.min(line_b);
    let hi = line_a.max(line_b);
    let lines = hard_line_ranges(text);
    let mut out = Vec::with_capacity(hi - lo + 1);
    for i in lo..=hi {
        let Some(&(start, end)) = lines.get(i) else {
            continue;
        };
        if column_select {
            let Some(caret) = byte_at_char_col(text, start, end, col_from) else {
                continue;
            };
            let Some(anchor) = byte_at_char_col(text, start, end, col_to) else {
                continue;
            };
            out.push(TextCursor { caret, anchor });
        } else {
            let body_len = end.saturating_sub(start);
            let local = byte_from.min(body_len);
            let at = text.floor_char_boundary((start + local).min(end));
            out.push(TextCursor::collapsed(at));
        }
    }
    if column_select {
        let origin_idx = out.iter().position(|c| {
            caret_line_and_local(text, c.anchor).0 == line_a
                || caret_line_and_local(text, c.caret).0 == line_a
        });
        match origin_idx {
            Some(i) => out.swap(0, i),
            None => out.insert(0, TextCursor::collapsed(from)),
        }
    }
    if out.is_empty() {
        out.push(TextCursor::collapsed(from));
    }
    out
}

/// Caret (origin column) on the hard line under `pointer`, if that line is in `cursors`.
pub fn caret_on_pointer_line(text: &str, cursors: &[TextCursor], pointer: usize) -> Option<usize> {
    let (line, _) = caret_line_and_local(text, pointer);
    cursors
        .iter()
        .find_map(|c| (caret_line_and_local(text, c.caret).0 == line).then_some(c.caret))
}

fn line_and_char_col(text: &str, byte_ix: usize) -> (usize, usize) {
    let (line, local_byte) = caret_line_and_local(text, byte_ix);
    let lines = hard_line_ranges(text);
    let start = lines.get(line).map(|(s, _)| *s).unwrap_or(0);
    let end = (start + local_byte).min(text.len());
    let col = text.get(start..end).map(|s| s.chars().count()).unwrap_or(0);
    (line, col)
}

/// `col == n` is EOL. `col > n` → line is too short.
fn byte_at_char_col(text: &str, start: usize, end: usize, col: usize) -> Option<usize> {
    let body = text.get(start..end)?;
    let n = body.chars().count();
    if col > n {
        return None;
    }
    if col == n {
        return Some(end);
    }
    body.char_indices()
        .nth(col)
        .map(|(i, _)| start + i)
        .or(Some(end))
}

fn hard_line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            out.push((start, i));
            start = i + 1;
        }
    }
    out.push((start, text.len()));
    out
}

pub fn insert_at_cursors(
    text: &mut String,
    cursors: Vec<TextCursor>,
    insert: &str,
) -> Vec<TextCursor> {
    if insert.is_empty() && cursors.iter().all(|c| c.caret == c.anchor) {
        return normalize(text, cursors);
    }
    let mut cs: Vec<(usize, TextCursor)> = cursors.into_iter().enumerate().collect();
    cs.sort_by_key(|(_, c)| c.range().0);
    let mut next = vec![TextCursor::collapsed(0); cs.len()];
    let mut delta: isize = 0;
    for (orig_i, c) in cs {
        let (raw_lo, raw_hi) = c.range();
        let lo = clamp_shifted(text, raw_lo, delta);
        let hi = clamp_shifted(text, raw_hi, delta).max(lo);
        text.replace_range(lo..hi, insert);
        next[orig_i] = TextCursor::collapsed(lo + insert.len());
        delta += insert.len() as isize - (hi - lo) as isize;
    }
    normalize(text, next)
}

fn clamp_shifted(text: &str, pos: usize, delta: isize) -> usize {
    let shifted = (pos as isize + delta).max(0) as usize;
    text.floor_char_boundary(shifted.min(text.len()))
}

pub fn delete_backward(text: &mut String, cursors: Vec<TextCursor>) -> Vec<TextCursor> {
    let mut cs: Vec<(usize, TextCursor)> = cursors.into_iter().enumerate().collect();
    cs.sort_by_key(|(_, c)| c.range().0);
    let mut next = vec![TextCursor::collapsed(0); cs.len()];
    let mut delta: isize = 0;
    for (orig_i, c) in cs {
        let (raw_lo, raw_hi) = c.range();
        let lo = clamp_shifted(text, raw_lo, delta);
        let hi = clamp_shifted(text, raw_hi, delta).max(lo);
        if lo < hi {
            text.replace_range(lo..hi, "");
            next[orig_i] = TextCursor::collapsed(lo);
            delta -= (hi - lo) as isize;
        } else if lo > 0 {
            let prev = text.floor_char_boundary(lo - 1);
            text.replace_range(prev..lo, "");
            next[orig_i] = TextCursor::collapsed(prev);
            delta -= (lo - prev) as isize;
        } else {
            next[orig_i] = TextCursor::collapsed(0);
        }
    }
    normalize(text, next)
}

pub fn delete_forward(text: &mut String, cursors: Vec<TextCursor>) -> Vec<TextCursor> {
    let mut cs: Vec<(usize, TextCursor)> = cursors.into_iter().enumerate().collect();
    cs.sort_by_key(|(_, c)| c.range().0);
    let mut next = vec![TextCursor::collapsed(0); cs.len()];
    let mut delta: isize = 0;
    for (orig_i, c) in cs {
        let (raw_lo, raw_hi) = c.range();
        let lo = clamp_shifted(text, raw_lo, delta);
        let hi = clamp_shifted(text, raw_hi, delta).max(lo);
        if lo < hi {
            text.replace_range(lo..hi, "");
            next[orig_i] = TextCursor::collapsed(lo);
            delta -= (hi - lo) as isize;
        } else {
            let before = text.len();
            let _ = delete_char_forward(text, lo);
            next[orig_i] = TextCursor::collapsed(lo.min(text.len()));
            delta -= (before - text.len()) as isize;
        }
    }
    normalize(text, next)
}

pub fn move_cursors(
    text: &str,
    cursors: Vec<TextCursor>,
    shift: bool,
    mut dest_of: impl FnMut(TextCursor) -> usize,
) -> Vec<TextCursor> {
    let next = cursors
        .into_iter()
        .map(|c| {
            let c = c.clamp(text);
            let dest = dest_of(c).min(text.len());
            let dest = text.floor_char_boundary(dest);
            if shift {
                let (anchor, caret) = extend_selection(c.caret, c.selection(), dest);
                TextCursor { caret, anchor }
            } else {
                TextCursor::collapsed(dest)
            }
        })
        .collect();
    normalize(text, next)
}

/// Unique sorted committed-byte insertion points (selection starts / collapsed carets).
pub fn caret_insert_points(cursors: &[TextCursor]) -> Vec<usize> {
    let mut v: Vec<usize> = cursors.iter().map(|c| c.range().0).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Insert `preedit` at each unique anchor (committed UTF-8 bytes), left to right.
pub fn splice_preedit(committed: &str, anchors: &[usize], preedit: &str) -> String {
    if preedit.is_empty() {
        return committed.to_string();
    }
    let mut points: Vec<usize> = anchors
        .iter()
        .map(|&a| committed.floor_char_boundary(a.min(committed.len())))
        .collect();
    points.sort_unstable();
    points.dedup();
    if points.is_empty() {
        return committed.to_string();
    }
    let mut out = String::with_capacity(committed.len() + preedit.len() * points.len());
    let mut last = 0usize;
    for a in points {
        out.push_str(&committed[last..a]);
        out.push_str(preedit);
        last = a;
    }
    out.push_str(&committed[last..]);
    out
}

/// Display-byte caret sitting inside the splice at `committed_anchor`.
pub fn preedit_display_caret(
    committed: &str,
    anchors: &[usize],
    preedit: &str,
    committed_anchor: usize,
    preedit_caret: usize,
) -> usize {
    let a = committed.floor_char_boundary(committed_anchor.min(committed.len()));
    let pc = preedit.floor_char_boundary(preedit_caret.min(preedit.len()));
    if preedit.is_empty() {
        return a;
    }
    let n_before = anchors
        .iter()
        .map(|&x| committed.floor_char_boundary(x.min(committed.len())))
        .filter(|&x| x < a)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    a + n_before * preedit.len() + pc
}

/// Display `[start, end)` of every preedit splice (unique sorted anchors).
pub fn preedit_display_ranges(
    committed: &str,
    anchors: &[usize],
    preedit: &str,
) -> Vec<(usize, usize)> {
    if preedit.is_empty() {
        return Vec::new();
    }
    let mut points: Vec<usize> = anchors
        .iter()
        .map(|&a| committed.floor_char_boundary(a.min(committed.len())))
        .collect();
    points.sort_unstable();
    points.dedup();
    points
        .iter()
        .map(|&a| {
            let start = preedit_display_caret(committed, &points, preedit, a, 0);
            (start, start + preedit.len())
        })
        .collect()
}

/// Map a byte in the spliced display string back to a committed-space caret.
pub fn unsplice_display_hit(
    committed: &str,
    anchors: &[usize],
    preedit: &str,
    display_ix: usize,
) -> usize {
    if preedit.is_empty() {
        return committed.floor_char_boundary(display_ix.min(committed.len()));
    }
    let ranges = preedit_display_ranges(committed, anchors, preedit);
    let display_len = committed.len() + preedit.len() * ranges.len();
    let mut ix = display_ix.min(display_len);
    for &(s, e) in ranges.iter().rev() {
        if ix >= e {
            ix -= preedit.len();
        } else if ix > s {
            ix = s - ranges.iter().filter(|&&(a, _)| a < s).count() * preedit.len();
            return committed.floor_char_boundary(ix.min(committed.len()));
        }
    }
    committed.floor_char_boundary(ix.min(committed.len()))
}

/// After a multi-cursor edit, keep `latest` on the same slot (primary-first, extras LTR).
pub fn follow_latest(old: &[TextCursor], new: &[TextCursor], latest: usize) -> usize {
    if let Some(i) = old
        .iter()
        .position(|c| c.caret == latest || c.anchor == latest)
    {
        if let Some(c) = new.get(i) {
            return c.caret;
        }
    }
    new.first().map(|c| c.caret).unwrap_or(latest)
}

/// Latest compose caret: explicit latest if it still sits on a cursor, else primary.
pub fn resolve_compose_anchor(cursors: &[TextCursor], latest: Option<usize>) -> usize {
    let primary = cursors.first().map(|c| c.caret).unwrap_or(0);
    let Some(latest) = latest else {
        return primary;
    };
    if cursors
        .iter()
        .any(|c| c.caret == latest || c.anchor == latest)
    {
        latest
    } else {
        primary
    }
}

/// Collapse extras, keep the primary caret.
pub fn collapse_to_primary(cursors: Vec<TextCursor>) -> Vec<TextCursor> {
    match cursors.first() {
        Some(c) => vec![TextCursor::collapsed(c.caret)],
        None => vec![TextCursor::collapsed(0)],
    }
}

/// Copy payload: non-empty selections joined with `\n` (VS Code).
pub fn copy_payload(text: &str, cursors: &[TextCursor]) -> Option<String> {
    let parts: Vec<&str> = cursors
        .iter()
        .filter_map(|c| {
            let (lo, hi) = c.range();
            (lo < hi && hi <= text.len()).then(|| &text[lo..hi])
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::text_field::caret_line_and_local;

    #[test]
    fn insert_same_glyph_at_two_carets() {
        let mut s = "abc".to_string();
        let cs = vec![TextCursor::collapsed(1), TextCursor::collapsed(2)];
        let out = insert_at_cursors(&mut s, cs, "X");
        assert_eq!(s, "aXbXc");
        assert_eq!(out[0].caret, 2);
        assert_eq!(out[1].caret, 4);
    }

    #[test]
    fn insert_replaces_each_selection() {
        let mut s = "hello".to_string();
        let cs = vec![
            TextCursor {
                caret: 2,
                anchor: 0,
            },
            TextCursor {
                caret: 5,
                anchor: 3,
            },
        ];
        let out = insert_at_cursors(&mut s, cs, "X");
        assert_eq!(s, "XlX");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].caret, 1);
        assert_eq!(out[1].caret, 3);
    }

    #[test]
    fn backspace_at_two_carets() {
        let mut s = "abcd".to_string();
        let cs = vec![TextCursor::collapsed(1), TextCursor::collapsed(3)];
        let out = delete_backward(&mut s, cs);
        assert_eq!(s, "bd");
        assert_eq!(out[0].caret, 0);
        assert_eq!(out[1].caret, 1);
    }

    #[test]
    fn add_or_toggle_drops_extra_at_same_index() {
        let s = "hello";
        let start = vec![TextCursor::collapsed(0), TextCursor::collapsed(2)];
        let (cs, added) = add_or_toggle(s, start, 2);
        assert!(!added);
        assert_eq!(cs, vec![TextCursor::collapsed(0)]);
    }

    #[test]
    fn add_or_toggle_ignores_primary_duplicate() {
        let s = "hello";
        let start = vec![TextCursor::collapsed(1)];
        let (cs, added) = add_or_toggle(s, start, 1);
        assert!(!added);
        assert_eq!(cs, vec![TextCursor::collapsed(1)]);
    }

    #[test]
    fn column_drag_places_one_caret_per_hard_line() {
        let s = "ab\ncd\nef";
        let from = 1; // 'b'
        let to = s.len();
        let cs = gesture_carets(s, from, to, false);
        assert_eq!(cs.len(), 3);
        assert_eq!(cs[0], TextCursor::collapsed(1));
        assert_eq!(cs[1], TextCursor::collapsed("ab\n".len() + 1));
        assert_eq!(cs[2], TextCursor::collapsed("ab\ncd\n".len() + 1));
    }

    #[test]
    fn same_line_gesture_is_a_selection() {
        let s = "hello";
        let cs = gesture_carets(s, 1, 4, false);
        assert_eq!(
            cs,
            vec![TextCursor {
                caret: 4,
                anchor: 1
            }]
        );
    }

    #[test]
    fn shift_alt_column_drag_selects_each_hard_line() {
        let s = "abcd\nefg\nhi";
        let from = 1; // 'b'
        let to = "abcd\nefg\n".len() + 2; // past 'i' → col 2
        let cs = gesture_carets(s, from, to, true);
        assert_eq!(cs.len(), 3);
        assert_eq!(
            cs[0],
            TextCursor {
                caret: 1,
                anchor: 2
            }
        );
        let l1 = "abcd\n".len();
        assert_eq!(
            cs[1],
            TextCursor {
                caret: l1 + 1,
                anchor: l1 + 2
            }
        );
        let l2 = "abcd\nefg\n".len();
        assert_eq!(
            cs[2],
            TextCursor {
                caret: l2 + 1,
                anchor: l2 + 2
            }
        );
    }

    #[test]
    fn shift_alt_column_drag_skips_short_lines() {
        let s = "abcd\nx\nzzzz";
        let from = 0;
        let to = "abcd\nx\nzz".len(); // col 2 on last line
        let cs = gesture_carets(s, from, to, true);
        assert_eq!(cs.len(), 2);
        assert_eq!(
            cs[0],
            TextCursor {
                caret: 0,
                anchor: 2
            }
        );
        let last = "abcd\nx\n".len();
        assert_eq!(
            cs[1],
            TextCursor {
                caret: last,
                anchor: last + 2
            }
        );
    }

    #[test]
    fn shift_alt_reversing_end_keeps_origin_caret() {
        let s = "abcdefgh\nijklmnop";
        let origin = 3;
        let l1 = "abcdefgh\n".len();
        let right = gesture_carets(s, origin, l1 + 6, true);
        assert!(right
            .iter()
            .all(|c| { caret_line_and_local(s, c.caret).1 == caret_line_and_local(s, origin).1 }));
        assert_eq!(right[0].caret, origin);
        assert_eq!(right[0].anchor, 6);
        let left = gesture_carets(s, origin, l1 + 1, true);
        assert!(left.iter().all(|c| c.caret != c.anchor));
        assert_eq!(left[0].caret, origin);
        assert_eq!(left[0].anchor, 1);
        assert_eq!(left[1].caret, l1 + 3);
        assert_eq!(left[1].anchor, l1 + 1);
    }

    #[test]
    fn shift_alt_click_other_line_same_column_places_carets() {
        let s = "abcd\nefgh\nijkl";
        let from = 2;
        let to = "abcd\nefgh\n".len() + 2;
        let cs = gesture_carets(s, from, to, true);
        assert_eq!(cs.len(), 3);
        assert!(cs.iter().all(|c| c.caret == c.anchor));
        assert_eq!(cs[0], TextCursor::collapsed(2));
        assert_eq!(cs[1], TextCursor::collapsed("abcd\n".len() + 2));
        assert_eq!(cs[2], TextCursor::collapsed(to));
    }

    #[test]
    fn shift_alt_skips_line_too_short_for_start_column() {
        let s = "abcd\nab\nwxyz";
        let from = 3;
        let to = "abcd\nab\n".len() + 3;
        let cs = gesture_carets(s, from, to, true);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0], TextCursor::collapsed(3));
        assert_eq!(cs[1], TextCursor::collapsed(to));
    }

    #[test]
    fn shift_alt_keeps_origin_caret_when_its_line_is_too_short() {
        let s = "x\nabcd";
        let from = 0;
        let to = "x\nab".len();
        let cs = gesture_carets(s, from, to, true);
        assert_eq!(cs[0], TextCursor::collapsed(0));
        assert_eq!(
            cs[1],
            TextCursor {
                caret: "x\n".len(),
                anchor: "x\n".len() + 2
            }
        );
    }

    #[test]
    fn splice_preedit_at_two_carets() {
        let s = "ab";
        let anchors = [0, 1];
        assert_eq!(splice_preedit(s, &anchors, "X"), "XaXb");
        assert_eq!(preedit_display_caret(s, &anchors, "X", 0, 1), 1);
        assert_eq!(preedit_display_caret(s, &anchors, "X", 1, 1), 3);
        assert_eq!(
            preedit_display_ranges(s, &anchors, "X"),
            vec![(0, 1), (2, 3)]
        );
        assert_eq!(unsplice_display_hit(s, &anchors, "X", 0), 0);
        assert_eq!(unsplice_display_hit(s, &anchors, "X", 2), 1);
        assert_eq!(unsplice_display_hit(s, &anchors, "X", 4), 2);
    }

    #[test]
    fn follow_latest_keeps_slot_after_insert() {
        let old = vec![TextCursor::collapsed(0), TextCursor::collapsed(1)];
        let mut s = "ab".to_string();
        let new = insert_at_cursors(&mut s, old.clone(), "X");
        assert_eq!(s, "XaXb");
        assert_eq!(follow_latest(&old, &new, 1), 3);
        assert_eq!(follow_latest(&old, &new, 0), 1);
    }

    #[test]
    fn resolve_compose_anchor_prefers_live_latest() {
        let cs = vec![TextCursor::collapsed(0), TextCursor::collapsed(4)];
        assert_eq!(resolve_compose_anchor(&cs, Some(4)), 4);
        assert_eq!(resolve_compose_anchor(&cs, Some(9)), 0);
        assert_eq!(resolve_compose_anchor(&cs, None), 0);
    }
}
