use crate::core::tree::{Element, LayoutStatus};
use crate::style::InputCaretDraw;
use crate::utils::clipboard::set_text as clipboard_set_text;
use unicode_segmentation::UnicodeSegmentation;

/// 全局 signal：最近一次静态文本复制的内容（便于单测断言；剪贴板仍走 OS API）。
pub const LABEL_SELECTION_COPY_SIGNAL: &str = "__jadix_label_selection_copy";

pub fn label_selection_signal_key(id: &str) -> String {
    format!("__jadix_label_selection_{}", id)
}

/// 稳定键：`focus_id` 优先，否则 `template_id`；不可选或无文本时返回 `None`。
pub fn selectable_text_key(el: &Element) -> Option<String> {
    if !el.text_style.selectable {
        return None;
    }
    if el.text.as_ref().is_none_or(|t| t.is_empty()) {
        return None;
    }
    if el
        .focus_id
        .as_deref()
        .is_some_and(|id| id.starts_with(crate::utils::text_field::INPUT_FOCUS_PREFIX))
    {
        return None;
    }
    el.focus_id.clone().or_else(|| el.template_id.clone())
}

/// Return the hard-paragraph byte range containing a caret in static selectable text.
/// Newline bytes are excluded; a caret on a newline belongs to the preceding paragraph.
pub fn selectable_paragraph_range(text: &str, caret_byte_index: usize) -> Option<(usize, usize)> {
    let mut caret = text.floor_char_boundary(caret_byte_index.min(text.len()));
    // Both bytes of CRLF belong to the paragraph before the line break.
    if text.as_bytes().get(caret) == Some(&b'\n')
        && caret > 0
        && text.as_bytes()[caret - 1] == b'\r'
    {
        caret -= 1;
    }

    let start = text[..caret].rfind('\n').map_or(0, |newline| newline + 1);
    let mut end = caret + text[caret..].find('\n').unwrap_or(text.len() - caret);
    if end > start && text.as_bytes()[end - 1] == b'\r' {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

/// Unicode word-boundary selection within one hard paragraph, in UTF-8 bytes.
/// At a paragraph's end, select the preceding unit; an empty paragraph has none.
/// Whitespace is a unit too; combining marks and emoji sequences stay intact.
pub fn selectable_word_range(text: &str, caret_byte_index: usize) -> Option<(usize, usize)> {
    let (start, end) = selectable_paragraph_range(text, caret_byte_index)?;
    let caret = text.floor_char_boundary(caret_byte_index.min(end).max(start));
    let probe = if caret == end { end - 1 } else { caret };
    text[start..end]
        .split_word_bound_indices()
        .find(|(offset, word)| start + offset + word.len() > probe)
        .map(|(offset, word)| (start + offset, start + offset + word.len()))
}

/// The initially selected unit stays selected while dragging in either direction.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TextSelectionAnchor {
    range: (usize, usize),
    clicks: u8,
}

impl TextSelectionAnchor {
    pub(crate) fn new(text: &str, caret: usize, clicks: u8) -> Self {
        let caret = text.floor_char_boundary(caret.min(text.len()));
        let range = Self::unit(text, caret, clicks).unwrap_or((caret, caret));
        Self { range, clicks }
    }

    fn unit(text: &str, caret: usize, clicks: u8) -> Option<(usize, usize)> {
        match clicks {
            0 | 1 => None,
            2 => selectable_word_range(text, caret),
            _ => selectable_paragraph_range(text, caret),
        }
    }

    pub(crate) fn range_to(self, text: &str, caret: usize) -> (usize, usize) {
        let snap = |index: usize| text.floor_char_boundary(index.min(text.len()));
        let caret = snap(caret);
        let focus = Self::unit(text, caret, self.clicks).unwrap_or((caret, caret));
        (
            snap(self.range.0).min(focus.0),
            snap(self.range.1).max(focus.1),
        )
    }
}

fn selection_bg_color() -> [u8; 4] {
    InputCaretDraw::default_selection_bg()
}

/// Input 内联文本由 [`crate::utils::text_edit::sync_text_edit_paint_subtree`] 写入 `input_caret`。
/// 这些节点不是可选 Label（`selectable` 为 false → [`selectable_text_key`] 为 `None`），
/// 若在「非 Label」分支里清空 `input_caret`，每帧都会抹掉光标/选区高亮。
fn is_input_owned_text_node(el: &Element) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|t| t.starts_with("__jadix_input:") && t.ends_with(":text"))
}

fn sync_one_label_selection(el: &mut Element, impact: &mut LayoutStatus) {
    let Some(key) = selectable_text_key(el) else {
        if is_input_owned_text_node(el) {
            return;
        }
        if el.text_style.input_caret.is_some() {
            el.text_style.input_caret = None;
            *impact = LayoutStatus::StyleChanged;
        }
        return;
    };
    let sk = label_selection_signal_key(&key);
    let sel = crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).peek();
    let sel_draw = sel.and_then(|(a, b)| {
        let lo = a.min(b);
        let hi = a.max(b);
        (lo < hi
            && el
                .text
                .as_ref()
                .is_some_and(|text| text.get(lo..hi).is_some()))
        .then_some((lo, hi))
    });
    let want = sel_draw.map(|range| InputCaretDraw {
        caret_byte_index: 0,
        selection: Some(range),
        selection_bg: selection_bg_color(),
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: Some(InputCaretDraw::default_selection_foreground()),
    });
    if el.text_style.input_caret != want {
        el.text_style.input_caret = want;
        *impact = LayoutStatus::StyleChanged;
    }
}

