use super::*;

#[test]
fn password_commit_is_excluded_from_diagnostics() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let rest = "private-password-commit";
        record_ime_commit("ordinary-input", "visible");
        crate::core::reactivity::use_signal(
            &crate::utils::text_field::input_type_signal_key(rest),
            String::new,
        )
        .set("password".into());
        record_ime_commit(rest, "pāss字");
        assert!(take_last_ime_commit().is_none());
    });
}

#[test]
fn preedit_hidden_cursor_and_windows_missing_position_are_distinct() {
    let text = "你好";
    assert_eq!(resolve_preedit_chrome(text, None, false, None, &[]).0, None);
    assert_eq!(
        resolve_preedit_chrome(text, None, false, Some(None), &[]).0,
        Some((text.len(), text.len()))
    );
    assert_eq!(
        resolve_preedit_chrome(text, Some((0, 0)), true, None, &[]).0,
        Some((text.len(), text.len()))
    );
}

#[test]
fn missing_composition_caret_is_not_byte_zero() {
    assert_eq!(map_preedit_cursor(Some((0, 0)), true), None);
    assert_eq!(map_preedit_cursor(Some((0, 3)), true), None);
    assert_eq!(map_preedit_cursor(Some((0, 0)), false), Some((0, 0)));
    assert_eq!(map_preedit_cursor(Some((1, 4)), false), Some((1, 4)));
    assert_eq!(map_preedit_cursor(None, false), None);
}

#[test]
fn japanese_target_clause_is_not_the_caret() {
    // winit encodes the thick target clause as the cursor range. After the
    // first character is converted, that range ends after it. The caret is
    // GCS_CURSORPOS (here: end of the string still being typed).
    let text = "あああ";
    let winit_clause = Some((0, "あ".len()));
    let attrs = [ATTR_TARGET_CONVERTED, ATTR_CONVERTED, ATTR_INPUT];
    let (caret, marks) = resolve_preedit_chrome(
        text,
        winit_clause,
        false,
        Some(Some(text.chars().count())),
        &attrs,
    );
    assert_eq!(caret, Some((text.len(), text.len())));
    assert_eq!(
        marks,
        vec![
            (0, "あ".len(), PREEDIT_UL_THICK),
            ("あ".len(), "ああ".len(), PREEDIT_UL_THIN),
            ("ああ".len(), text.len(), PREEDIT_UL_WAVY),
        ]
    );
}

#[test]
fn japanese_kana_being_typed_is_wavy() {
    let text = "ああ";
    let attrs = [ATTR_INPUT, ATTR_INPUT];
    let marks = preedit_underline_spans(text, &attrs);
    assert_eq!(marks, vec![(0, text.len(), PREEDIT_UL_WAVY)]);
}

#[test]
fn chinese_pinyin_keeps_a_single_solid_line() {
    let marks = preedit_underline_spans("nihao", &[ATTR_INPUT; 5]);
    assert!(marks.is_empty());
}

#[test]
fn text_edit_rest_input_and_textarea() {
    assert_eq!(text_edit_rest("input:foo"), Some("foo"));
    assert_eq!(text_edit_rest("textarea:bio"), Some("bio"));
    assert_eq!(text_edit_rest("button:x"), None);
}

#[test]
fn visual_fully_outside_window_edges() {
    let win = (400.0, 300.0);
    assert!(!visual_fully_outside_window(
        [10.0, 10.0, 80.0, 24.0],
        win.0,
        win.1
    ));
    assert!(visual_fully_outside_window(
        [10.0, -30.0, 80.0, 24.0],
        win.0,
        win.1
    )); // above
    assert!(visual_fully_outside_window(
        [10.0, 300.0, 80.0, 24.0],
        win.0,
        win.1
    )); // below
    assert!(visual_fully_outside_window(
        [-90.0, 10.0, 80.0, 24.0],
        win.0,
        win.1
    )); // left
    assert!(visual_fully_outside_window(
        [400.0, 10.0, 80.0, 24.0],
        win.0,
        win.1
    )); // right
        // Still intersecting by 1px → not fully outside (focus stays either way).
    assert!(!visual_fully_outside_window(
        [10.0, -23.0, 80.0, 24.0],
        win.0,
        win.1
    ));
}

