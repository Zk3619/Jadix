//! Shared Input / Textarea display vs layout-measure string derivation.

use crate::components::input::{map_byte_index_to_masked, map_byte_range_to_masked, mask_password};

/// Per-frame (or build-time) store signals for a text field.
pub(crate) struct TextEditStringsInput<'a> {
    pub committed: &'a str,
    pub preedit: &'a str,
    pub preedit_sel: Option<(usize, usize)>,
    pub ime_anchor: Option<usize>,
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
    /// Display-text byte range of the live IME preedit (for the spaced underline).
    pub preedit_underline: Option<(usize, usize)>,
}

pub(crate) fn compose_text_edit_strings(input: TextEditStringsInput<'_>) -> TextEditStrings {
    let real_display = if input.show_placeholder {
        input.placeholder.to_string()
    } else if input.preedit.is_empty() {
        input.committed.to_string()
    } else {
        let mut a = input
            .ime_anchor
            .unwrap_or(input.committed.len())
            .min(input.committed.len());
        a = input.committed.floor_char_boundary(a);
        format!(
            "{}{}{}",
            &input.committed[..a],
            input.preedit,
            &input.committed[a..]
        )
    };

    let layout_measure_text = text_edit_layout_measure_text(
        input.show_placeholder,
        input.placeholder,
        input.committed,
        input.preedit,
    );

    let caret_in_real = if input.is_focused {
        if input.show_placeholder {
            0
        } else if !input.preedit.is_empty() {
            let mut a = input
                .ime_anchor
                .unwrap_or(input.committed.len())
                .min(input.committed.len());
            a = input.committed.floor_char_boundary(a);
            let end = input
                .preedit_sel
                .map(|(_, e)| e)
                .unwrap_or_else(|| input.preedit.len())
                .min(input.preedit.len());
            a + input.preedit.floor_char_boundary(end)
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

    let preedit_underline = if input.show_placeholder || input.preedit.is_empty() {
        None
    } else {
        let mut a = input
            .ime_anchor
            .unwrap_or(input.committed.len())
            .min(input.committed.len());
        a = input.committed.floor_char_boundary(a);
        let end = a + input.preedit.len();
        Some(if input.mask {
            map_byte_range_to_masked(&real_display, a, end, &display_text)
        } else {
            (a, end)
        })
    };

    TextEditStrings {
        real_display,
        display_text,
        layout_measure_text,
        caret_in_real,
        caret_byte,
        preedit_underline,
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
    fn layout_measure_ignores_ime_preedit() {
        let m = text_edit_layout_measure_text(false, "hint", "你好世界", "nihaoshijie");
        assert_eq!(m, "你好世界");
    }

    #[test]
    fn display_includes_preedit_caret_tracks_compose_end() {
        let s = compose_text_edit_strings(TextEditStringsInput {
            committed: "你好",
            preedit: "hao",
            preedit_sel: Some((0, 2)),
            ime_anchor: Some(6),
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
    }
}