/// 每帧将可选 Label 的选区信号同步到 `text_style.input_caret`（`peek`，不触发 rebuild）。
pub fn sync_label_selection_paint_subtree(el: &mut Element, impact: &mut LayoutStatus) {
    crate::core::tree::walk_elements_mut(el, |n| sync_one_label_selection(n, impact));
}

/// 复制首个非空静态文本选区到剪贴板与 [`LABEL_SELECTION_COPY_SIGNAL`]。
pub fn try_copy_label_selection(root: &Element) -> bool {
    try_copy_label_selection_subtree(root)
}

fn try_copy_label_selection_subtree(el: &Element) -> bool {
    crate::core::tree::find_in_tree(el, |n| {
        let Some(key) = selectable_text_key(n) else {
            return false;
        };
        let sk = label_selection_signal_key(&key);
        let Some((a, b)) =
            crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).peek()
        else {
            return false;
        };
        let lo = a.min(b);
        let hi = a.max(b);
        lo < hi && n.text.as_ref().is_some_and(|t| t.get(lo..hi).is_some())
    })
    .is_some_and(|n| {
        let key = selectable_text_key(n).expect("pred matched");
        let sk = label_selection_signal_key(&key);
        let (a, b) = crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>)
            .peek()
            .expect("pred matched");
        let lo = a.min(b);
        let hi = a.max(b);
        let slice = &n.text.as_ref().expect("pred matched")[lo..hi];
        clipboard_set_text(slice);
        crate::core::reactivity::use_signal(LABEL_SELECTION_COPY_SIGNAL, String::new)
            .set(slice.to_string());
        true
    })
}

/// Label-to-control association attribute used by pointer and accessibility routing.
pub const ATTR_LABEL_FOR: &str = "for";

#[cfg(test)]
mod tests {
    use super::{TextSelectionAnchor, selectable_paragraph_range, selectable_word_range};

    #[test]
    fn word_selection_preserves_unicode_units_and_hard_breaks() {
        for (text, needle, offset, expected) in [
            ("hello-world", "world", 2, "world"),
            ("can't stop", "can't", 3, "can't"),
            ("foo_bar", "_", 0, "foo_bar"),
            ("e\u{301}clair", "\u{301}", 1, "e\u{301}clair"),
            ("a 👩🏽‍💻 b", "🏽", 2, "👩🏽‍💻"),
            ("a 🇨🇳 b", "🇳", 1, "🇨🇳"),
            ("你好，world", "好", 1, "好"),
            ("hello  world", "  ", 1, "  "),
            ("old\r\nnew", "\r", 0, "old"),
            ("old\r\nnew", "\n", 0, "old"),
        ] {
            let caret = text.find(needle).unwrap() + offset;
            let (start, end) = selectable_word_range(text, caret).unwrap();
            assert_eq!(&text[start..end], expected, "{text:?} at {caret}");
        }
        assert_eq!(selectable_word_range("last", usize::MAX), Some((0, 4)));
        for (text, caret) in [("", 0), ("a\n\nb", 2), ("a\r\n\r\nb", 4), ("a\n", 2)] {
            assert_eq!(selectable_word_range(text, caret), None);
        }
    }

    #[test]
    fn unit_drag_keeps_anchor_whole_and_extends_in_both_directions() {
        let text = "one two three\r\n末尾";
        let word = TextSelectionAnchor::new(text, 5, 2);
        for (caret, expected) in [(4, "two"), (0, "one two"), (10, "two three"), (6, "two")] {
            let (start, end) = word.range_to(text, caret);
            assert_eq!(&text[start..end], expected);
        }
        let paragraph = TextSelectionAnchor::new(text, text.len(), 3);
        assert_eq!(paragraph.range_to(text, 0), (0, text.len()));
        assert_eq!(paragraph.range_to(text, text.len()), (15, text.len()));
        let character = TextSelectionAnchor::new(text, 5, 1);
        assert_eq!(character.range_to(text, 1), (1, 5));
        assert_eq!(character.range_to(text, 10), (5, 10));
        // A dynamic text replacement during a gesture must never yield invalid UTF-8 slices.
        for clicks in [1, 2, 3] {
            let anchor = TextSelectionAnchor::new(text, text.len(), clicks);
            let (start, end) = anchor.range_to("🙂", usize::MAX);
            assert!("🙂".get(start..end).is_some());
        }
    }

    #[test]
    fn paragraph_selection_uses_utf8_byte_ranges_and_excludes_newlines() {
        let text = "第一段\nsecond\n末尾";
        let first_end = "第一段".len();
        let second_start = first_end + 1;
        let second_end = second_start + "second".len();

        assert_eq!(selectable_paragraph_range(text, 0), Some((0, first_end)));
        assert_eq!(
            selectable_paragraph_range(text, first_end),
            Some((0, first_end))
        );
        assert_eq!(
            selectable_paragraph_range(text, second_start),
            Some((second_start, second_end))
        );
        assert_eq!(
            selectable_paragraph_range(text, text.len()),
            Some((second_end + 1, text.len()))
        );
    }

    #[test]
    fn paragraph_selection_skips_empty_paragraphs_and_clamps_the_caret() {
        assert_eq!(selectable_paragraph_range("\ntext", 0), None);
        assert_eq!(selectable_paragraph_range("text\n", usize::MAX), None);
        assert_eq!(selectable_paragraph_range("text", usize::MAX), Some((0, 4)));
        for caret in [0, 1, 4, 5] {
            assert_eq!(
                selectable_paragraph_range("text\r\nnext", caret),
                Some((0, 4))
            );
        }
        assert_eq!(selectable_paragraph_range("\r\nnext", 1), None);
        assert_eq!(selectable_paragraph_range("text\r\nnext", 6), Some((6, 10)));
    }
}
