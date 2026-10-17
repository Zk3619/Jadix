//! Shared Input / Textarea display vs layout-measure string derivation.

use crate::components::input::{map_byte_index_to_masked, map_byte_range_to_masked, mask_password};

pub(super) fn finish_selection_draw(
    draw: Option<crate::style::InputCaretDraw>,
    focused: bool,
    policy: crate::utils::text_field::SelectionOnBlur,
    text_color: [u8; 4],
) -> Option<crate::style::InputCaretDraw> {
    if focused {
        return draw;
    }
    let mut draw = draw?;
    if policy != crate::utils::text_field::SelectionOnBlur::Retain
        || (draw.selection.is_none() && draw.extra_selections.is_empty())
    {
        return None;
    }
    draw.selection_bg[3] = (draw.selection_bg[3] as f32 * 0.25).round() as u8;
    draw.selection_foreground = Some(text_color);
    draw.caret_color = Some([0; 4]);
    draw.extra_carets.clear();
    draw.overwrite_highlight = None;
    draw.drop_caret = false;
    draw.preedit_underline = None;
    draw.extra_preedit_underlines.clear();
    draw.preedit_ul.clear();
    Some(draw)
}

/// Per-frame (or build-time) store signals for a text field.
pub(crate) struct TextEditStringsInput<'a> {
    pub committed: &'a str,
    pub preedit: &'a str,
    pub preedit_sel: Option<(usize, usize)>,
    /// `(start, end, style)` in preedit bytes. Empty → one solid underline.
    pub preedit_marks: &'a [(usize, usize, u8)],
    pub ime_anchor: Option<usize>,
    /// Extra Alt+click carets as `(caret, anchor)` in committed bytes.
    pub extras: &'a [(usize, usize)],
    /// Primary caret in committed bytes (also a compose insertion point).
    pub committed_caret: Option<usize>,
    pub placeholder: &'a str,
    pub show_placeholder: bool,
    pub mask: bool,
    pub is_focused: bool,
    /// Caret from the caret signal when not composing (not selection-end).
    /// Tab select-all parks caret at 0 so overflow viewports stay on the line head.
    pub caret_override: Option<usize>,
}

/// Derived strings + caret indices shared by [`super::build`] and [`super::paint`].
pub(crate) struct TextEditStrings {
    pub real_display: String,
    pub display_text: String,
    /// Soft-wrap / leaf height must use this during IME — Latin preedit syllables must
    /// not temporarily inflate line count (textarea vertical jitter on CJK compose).
    pub layout_measure_text: String,
    pub caret_in_real: usize,
    pub caret_byte: usize,
    /// Winit `Preedit(_, None)` hides the caret while keeping inline candidates.
    pub caret_hidden: bool,
    /// Display-text byte range of the live IME preedit (for the spaced underline).
    pub preedit_underline: Option<(usize, usize)>,
    pub extra_carets: Vec<usize>,
    pub extra_selections: Vec<(usize, usize)>,
    pub extra_preedit_underlines: Vec<(usize, usize)>,
    /// Display-text runs `(start, end, style)` for Japanese clause underlines.
    pub preedit_ul: Vec<(usize, usize, u8)>,
}