#[test]
fn idle_ime_skips_imm_when_leaf_is_only_a_clip_sliver() {
    let win = (400.0, 300.0);
    // ScrollArea clip at y=80; text leaf mostly above it (gallery forms first Input).
    let leaf = [24.0, 56.0, 160.0, 20.0];
    let scroll_clip = [0.0, 80.0, 400.0, 220.0];
    assert!(
        !idle_ime_may_push_imm(leaf, Some(scroll_clip), win.0, win.1),
        "half-clipped wrap Input must not ImmSetCompositionWindow"
    );
    let in_view = [24.0, 200.0, 160.0, 20.0];
    assert!(
        idle_ime_may_push_imm(in_view, Some(scroll_clip), win.0, win.1),
        "2b / fully visible field may still push Imm"
    );
}

#[test]
fn clamp_ime_anchor_y_sticks_to_viewport_edges() {
    assert_eq!(clamp_ime_anchor_y(-40.0, 20.0, 300.0), 0.0);
    assert_eq!(clamp_ime_anchor_y(10.0, 20.0, 300.0), 10.0);
    assert_eq!(clamp_ime_anchor_y(290.0, 20.0, 300.0), 280.0);
}

#[test]
fn physical_area_tracks_scroll_and_scale() {
    // Field visually at (10, 40) after scroll; caret 24px into field; 2x scale.
    let (x, y, w, h) = physical_ime_cursor_area([10.0, 40.0, 200.0, 32.0], 24.0, 2.0);
    assert_eq!((x, y), (68, 80)); // (10+24)*2, 40*2
    assert!(w >= 1);
    assert_eq!(h, 64);
}

#[test]
fn physical_area_clamps_caret_to_field_width() {
    let (x, _, _, _) = physical_ime_cursor_area([0.0, 0.0, 50.0, 20.0], 999.0, 1.0);
    assert_eq!(x, 50);
}

#[test]
fn paint_space_ime_origin_identity_is_layout() {
    let p = paint_space_ime_origin([24.0, 80.0], &crate::style::MAT2D_IDENTITY, [124.0, 96.0]);
    assert!((p[0] - 24.0).abs() < 1e-4 && (p[1] - 80.0).abs() < 1e-4);
}

#[test]
fn paint_space_ime_origin_follows_center_pivot_rotate() {
    let origin = [50.0, 10.0];
    let linear =
        crate::style::mat2d_linear(&crate::style::mat2d_rotate(std::f32::consts::FRAC_PI_2));
    let p = paint_space_ime_origin([80.0, 10.0], &linear, origin);
    assert!(
        (p[0] - 50.0).abs() < 0.5 && (p[1] - 40.0).abs() < 0.5,
        "90° caret must land on the paint box, got {p:?}"
    );
    assert!((p[0] - 80.0).abs() > 5.0, "must not stay at layout x");
}

#[test]
fn prepare_compose_deletes_selection() {
    let (v, a) = prepare_compose_anchor("hello world", Some(11), Some((6, 11)));
    assert_eq!(v, "hello ");
    assert_eq!(a, 6);
}

#[test]
fn preedit_without_syllable_separators_strips_apostrophe() {
    assert_eq!(preedit_without_syllable_separators("shi'jie"), "shijie");
    assert_eq!(preedit_without_syllable_separators("你好"), "你好");
    assert_eq!(preedit_without_syllable_separators(""), "");
}

