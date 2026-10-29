use super::*;
use crate::core::reactivity::create_root;

#[test]
fn cjk_chrome_band_pads_ideograph_equally() {
    // Microsoft YaHei hhea + U+4E2D ink (font units / 2048).
    let fs = 14.0_f32;
    let lh = 21.0_f32;
    let ascent = 2167.0 / 2048.0;
    let descent = 536.0 / 2048.0;
    let above = 1707.0 / 2048.0;
    let below = 277.0 / 2048.0;
    let (y, h) = chrome_band_around_ink(lh, fs, ascent, descent, above, below);
    let baseline = (lh - (ascent + descent) * fs) * 0.5 + ascent * fs;
    let ink_top = baseline - above * fs;
    let ink_bot = baseline + below * fs;
    let gap_above = ink_top - y;
    let gap_below = (y + h) - ink_bot;
    assert!(
        (gap_above - gap_below).abs() < 0.05,
        "gaps {gap_above} vs {gap_below} (y={y} h={h})"
    );
    assert!(y >= -0.01 && y + h <= lh + 0.01);
    assert!(
        h < lh - 0.5,
        "band should trim the uneven side, h={h} lh={lh}"
    );
}

#[test]
fn cjk_probe_band_is_symmetric_when_face_present() {
    let Some((ascent, descent, above, below)) = crate::text::measure::ideograph_vertical_em()
    else {
        #[cfg(windows)]
        if std::path::Path::new(r"C:\Windows\Fonts\msyh.ttc").is_file()
            || std::path::Path::new(r"C:\Windows\Fonts\msyh.ttf").is_file()
        {
            panic!("YaHei is installed but U+4E2D ink was not measured");
        }
        return;
    };
    let fs = 14.0_f32;
    let lh = 21.0_f32;
    let (y, h) = chrome_band_around_ink(lh, fs, ascent, descent, above, below);
    let baseline = (lh - (ascent + descent) * fs) * 0.5 + ascent * fs;
    let ink_top = baseline - above * fs;
    let ink_bot = baseline + below * fs;
    let gap_above = ink_top - y;
    let gap_below = (y + h) - ink_bot;
    assert!(
            (gap_above - gap_below).abs() < 0.05,
            "probed gaps {gap_above} vs {gap_below} ascent={ascent} descent={descent} above={above} below={below}"
        );
    assert!(y >= -0.01 && y + h <= lh + 0.01);
    assert!(h < lh - 0.5, "probed band should trim the line box, h={h}");
}

#[test]
fn focus_prefixes() {
    assert_eq!(input_focus_id("a"), "input:a");
    assert_eq!(textarea_focus_id("a"), "textarea:a");
    assert_eq!(text_edit_rest("textarea:bio"), Some("bio"));
    assert_eq!(focus_id_for_text_edit("x", true), "textarea:x");
}

#[test]
fn pull_controlled_into_store_after_external_dispose_must_not_peek() {
    use crate::core::reactivity::use_scoped_signal;

    let field = "tf_disposed_ext";
    let (owner, ()) = create_root(|| {
        let ext = use_scoped_signal("tf_disposed_ext_val", || "hello".to_string());
        bind_controlled_value(field, ext);
    });
    owner.dispose();
    assert!(
        pull_controlled_into_store(field, "hello", false).is_none(),
        "paint sync must unbind a disposed controlled Signal instead of peeking it"
    );
    unbind_controlled_value(field);
}

#[test]
fn commit_mirrors_controlled() {
    let (_owner, ()) = create_root(|| {
        let ext = use_signal("tf_ext", || "seed".to_string());
        bind_controlled_value("tf1", ext);
        assert_eq!(
            use_signal(&input_value_signal_key("tf1"), String::new).peek(),
            "seed"
        );
        commit_value("tf1", "typed".into());
        assert_eq!(ext.peek(), "typed");
    });
}

#[test]
fn external_set_pulls_into_store_when_unfocused() {
    let (_owner, ()) = create_root(|| {
        let ext = use_signal("tf_ext_pull", || "seed".to_string());
        bind_controlled_value("tf_pull", ext);
        ext.set("hello".into());
        let next = pull_controlled_into_store("tf_pull", "seed", false);
        assert_eq!(next.as_deref(), Some("hello"));
        assert_eq!(
            use_signal(&input_value_signal_key("tf_pull"), String::new).peek(),
            "hello"
        );
        assert!(pull_controlled_into_store("tf_pull", "hello", true).is_none());
    });
}