pub(crate) fn compose_text_edit_strings(input: TextEditStringsInput<'_>) -> TextEditStrings {
    use crate::utils::text_multi_cursor::{
        preedit_display_caret, preedit_display_ranges, splice_preedit,
    };

    let compose_anchors = {
        let mut a = Vec::with_capacity(1 + input.extras.len());
        if !input.preedit.is_empty() && !input.show_placeholder {
            let primary = input
                .ime_anchor
                .unwrap_or(input.committed.len())
                .min(input.committed.len());
            a.push(input.committed.floor_char_boundary(primary));
            if let Some(c) = input.committed_caret {
                a.push(
                    input
                        .committed
                        .floor_char_boundary(c.min(input.committed.len())),
                );
            }
            for &(c, anc) in input.extras {
                a.push(
                    input
                        .committed
                        .floor_char_boundary(c.min(input.committed.len())),
                );
                a.push(
                    input
                        .committed
                        .floor_char_boundary(anc.min(input.committed.len())),
                );
            }
            a.sort_unstable();
            a.dedup();
        }
        a
    };

    let real_display = if input.show_placeholder {
        input.placeholder.to_string()
    } else if input.preedit.is_empty() {
        input.committed.to_string()
    } else {
        splice_preedit(input.committed, &compose_anchors, input.preedit)
    };

    let layout_measure_text = text_edit_layout_measure_text(
        input.show_placeholder,
        input.placeholder,
        input.committed,
        input.preedit,
    );

    let ime_anchor_byte = if input.preedit.is_empty() {
        0
    } else {
        input.committed.floor_char_boundary(
            input
                .ime_anchor
                .unwrap_or(input.committed.len())
                .min(input.committed.len()),
        )
    };

    let caret_in_real = if input.is_focused {
        if input.show_placeholder {
            0
        } else if !input.preedit.is_empty() {
            // IMEs may report a reversed cursor/highlight range (Smart Pinyin
            // sends (18, 6) while revising a six-Han preedit). A hairline belongs
            // at its trailing edge, not whichever endpoint happened to be last.
            let end = input
                .preedit_sel
                .map(|(a, b)| a.max(b))
                .unwrap_or_else(|| input.preedit.len())
                .min(input.preedit.len());
            preedit_display_caret(
                input.committed,
                &compose_anchors,
                input.preedit,
                ime_anchor_byte,
                input.preedit.floor_char_boundary(end),
            )
        } else {
            let len = real_display.len();
            let ix = input.caret_override.unwrap_or(len);
            real_display.floor_char_boundary(ix.min(len))
        }
    } else {
        0
    };

    let display_text = if input.mask {
        mask_password(&real_display)
    } else {
        real_display.clone()
    };
    let caret_byte = if input.mask {
        map_byte_index_to_masked(&real_display, caret_in_real, &display_text)
    } else {
        caret_in_real
    };

    let mut extra_carets = Vec::new();
    let mut extra_selections = Vec::new();
    let mut extra_preedit_underlines = Vec::new();
    let mut preedit_ul = Vec::new();
    let preedit_underline = if input.show_placeholder || input.preedit.is_empty() {
        for &(c, a) in input.extras {
            let caret_i = if input.mask {
                map_byte_index_to_masked(&real_display, c, &display_text)
            } else {
                real_display.floor_char_boundary(c.min(real_display.len()))
            };
            extra_carets.push(caret_i);
            let lo = a.min(c);
            let hi = a.max(c);
            if lo < hi {
                extra_selections.push(if input.mask {
                    map_byte_range_to_masked(&real_display, lo, hi, &display_text)
                } else {
                    (
                        real_display.floor_char_boundary(lo.min(real_display.len())),
                        real_display.floor_char_boundary(hi.min(real_display.len())),
                    )
                });
            }
        }
        None
    } else {
        let ranges = preedit_display_ranges(input.committed, &compose_anchors, input.preedit);
        let ime_range = ranges.iter().copied().find(|&(s, _)| {
            preedit_display_caret(
                input.committed,
                &compose_anchors,
                input.preedit,
                ime_anchor_byte,
                0,
            ) == s
        });
        let preedit_caret = input
            .preedit_sel
            .map(|(a, b)| a.max(b))
            .unwrap_or_else(|| input.preedit.len())
            .min(input.preedit.len());
        let preedit_caret = input.preedit.floor_char_boundary(preedit_caret);
        for &(start, end) in &ranges {
            let mapped = if input.mask {
                map_byte_range_to_masked(&real_display, start, end, &display_text)
            } else {
                (start, end)
            };
            for &(ps, pe, kind) in input.preedit_marks {
                push_preedit_mark(
                    &mut preedit_ul,
                    start,
                    input.preedit.len(),
                    ps,
                    pe,
                    kind,
                    input.mask,
                    &real_display,
                    &display_text,
                );
            }
            if Some((start, end)) == ime_range {
                continue;
            }
            extra_preedit_underlines.push(mapped);
            extra_carets.push(if input.mask {
                map_byte_index_to_masked(&real_display, start + preedit_caret, &display_text)
            } else {
                start + preedit_caret
            });
        }
        ime_range.map(|(s, e)| {
            if input.mask {
                map_byte_range_to_masked(&real_display, s, e, &display_text)
            } else {
                (s, e)
            }
        })
    };

    TextEditStrings {
        real_display,
        display_text,
        layout_measure_text,
        caret_in_real,
        caret_byte,
        caret_hidden: !input.preedit.is_empty() && input.preedit_sel.is_none(),
        preedit_underline,
        extra_carets,
        extra_selections,
        extra_preedit_underlines,
        preedit_ul,
    }
}

fn push_preedit_mark(
    out: &mut Vec<(usize, usize, u8)>,
    range_start: usize,
    preedit_len: usize,
    ps: usize,
    pe: usize,
    kind: u8,
    mask: bool,
    real: &str,
    display: &str,
) {
    let ps = ps.min(preedit_len);
    let pe = pe.min(preedit_len);
    if ps >= pe {
        return;
    }
    let ds = range_start + ps;
    let de = range_start + pe;
    if mask {
        let a = map_byte_index_to_masked(real, ds, display);
        let b = map_byte_index_to_masked(real, de, display);
        if a < b {
            out.push((a, b, kind));
        }
    } else if de <= real.len() {
        out.push((ds, de, kind));
    }
}

