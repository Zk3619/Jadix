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