#[test]
fn composing_external_set_does_not_pull_even_when_unfocused() {
    let (_owner, ()) = create_root(|| {
        let ext = use_signal("tf_ext_ime", || "seed".to_string());
        bind_controlled_value("tf_ime", ext);
        use_signal(&input_preedit_signal_key("tf_ime"), String::new).set("ni".into());
        ext.set("hello".into());
        assert!(
            pull_controlled_into_store("tf_ime", "seed", false).is_none(),
            "IME preedit must block external pull (do not steal caret)"
        );
        assert_eq!(
            use_signal(&input_value_signal_key("tf_ime"), String::new).peek(),
            "seed"
        );
    });
}

#[test]
fn focused_same_value_external_set_does_not_move_caret() {
    let (_owner, ()) = create_root(|| {
        let ext = use_signal("tf_ext_caret", || "hello".to_string());
        bind_controlled_value("tf_caret", ext);
        use_signal(&input_caret_signal_key("tf_caret"), || None::<usize>).set(Some(2));
        use_signal(&input_selection_signal_key("tf_caret"), || {
            None::<(usize, usize)>
        })
        .set(Some((1, 4)));
        ext.set("hello".into());
        assert!(
            pull_controlled_into_store("tf_caret", "hello", true).is_none(),
            "focused same-value set must not pull"
        );
        assert_eq!(
            use_signal(&input_caret_signal_key("tf_caret"), || None::<usize>).peek(),
            Some(2)
        );
        assert_eq!(
            use_signal(&input_selection_signal_key("tf_caret"), || {
                None::<(usize, usize)>
            })
            .peek(),
            Some((1, 4))
        );
    });
}

#[test]
fn select_all_in_text_edit_covers_value_and_parks_caret_at_start() {
    let (_owner, ()) = create_root(|| {
        let id = "sel_all_tab";
        use_signal(&input_value_signal_key(id), String::new).set("hello".into());
        use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(5));
        use_signal(&input_scroll_x_signal_key(id), || 0.0_f32).set(40.0);
        assert!(select_all_in_text_edit(&input_focus_id(id)));
        assert_eq!(
            use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).peek(),
            Some((0, 5))
        );
        assert_eq!(
            use_signal(&input_caret_signal_key(id), || None::<usize>).peek(),
            Some(0)
        );
        assert_eq!(
            use_signal(&input_scroll_x_signal_key(id), || 0.0_f32).peek(),
            0.0
        );
        assert!(!select_all_in_text_edit("button:x"));
    });
}

#[test]
fn caret_after_newline_maps_to_next_line_start() {
    assert_eq!(caret_line_and_local("a\n", 2), (1, 0));
    assert_eq!(caret_line_and_local("a\nb", 2), (1, 0));
    assert_eq!(caret_line_and_local("a\nb", 3), (1, 1));
    assert_eq!(caret_line_and_local("a", 1), (0, 1));
}

#[test]
fn ensure_caret_visible_scrolls_when_below_viewport() {
    // line 4 at y=80..100, viewport 50 tall scrolled to 0 → need sy=50
    let text = "a\nb\nc\nd\ne";
    let caret = text.len();
    let new_sy =
        ensure_caret_visible_scroll_y(text, caret, 20.0, 50.0, 0.0, 50.0, Default::default());
    assert_eq!(new_sy, Some(50.0));
    assert_eq!(
        ensure_caret_visible_scroll_y(text, caret, 20.0, 50.0, 50.0, 50.0, Default::default()),
        None
    );
}

#[test]
fn ensure_caret_visible_textarea_insets_include_pad_and_last_line_ink() {
    let text = "a\nb\nc\nd\ne";
    let caret = text.len();
    let lh = 20.0;
    let vh = 50.0;
    let insets = TextareaCaretScrollInsets {
        pad_top: 8.0,
        pad_bottom: 8.0,
        ink_bottom: 4.0,
    };
    // Last line bottom in content space: 8 + 5*20 + 4 = 112 → sy = 62
    assert_eq!(
        ensure_caret_visible_scroll_y(text, caret, lh, vh, 0.0, 80.0, insets),
        Some(62.0)
    );
    // Without insets (input path): sy = 50
    assert_eq!(
        ensure_caret_visible_scroll_y(text, caret, lh, vh, 0.0, 80.0, Default::default()),
        Some(50.0)
    );
}