/// Text used for multiline soft-wrap height / scroll content sizing.
///
/// Single-line Input pins one line box — this only matters for Textarea. During IME
/// composition, paint still shows `preedit` in `display_text`, but layout measure
/// stays on committed (or placeholder) so transient Latin syllables do not soft-wrap
/// into extra visual lines.
pub(crate) fn text_edit_layout_measure_text(
    show_placeholder: bool,
    placeholder: &str,
    committed: &str,
    preedit: &str,
) -> String {
    if show_placeholder {
        placeholder.to_string()
    } else if preedit.is_empty() {
        committed.to_string()
    } else {
        committed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversed_ime_cursor_range_keeps_caret_at_trailing_edge() {
        // Smart Pinyin replay: candidates change in the middle while the
        // composing cursor still spans to the end (UTF-8 bytes, not chars).
        for (preedit, cursor) in [
            ("甲", (3, 3)),
            ("甲乙", (6, 6)),
            ("甲丙丁", (9, 3)),
            ("甲丙丁戊己庚", (18, 6)),
        ] {
            let s = compose_text_edit_strings(TextEditStringsInput {
                committed: "前后",
                preedit,
                preedit_sel: Some(cursor),
                preedit_marks: &[],
                ime_anchor: Some(3),
                extras: &[(6, 6)],
                committed_caret: Some(3),
                placeholder: "",
                show_placeholder: false,
                mask: false,
                is_focused: true,
                caret_override: None,
            });
            assert_eq!(s.caret_in_real, 3 + preedit.len());
            assert_eq!(s.extra_carets, vec![6 + 2 * preedit.len()]);
            assert!(!s.caret_hidden);
        }
    }

    #[test]
    fn layout_measure_ignores_ime_preedit() {
        let m = text_edit_layout_measure_text(false, "hint", "你好世界", "nihaoshijie");
        assert_eq!(m, "你好世界");
    }

    #[test]
    fn hidden_preedit_caret_keeps_end_geometry_for_ime_anchor() {
        let committed = "가나";
        let preedit = "스";
        let anchor = committed.len();
        let s = compose_text_edit_strings(TextEditStringsInput {
            committed,
            preedit,
            preedit_sel: None,
            preedit_marks: &[],
            ime_anchor: Some(anchor),
            extras: &[],
            committed_caret: Some(anchor),
            placeholder: "",
            show_placeholder: false,
            mask: false,
            is_focused: true,
            caret_override: None,
        });
        assert_eq!(s.real_display, format!("{committed}{preedit}"));
        assert_eq!(s.caret_in_real, anchor + preedit.len());
        assert!(s.caret_hidden);
        assert_eq!(s.preedit_underline, Some((anchor, anchor + preedit.len())));
    }

    #[test]
    fn display_includes_preedit_caret_tracks_compose_end() {
        let s = compose_text_edit_strings(TextEditStringsInput {
            committed: "你好",
            preedit: "hao",
            preedit_sel: Some((0, 2)),
            preedit_marks: &[],
            ime_anchor: Some(6),
            extras: &[],
            committed_caret: Some(6),
            placeholder: "",
            show_placeholder: false,
            mask: false,
            is_focused: true,
            caret_override: None,
        });
        assert_eq!(s.real_display, "你好hao");
        assert_eq!(s.layout_measure_text, "你好");
        assert_eq!(s.caret_in_real, 6 + "ha".len());
        assert_eq!(s.preedit_underline, Some((6, 6 + "hao".len())));
        assert!(s.extra_carets.is_empty());
        assert!(s.extra_preedit_underlines.is_empty());
    }

    #[test]
    fn display_splices_preedit_at_extra_carets() {
        let extras = [(1, 1)];
        let s = compose_text_edit_strings(TextEditStringsInput {
            committed: "ab",
            preedit: "X",
            preedit_sel: Some((0, 1)),
            preedit_marks: &[],
            ime_anchor: Some(0),
            extras: &extras,
            committed_caret: Some(0),
            placeholder: "",
            show_placeholder: false,
            mask: false,
            is_focused: true,
            caret_override: None,
        });
        assert_eq!(s.real_display, "XaXb");
        assert_eq!(s.caret_in_real, 1);
        assert_eq!(s.preedit_underline, Some((0, 1)));
        assert_eq!(s.extra_carets, vec![3]);
        assert_eq!(s.extra_preedit_underlines, vec![(2, 3)]);
    }
}
