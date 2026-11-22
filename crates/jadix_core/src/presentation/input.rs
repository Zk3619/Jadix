//! Shared focus reveal and insertion limits for presentation adapters.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Maximum supported insertion constraint, measured in UTF-16 code units.
pub const MAX_INPUT_LENGTH: usize = 1024 * 1024;

/// Convert a native Unicode-scalar offset (GTK/Pango) to a UTF-8 byte boundary.
/// Out-of-range positions clamp to the end, including native end sentinels.
pub fn scalar_to_byte(text: &str, offset: usize) -> usize {
    text.char_indices().nth(offset).map_or(text.len(), |(at, _)| at)
}

/// Convert a native Unicode-scalar position to the shared UTF-16 coordinate space.
pub fn scalar_to_utf16(text: &str, offset: usize) -> usize {
    text.chars().take(offset).map(char::len_utf16).sum()
}

/// Convert a shared selection to a native scalar position. A split surrogate
/// rounds toward the start; a position beyond the string clamps to its end.
pub fn utf16_to_scalar(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.chars().enumerate() {
        units += ch.len_utf16();
        if units > offset { return index; }
    }
    text.chars().count()
}

/// Longest prefix fitting after replacing a UTF-8 selection. Never cuts an
/// inserted extended grapheme. Invalid ranges/limits return None. An empty
/// result for nonempty insertion means reject the insertion and keep selection;
/// an explicitly empty insertion remains a deletion, including over-limit values.
/// External controlled values and history restoration are not truncated.
pub fn limited_insertion<'a>(
    value: &str,
    selection: Range<usize>,
    insert: &'a str,
    limit: Option<usize>,
) -> Option<&'a str> {
    if selection.start > selection.end
        || selection.end > value.len()
        || !value.is_char_boundary(selection.start)
        || !value.is_char_boundary(selection.end)
        || limit.is_some_and(|n| n > MAX_INPUT_LENGTH)
    {
        return None;
    }
    let Some(limit) = limit else {
        return Some(insert);
    };
    let retained = value[..selection.start].encode_utf16().count()
        + value[selection.end..].encode_utf16().count();
    let mut remaining = limit.saturating_sub(retained);
    for (at, cluster) in insert.grapheme_indices(true) {
        let cost = cluster.encode_utf16().count();
        if cost > remaining {
            return Some(&insert[..at]);
        }
        remaining -= cost;
    }
    Some(insert)
}

/// Scroll delta for a newly focused target in viewport-relative coordinates.
/// A target larger than the viewport stays put while any of it is visible.
/// Fully hidden targets align their start; fitting targets reveal the hidden edge.
/// Empty/invalid viewports or invalid spans do not request a scroll.
/// This is O(1), independent of the number of nodes or virtual rows.
pub fn focus_reveal_delta(start: f64, end: f64, viewport: f64) -> f64 {
    if !start.is_finite()
        || !end.is_finite()
        || !viewport.is_finite()
        || viewport <= 0.0
        || end < start
        || (end - start > viewport && end > 0.0 && start < viewport)
    {
        0.0
    } else if start < 0.0 {
        start
    } else if end > viewport {
        (end - viewport).min(start)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_positions_preserve_supplementary_characters_and_combining_marks() {
        let text = "a🦀e\u{301}中";
        for (scalar, byte, utf16) in [(0,0,0),(1,1,1),(2,5,3),(3,6,4),(4,8,5),(5,11,6)] {
            assert_eq!(scalar_to_byte(text, scalar), byte);
            assert_eq!(scalar_to_utf16(text, scalar), utf16);
            assert_eq!(utf16_to_scalar(text, utf16), scalar);
        }
        assert_eq!(utf16_to_scalar(text, 2), 1);
        assert_eq!(scalar_to_byte(text, usize::MAX), text.len());
        assert_eq!(scalar_to_utf16(text, usize::MAX), 6);
        assert_eq!(utf16_to_scalar(text, usize::MAX), 5);
        assert_eq!(utf16_to_scalar("", 1), 0);
    }
    #[test]
    fn focus_reveal_keeps_visible_oversized_targets_and_reveals_hidden_ones() {
        for (start, end, viewport, expected) in [
            (200., 900., 400., 0.),
            (-2000., 300., 400., 0.),
            (-2000., 900., 400., 0.),
            (500., 1200., 400., 500.),
            (-1200., -500., 400., -1200.),
            (400., 1100., 400., 400.),
            (-700., 0., 400., -700.),
            (-100., 290., 400., -100.),
            (380., 420., 400., 20.),
            (0., 400., 400., 0.),
            (20., 420., 400., 20.),
            (-20., 380., 400., -20.),
            (0., 100., 0., 0.),
            (f64::NAN, 100., 400., 0.),
            (0., f64::INFINITY, 400., 0.),
            (20., 10., 400., 0.),
        ] {
            assert_eq!(focus_reveal_delta(start, end, viewport), expected);
        }
    }

    #[test]
    fn utf16_prefix_preserves_selection_tail_and_whole_graphemes() {
        for (value, range, insert, limit, want) in [
            ("ab", 1..1, "中🦀x", 5, "中🦀"),
            ("ab", 0..1, "👩‍💻x", 6, "👩‍💻"),
            ("ab", 0..1, "👩‍💻x", 5, ""),
            ("", 0..0, "e\u{301}x", 1, ""),
            ("", 0..0, "e\u{301}x", 2, "e\u{301}"),
            ("too long", 3..8, "", 0, ""),
            ("too long", 3..8, "x", 0, ""),
            ("", 0..0, "x", 0, ""),
        ] {
            assert_eq!(
                limited_insertion(value, range, insert, Some(limit)),
                Some(want)
            );
        }
        assert_eq!(limited_insertion("中", 1..2, "x", Some(3)), None);
        assert_eq!(
            limited_insertion("", 0..0, "x", Some(MAX_INPUT_LENGTH + 1)),
            None
        );
        assert_eq!(limited_insertion("", 0..0, "👩‍💻", None), Some("👩‍💻"));
    }
}