#[test]
fn ensure_caret_visible_skips_when_viewport_unready() {
    let text = "a\nb";
    // Unready viewport still clears a stale scroll offset (gallery squash).
    assert_eq!(
        ensure_caret_visible_scroll_y(text, text.len(), 20.0, 0.0, 40.0, 0.0, Default::default()),
        Some(0.0)
    );
    assert_eq!(
        ensure_caret_visible_scroll_y(text, text.len(), 20.0, 5.0, 0.0, 0.0, Default::default()),
        None
    );
}

#[test]
fn ensure_caret_visible_clears_stale_sy_when_content_fits() {
    let text = "a\nb";
    assert_eq!(
        ensure_caret_visible_scroll_y(text, text.len(), 20.0, 100.0, 40.0, 0.0, Default::default()),
        Some(0.0)
    );
}

#[test]
fn content_height_grows_with_hard_newlines_and_ink() {
    let size = 14.0;
    let lh = text_edit_line_box_px(size, None, None);
    let h1 = text_edit_content_height_px("啊", size, 400, lh, 400.0);
    let h2 = text_edit_content_height_px("啊\n啊", size, 400, lh, 400.0);
    assert!(h1 >= lh && h1 <= lh + text_edit_ink_pad_px(size) + 1.0);
    assert!(h2 > h1 + lh * 0.9, "h1={h1} h2={h2} lh={lh}");
}

#[test]
fn ensure_caret_visible_scroll_x_pans_when_caret_past_fold() {
    // viewport 40, content 100, caret at 90, scroll 0 → need ~52 (90+2-40)
    assert!((ensure_caret_visible_scroll_x(90.0, 100.0, 40.0, 0.0) - 52.0).abs() < 0.01);
    // already showing caret
    assert!((ensure_caret_visible_scroll_x(90.0, 100.0, 40.0, 52.0) - 52.0).abs() < 0.01);
    // caret moved back near the head → snap to 0 so the first glyph is not clipped
    assert_eq!(ensure_caret_visible_scroll_x(5.0, 100.0, 40.0, 52.0), 0.0);
    assert_eq!(ensure_caret_visible_scroll_x(0.0, 100.0, 40.0, 52.0), 0.0);
    // content fits
    assert_eq!(ensure_caret_visible_scroll_x(10.0, 30.0, 40.0, 12.0), 0.0);
    // caret at content end: pan 2px past text so the 2px caret is inside the viewport
    let end = ensure_caret_visible_scroll_x(100.0, 100.0, 40.0, 0.0);
    assert!((end - 62.0).abs() < 0.01, "end caret scroll={end}");
    assert!(
        100.0 - end <= 40.0 - 1.5,
        "caret must sit inside the viewport"
    );
}

#[test]
fn ensure_caret_visible_scroll_x_caps_at_current_line_end() {
    let pad_left = 12.0;
    let line_end = 300.0;
    let vw = 200.0;
    let caret = pad_left + line_end;
    let cap = pad_left + line_end;
    let want = nowrap_line_end_max_scroll_x(line_end, vw, pad_left);
    assert!((want - 114.0).abs() < 0.01, "want={want}");
    let from_zero = ensure_caret_visible_scroll_x(caret, cap, vw, 0.0);
    assert!((from_zero - want).abs() < 0.01, "from_zero={from_zero}");
    // Stale offset from a longer sibling line must snap back to this line's end.
    let from_overscroll = ensure_caret_visible_scroll_x(caret, cap, vw, 400.0);
    assert!(
        (from_overscroll - want).abs() < 0.01,
        "from_overscroll={from_overscroll}"
    );
    assert!(
        caret + TEXT_EDIT_CARET_W - from_zero <= vw + 0.01,
        "caret must sit fully inside the viewport"
    );
    assert_eq!(
        hard_line_body_at_y("short\nlong line here", 5.0, 21.0),
        "short"
    );
    assert_eq!(
        hard_line_body_at_y("short\nlong line here", 25.0, 21.0),
        "long line here"
    );
}

#[test]
fn clamp_caret_scroll_x_drops_pan_when_viewport_grows() {
    assert_eq!(
        clamp_caret_scroll_x(400.0, 80.0, 322.0),
        max_caret_scroll_x(400.0, 80.0)
    );
    let after_grow = clamp_caret_scroll_x(400.0, 320.0, 322.0);
    assert!(
        after_grow < 90.0,
        "widening the clip must drop leftover pan, got {after_grow}"
    );
    assert_eq!(clamp_caret_scroll_x(400.0, 420.0, 322.0), 0.0);
}