#[test]
fn alphanumeric_toggle_commits_preedit_and_clears_underline_marks() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let rest = "ime_shift_commit";
        let fid = format!("input:{rest}");
        let vk = crate::utils::text_field::input_value_signal_key(rest);
        let pk = crate::utils::text_field::input_preedit_signal_key(rest);
        let mk = crate::utils::text_field::input_preedit_marks_signal_key(rest);
        let ak = crate::utils::text_field::input_ime_anchor_signal_key(rest);
        crate::core::reactivity::use_signal(&vk, String::new).set("4@qqq".into());
        crate::core::reactivity::use_signal(&pk, String::new).set("q".into());
        crate::core::reactivity::use_signal(&ak, || None::<usize>).set(Some("4@qqq".len()));
        crate::core::reactivity::use_signal(&mk, Vec::<(usize, usize, u8)>::new)
            .set(vec![(0, 1, 1)]);
        crate::core::reactivity::use_signal(
            crate::utils::text_edit::field::IME_ACTIVE_SIGNAL,
            || false,
        )
        .set(true);
        on_ime_alphanumeric_toggle(&fid);
        assert_eq!(
            crate::core::reactivity::use_signal(&vk, String::new).peek(),
            "4@qqqq",
            "Shift 中/英 must commit the in-flight preedit into value"
        );
        assert!(
            crate::core::reactivity::use_signal(&pk, String::new)
                .peek()
                .is_empty(),
            "preedit must clear so the underline leaves"
        );
        assert!(
            crate::core::reactivity::use_signal(&mk, Vec::<(usize, usize, u8)>::new)
                .peek()
                .is_empty(),
            "preedit underline marks must clear"
        );
        assert!(
            !crate::core::reactivity::use_signal(
                crate::utils::text_edit::field::IME_ACTIVE_SIGNAL,
                || { false }
            )
            .peek(),
            "IME active flag must clear after the toggle commit"
        );
        clear_ime_end_drain();
    });
}

#[test]
fn alphanumeric_toggle_is_noop_without_preedit() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let rest = "ime_shift_noop";
        let fid = format!("input:{rest}");
        let vk = crate::utils::text_field::input_value_signal_key(rest);
        crate::core::reactivity::use_signal(&vk, String::new).set("hello".into());
        on_ime_alphanumeric_toggle(&fid);
        assert_eq!(
            crate::core::reactivity::use_signal(&vk, String::new).peek(),
            "hello"
        );
        assert!(
            !ime_end_drain_active(),
            "empty preedit must not CPS_CANCEL / end-drain"
        );
    });
}

#[test]
fn finish_composition_folds_preedit_stripping_apostrophe() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let rest = "ime_finish_strip";
        let fid = format!("input:{rest}");
        let vk = crate::utils::text_field::input_value_signal_key(rest);
        let pk = crate::utils::text_field::input_preedit_signal_key(rest);
        let ak = crate::utils::text_field::input_ime_anchor_signal_key(rest);
        crate::core::reactivity::use_signal(&vk, String::new).set("已上屏".into());
        crate::core::reactivity::use_signal(&pk, String::new).set("shi'jie".into());
        crate::core::reactivity::use_signal(&ak, || None::<usize>).set(Some("已上屏".len()));
        finish_composition_preserve_text(&fid);
        assert_eq!(
            crate::core::reactivity::use_signal(&vk, String::new).peek(),
            "已上屏shijie",
            "preedit folds in; syllable `'` must be stripped"
        );
        assert!(
            crate::core::reactivity::use_signal(&pk, String::new)
                .peek()
                .is_empty(),
            "preedit signal must be cleared"
        );
        assert!(ime_end_drain_active());
        clear_ime_end_drain();
    });
}

#[test]
fn finish_composition_inserts_at_extra_carets() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let rest = "ime_finish_multi";
        let fid = format!("input:{rest}");
        let vk = crate::utils::text_field::input_value_signal_key(rest);
        let pk = crate::utils::text_field::input_preedit_signal_key(rest);
        crate::core::reactivity::use_signal(&vk, String::new).set("ab".into());
        crate::core::reactivity::use_signal(
            &crate::utils::text_field::input_caret_signal_key(rest),
            || None::<usize>,
        )
        .set(Some(0));
        crate::utils::text_field::extra_carets_signal(rest).set(vec![(1, 1)]);
        crate::core::reactivity::use_signal(&pk, String::new).set("X".into());
        finish_composition_preserve_text(&fid);
        assert_eq!(
            crate::core::reactivity::use_signal(&vk, String::new).peek(),
            "XaXb"
        );
        assert_eq!(
            crate::utils::text_field::extra_carets_signal(rest).peek(),
            vec![(3, 3)]
        );
        clear_ime_end_drain();
    });
}

