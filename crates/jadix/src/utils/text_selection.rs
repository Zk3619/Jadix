use crate::core::tree::{Element, LayoutStatus};
use crate::style::InputCaretDraw;
use crate::utils::clipboard::set_text as clipboard_set_text;

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
    let mut caret = caret_byte_index.min(text.len());
    while !text.is_char_boundary(caret) {
        caret -= 1;
    }

    let start = text[..caret].rfind('\n').map_or(0, |newline| newline + 1);
    let end = caret + text[caret..].find('\n').unwrap_or(text.len() - caret);
    (start < end).then_some((start, end))
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
        if lo < hi {
            Some((lo, hi))
        } else {
            None
        }
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
        lo < hi && n.text.as_ref().is_some_and(|t| hi <= t.len())
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
    use super::selectable_paragraph_range;

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
    }
}
