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