#[test]
fn field_to_field_focus_must_clear_end_drain() {
    // Mirrors `sync_ime_focus_owner`: after finish_* starts drain, landing on another
    // Input/Textarea (DevTools CSSOM ↔ app form) must clear it — IME stays allowed so
    // `Ime::Disabled` may never arrive.
    begin_ime_end_drain();
    assert!(ime_end_drain_active());
    let next = Some("input:gallery-name");
    if next.is_some_and(is_text_edit_focus) {
        clear_ime_end_drain();
    }
    assert!(!ime_end_drain_active());

    begin_ime_end_drain();
    let blur: Option<&str> = None;
    if blur.is_some_and(is_text_edit_focus) {
        clear_ime_end_drain();
    }
    assert!(
        ime_end_drain_active(),
        "blur to none must leave drain for Disabled to clear"
    );
    clear_ime_end_drain();
}

#[test]
fn apply_commit_mid_string() {
    let (v, c) = apply_ime_commit("ab中文", "ab中文".find('中').unwrap(), "好");
    assert_eq!(v, "ab好中文");
    assert_eq!(&v[..c], "ab好");
}

#[test]
fn snapshot_roundtrip() {
    let snap = ImeCursorSnapshot {
        focus_id: "input:t".into(),
        physical_x: 1,
        physical_y: 2,
        physical_w: 3,
        physical_h: 4,
        ui_scale: 1.5,
        visual_x: 0.0,
        visual_y: 0.0,
        caret_x_in_field: 8.0,
        source: "painted",
        scroll_acc_y: 40.0,
    };
    record_ime_cursor(snap.clone());
    assert_eq!(take_last_ime_cursor_snapshot(), Some(snap));
    assert!(take_last_ime_cursor_snapshot().is_none());
}

#[test]
fn painted_bounds_prefer_caret_node() {
    use crate::core::tree::TextLayoutInfo;
    use crate::style::TextStyle;
    let tid = crate::utils::text_field::input_text_template_id("gal");
    let mut style = TextStyle::default();
    style.input_caret = Some(crate::style::InputCaretDraw {
        caret_byte_index: 1,
        selection: None,
        selection_bg: [0; 4],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });
    let nodes = vec![
        TextLayoutInfo {
            text: "x".into(),
            shape_suffix: None,
            spans: None,
            template_id: Some(tid.clone()),
            x: 10.0,
            y: 500.0, // stale sibling without caret should lose
            width: 100.0,
            height: 20.0,
            clip_bounds: None,
            style: TextStyle::default(),
            prepared: None,
            glyph_fx: None,
            shimmer: None,
            paint_scale: 1.0,
            paint_transform: crate::style::MAT2D_IDENTITY,
            paint_origin: [0.0, 0.0],
            glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
        },
        TextLayoutInfo {
            text: "啊a".into(),
            shape_suffix: None,
            spans: None,
            template_id: Some(tid),
            x: 24.0,
            y: 80.0,
            width: 200.0,
            height: 32.0,
            clip_bounds: None,
            style,
            prepared: None,
            glyph_fx: None,
            shimmer: None,
            paint_scale: 1.0,
            paint_transform: crate::style::MAT2D_IDENTITY,
            paint_origin: [0.0, 0.0],
            glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
        },
    ];
    let b = painted_text_edit_visual_bounds(&nodes, "input:gal").expect("bounds");
    assert!((b[1] - 80.0).abs() < 0.5, "y={}", b[1]);
}

fn test_text_node(
    rest: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    clip: Option<[f32; 4]>,
    caret: bool,
) -> crate::core::tree::TextLayoutInfo {
    use crate::core::tree::TextLayoutInfo;
    use crate::style::TextStyle;
    let mut style = TextStyle::default();
    if caret {
        style.input_caret = Some(crate::style::InputCaretDraw {
            caret_byte_index: 0,
            selection: None,
            selection_bg: [0; 4],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            preedit_ul: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });
    }
    TextLayoutInfo {
        text: rest.into(),
        shape_suffix: None,
        spans: None,
        template_id: Some(crate::utils::text_field::input_text_template_id(rest)),
        x,
        y,
        width: w,
        height: h,
        clip_bounds: clip,
        style,
        prepared: None,
        glyph_fx: None,
        shimmer: None,
        paint_scale: 1.0,
        paint_transform: crate::style::MAT2D_IDENTITY,
        paint_origin: [x + w * 0.5, y + h * 0.5],
        glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
    }
}

#[test]
fn idle_ime_skips_imm_on_placeholder_after_focus_moved() {
    // Last presented frame: number still has the caret; Search is the in-view
    // placeholder. Focus signal already says Search (IME candidates sit there).
    let clip = Some([0.0, 0.0, 400.0, 300.0]);
    let nodes = vec![
        test_text_node("num", 40.0, -28.0, 120.0, 36.0, clip, true),
        test_text_node("search", 40.0, 80.0, 320.0, 36.0, clip, false),
    ];
    assert!(
        !painted_ime_field_in_overflow_clip(&nodes, "input:search", 400.0, 300.0),
        "placeholder leaf without caret must not Imm — composing would hang WaitUntil \
             while GPU still shows the previous field's ring"
    );
    let in_clip = test_text_node("search", 40.0, 80.0, 320.0, 36.0, clip, true);
    assert!(
        painted_ime_field_in_overflow_clip(&[in_clip], "input:search", 400.0, 300.0),
        "focused caret fully inside overflow clip may Imm"
    );
}

#[test]
fn scaled_ime_caret_uses_painted_extent_for_overflow_clip() {
    let mut node = test_text_node(
        "scaled",
        120.0,
        180.0,
        300.0,
        24.0,
        Some([100.0, 160.0, 195.0, 80.0]),
        true,
    );
    node.paint_scale = 0.57;
    assert!(
        painted_ime_field_in_overflow_clip(&[node.clone()], "input:scaled", 800.0, 600.0),
        "the complete scaled field is inside the preview clip"
    );
    node.clip_bounds = Some([100.0, 160.0, 150.0, 80.0]);
    assert!(
        !painted_ime_field_in_overflow_clip(&[node], "input:scaled", 800.0, 600.0),
        "an actually clipped scaled field must still suppress Imm updates"
    );
}

#[test]
fn painted_caret_mismatch_when_focus_moved_and_dl_still_has_old_caret() {
    let clip = Some([0.0, 0.0, 400.0, 300.0]);
    let nodes = vec![
        test_text_node("num", 40.0, -28.0, 120.0, 36.0, clip, true),
        test_text_node("search", 40.0, 80.0, 320.0, 36.0, clip, false),
    ];
    assert!(
        !painted_caret_matches_focus(&nodes, "input:search"),
        "Search focus + number caret must not plant WaitUntil on the stale blink"
    );
    assert!(
        painted_caret_matches_focus(&nodes, "input:num"),
        "caret still on the focused number field is a match"
    );
    let none = vec![test_text_node(
        "search", 40.0, 80.0, 320.0, 36.0, clip, false,
    )];
    assert!(
        painted_caret_matches_focus(&none, "input:search"),
        "no painted caret is not a stale-caret mismatch"
    );
}

#[test]
fn ime_fallback_leaf_is_on_number_while_caret_stays_on_placeholder() {
    // Gallery forms first wrap row: click Placeholder, then Number. IME candidates
    // jump immediately because `painted_text_edit_node` falls back to the number
    // placeholder leaf ("0"). Ring/caret stay on Placeholder until a Poll chrome frame.
    let clip = Some([0.0, 0.0, 520.0, 200.0]);
    let nodes = vec![
        test_text_node("placeholder", 16.0, 40.0, 120.0, 36.0, clip, true),
        test_text_node("num", 400.0, 40.0, 100.0, 36.0, clip, false),
    ];
    let ime_leaf = painted_text_edit_node(&nodes, "input:num").expect("number leaf in DL");
    assert!(
        ime_leaf.style.input_caret.is_none(),
        "IME uses the number placeholder box, not a caret"
    );
    assert!(
        (ime_leaf.x - 400.0).abs() < 0.5,
        "IME visual X must be the number field, got {}",
        ime_leaf.x
    );
    assert!(
        !painted_caret_matches_focus(&nodes, "input:num"),
        "Placeholder caret + number focus must Poll, not WaitUntil the stale blink"
    );
    assert!(
        !painted_ime_field_in_overflow_clip(&nodes, "input:num", 520.0, 200.0),
        "idle Imm stays off until the chrome frame installs the number caret"
    );
}
