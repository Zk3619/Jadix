use super::*;

#[test]
fn em_resolves_against_root_16px_not_element_font_size() {
    // IntoDimension 无节点上下文：em ≡ rem ≡ 活根字号。布局期 CssDim 用本节点 font-size。
    let root = css_root_font_size_px();
    match dim_kind("1em".into_dimension()) {
        DimKind::Length(px) => assert!((px - root).abs() < 1e-6, "1em → {px}"),
        other => panic!("expected length, got {other:?}"),
    }
    match dim_kind("0.5em".into_dimension()) {
        DimKind::Length(px) => assert!((px - root * 0.5).abs() < 1e-6, "0.5em → {px}"),
        other => panic!("expected length, got {other:?}"),
    }
    assert!((IntoF32::into_f32("1.25em") - root * 1.25).abs() < 1e-6);
}

#[test]
fn into_f32_unitless_string_is_rem_like_tailwind_tokens() {
    // `text-sm` ≈ 0.875rem；字符串无单位 = rem（非 spacing scale）。
    let root = css_root_font_size_px();
    assert!((IntoF32::into_f32("0.875") - 0.875 * root).abs() < 1e-6);
    assert!((IntoF32::into_f32(14) - 14.0).abs() < 1e-6); // Rust 数值 = px
}

#[test]
fn into_f32_raw_font_weight_unitless() {
    assert!((IntoF32Raw::into_f32_raw("600") - 600.0).abs() < 1e-6);
    assert!((IntoF32Raw::into_f32_raw(600) - 600.0).abs() < 1e-6);
    // 对比：长度路径会把 "600" 当 600rem
    assert!((IntoF32::into_f32("600") - 600.0 * css_root_font_size_px()).abs() < 1e-6);
}

#[test]
fn into_opacity_fraction_and_percent() {
    assert!((0.4f32.into_opacity() - 0.4).abs() < 1e-6);
    assert!((1i32.into_opacity() - 1.0).abs() < 1e-6);
    assert!((0i32.into_opacity() - 0.0).abs() < 1e-6);
    assert!((40i32.into_opacity() - 0.4).abs() < 1e-6);
    assert!((100u8.into_opacity() - 1.0).abs() < 1e-6);
    assert!(("40%".into_opacity() - 0.4).abs() < 1e-6);
    assert!(("100%".into_opacity() - 1.0).abs() < 1e-6);
    assert!(("0.4".into_opacity() - 0.4).abs() < 1e-6);
    assert!(("1".into_opacity() - 1.0).abs() < 1e-6);
    assert!((parse_opacity("50%") - 0.5).abs() < 1e-6);
}

#[test]
fn into_f32_side_offset_parity_4_eq_4px_not_unitless_1() {
    // Builder / RSX `{4}` = 4px；无单位 `"1"` / `"0.25"` = rem（活根字号）。
    let root = css_root_font_size_px();
    assert!((IntoF32::into_f32(4) - IntoF32::into_f32("4px")).abs() < 1e-6);
    assert!((IntoF32::into_f32("0.25") - 0.25 * root).abs() < 1e-6);
    assert!((IntoF32::into_f32("0.25rem") - 0.25 * root).abs() < 1e-6);
    assert!((IntoF32::into_f32("1") - root).abs() < 1e-6);
    assert!((IntoF32::into_f32(4) - IntoF32::into_f32("1")).abs() > 1.0);
}

#[test]
fn into_border_widths_css_shorthand() {
    assert_eq!(IntoBorderWidths::into_border_widths(2.0_f32), [2.0; 4]);
    assert_eq!(
        IntoBorderWidths::into_border_widths("3px 9px"),
        [3.0, 9.0, 3.0, 9.0]
    );
    assert_eq!(
        IntoBorderWidths::into_border_widths("1px 2px 3px 4px"),
        [1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn css_dim_negated() {
    assert!(matches!(CssDim::Rem(0.25).negated(), CssDim::Rem(v) if (v + 0.25).abs() < 1e-6));
    assert!(matches!(CssDim::Px(4.0).negated(), CssDim::Px(v) if (v + 4.0).abs() < 1e-6));
    assert!(matches!(CssDim::Auto.negated(), CssDim::Auto));
    let add = CssDim::CalcAdd(Box::new(CssDim::Px(1.0)), Box::new(CssDim::Px(2.0)));
    match add.negated() {
        CssDim::CalcAdd(a, b) => {
            assert!(matches!(*a, CssDim::Px(v) if (v + 1.0).abs() < 1e-6));
            assert!(matches!(*b, CssDim::Px(v) if (v + 2.0).abs() < 1e-6));
        }
        other => panic!("expected CalcAdd, got {other:?}"),
    }
}

#[test]
fn parses_dvh_dvw_as_percent() {
    match dim_kind("85dvh".into_dimension()) {
        DimKind::Percent(p) => assert!((p - 0.85).abs() < 1e-6),
        other => panic!("expected percent, got {other:?}"),
    }
    match dim_kind("85dvw".into_dimension()) {
        DimKind::Percent(p) => assert!((p - 0.85).abs() < 1e-6),
        other => panic!("expected percent, got {other:?}"),
    }
    match dim_kind("50vh".into_dimension()) {
        DimKind::Percent(p) => assert!((p - 0.5).abs() < 1e-6),
        other => panic!("expected percent, got {other:?}"),
    }
}

#[test]
fn css_dim_vw_vh_em_calc_resolve() {
    let ctx = UnitCtx::new(800.0, 600.0, 20.0);
    assert!(matches!(parse_css_dim("auto"), Some(CssDim::Auto)));
    match dim_kind(parse_css_dim("50vw").unwrap().to_dimension(&ctx)) {
        DimKind::Length(px) => assert!((px - 400.0).abs() < 0.5, "50vw → {px}"),
        other => panic!("{other:?}"),
    }
    match dim_kind(parse_css_dim("100dvh").unwrap().to_dimension(&ctx)) {
        DimKind::Length(px) => assert!((px - 600.0).abs() < 0.5, "100dvh → {px}"),
        other => panic!("{other:?}"),
    }
    match dim_kind(parse_css_dim("2em").unwrap().to_dimension(&ctx)) {
        DimKind::Length(px) => assert!((px - 40.0).abs() < 0.5, "2em@20px → {px}"),
        other => panic!("{other:?}"),
    }
    match dim_kind(parse_css_dim("10rem").unwrap().to_dimension(&ctx)) {
        DimKind::Length(px) => assert!((px - 160.0).abs() < 0.5, "10rem → {px}"),
        other => panic!("{other:?}"),
    }
    match dim_kind(parse_css_dim("50%").unwrap().to_dimension(&ctx)) {
        DimKind::Percent(p) => assert!((p - 0.5).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
    match dim_kind(
        parse_css_dim("calc(100vw - 2rem)")
            .unwrap()
            .to_dimension(&ctx),
    ) {
        DimKind::Length(px) => assert!((px - 768.0).abs() < 0.5, "calc → {px}"),
        other => panic!("{other:?}"),
    }
    match dim_kind(parse_css_dim("calc(1em + 8px)").unwrap().to_dimension(&ctx)) {
        DimKind::Length(px) => assert!((px - 28.0).abs() < 0.5, "calc em+px → {px}"),
        other => panic!("{other:?}"),
    }
    let pct_calc = parse_css_dim("calc(100% - 40px)").unwrap();
    assert!(pct_calc.has_percent());
    assert!(pct_calc.to_dimension(&ctx).is_auto());
    let ctx_cb = ctx.with_containing_block(Some(260.0), Some(180.0));
    match dim_kind(pct_calc.to_dimension(&ctx_cb)) {
        DimKind::Length(px) => {
            assert!((px - 220.0).abs() < 0.5, "calc(100%-40px) @260 → {px}")
        }
        other => panic!("{other:?}"),
    }
    match dim_kind(
        parse_css_dim("calc(50% + 10px)")
            .unwrap()
            .to_dimension_axis(&ctx_cb, PercentAxis::Block),
    ) {
        DimKind::Length(px) => {
            assert!((px - 100.0).abs() < 0.5, "50% of 180 + 10 → {px}")
        }
        other => panic!("{other:?}"),
    }
    let pair = CssRectPair::parse("10px 20px").unwrap();
    assert!(matches!(
        pair,
        CssRectPair::StartEnd(CssDim::Px(10.0), CssDim::Px(20.0))
    ));
}

#[test]
fn unsuffixed_100_is_rem_not_parent_percent() {
    match parse_css_dim("100") {
        Some(CssDim::Rem(v)) => assert!((v - 100.0).abs() < 1e-6, "got Rem({v})"),
        other => panic!("w(\"100\") must stay rem, got {other:?}"),
    }
    match dim_kind("100".into_dimension()) {
        DimKind::Length(px) => assert!(
            (px - 100.0 * css_root_font_size_px()).abs() < 1e-6,
            "100rem → {px}"
        ),
        other => panic!("expected length, got {other:?}"),
    }
    match parse_css_dim("12") {
        Some(CssDim::Rem(v)) => assert!((v - 12.0).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
}

#[test]
fn percent_token_is_parent_fraction_not_content() {
    match parse_css_dim("100%") {
        Some(CssDim::Percent(p)) => assert!((p - 1.0).abs() < 1e-6),
        other => panic!("100% must be parent fraction, got {other:?}"),
    }
    match parse_css_dim("50%") {
        Some(CssDim::Percent(p)) => assert!((p - 0.5).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
    match dim_kind("100%".into_dimension()) {
        DimKind::Percent(p) => assert!((p - 1.0).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
    let ctx = UnitCtx::new(800.0, 600.0, 16.0).with_containing_block(Some(260.0), Some(180.0));
    match dim_kind(parse_css_dim("100%").unwrap().to_dimension(&ctx)) {
        DimKind::Percent(p) => assert!(
            (p - 1.0).abs() < 1e-6,
            "Taffy keeps % so layout resolves against the parent CB, got {p}"
        ),
        other => panic!("{other:?}"),
    }
    assert!(
        (parse_css_dim("100%")
            .unwrap()
            .resolve_px(&ctx, PercentAxis::Inline)
            .unwrap()
            - 260.0)
            .abs()
            < 0.5,
        "100% of parent content 260"
    );
    assert!(
        (parse_css_dim("100%")
            .unwrap()
            .resolve_px(&ctx, PercentAxis::Block)
            .unwrap()
            - 180.0)
            .abs()
            < 0.5,
        "100% of parent content height 180"
    );
}

#[test]
fn slash_fraction_is_parent_percent_not_content() {
    match parse_css_dim("1/2") {
        Some(CssDim::Percent(p)) => assert!((p - 0.5).abs() < 1e-6),
        other => panic!("1/2 must be 50% of parent, got {other:?}"),
    }
    match parse_css_dim("3/4") {
        Some(CssDim::Percent(p)) => assert!((p - 0.75).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
    match parse_css_dim("11/12") {
        Some(CssDim::Percent(p)) => assert!((p - 11.0 / 12.0).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
    match dim_kind("1/2".into_dimension()) {
        DimKind::Percent(p) => assert!((p - 0.5).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
    assert!(
        parse_css_dim("3/2").is_none(),
        "ratio > 1 is not a size fraction"
    );
    assert!(parse_css_dim("1/0").is_none());
    let ctx = UnitCtx::new(800.0, 600.0, 16.0).with_containing_block(Some(260.0), Some(180.0));
    assert!(
        (parse_css_dim("1/2")
            .unwrap()
            .resolve_px(&ctx, PercentAxis::Inline)
            .unwrap()
            - 130.0)
            .abs()
            < 0.5,
        "1/2 of parent content 260"
    );
    assert!(
        (parse_css_dim("1/2")
            .unwrap()
            .resolve_px(&ctx, PercentAxis::Block)
            .unwrap()
            - 90.0)
            .abs()
            < 0.5,
        "1/2 of parent content height 180"
    );
}

#[test]
fn fit_min_max_content_keywords() {
    assert!(matches!(parse_css_dim("fit"), Some(CssDim::FitContent)));
    assert!(matches!(
        parse_css_dim("fit-content"),
        Some(CssDim::FitContent)
    ));
    assert!(matches!(
        parse_css_dim("FIT-CONTENT"),
        Some(CssDim::FitContent)
    ));
    assert!(matches!(
        parse_css_dim("min-content"),
        Some(CssDim::MinContent)
    ));
    assert!(matches!(parse_css_dim("min"), Some(CssDim::MinContent)));
    assert!(matches!(
        parse_css_dim("max-content"),
        Some(CssDim::MaxContent)
    ));
    assert!(matches!(parse_css_dim("max"), Some(CssDim::MaxContent)));
    assert!(parse_css_dim("fit").unwrap().is_content_keyword());
    match dim_kind("fit-content".into_dimension()) {
        DimKind::Auto => {}
        other => panic!("fit-content maps to Auto for Taffy, got {other:?}"),
    }
    assert!(matches!("fit".into_css_dim(), CssDim::FitContent));
}

#[test]
fn padding_margin_percent_bakes_against_containing_block_width() {
    let ctx = UnitCtx::new(800.0, 100.0, 16.0).with_containing_block(Some(400.0), Some(80.0));
    let pad = CssDim::Percent(0.10).to_box_edge_length_percentage(&ctx);
    assert!(
        (lp_length_px(pad).unwrap() - 40.0).abs() < 0.01,
        "10% of cb width 400 → 40, not 10% of height 80"
    );
    let mar = CssDim::Percent(0.25).to_box_edge_length_percentage_auto(&ctx);
    match lpa_kind(mar) {
        DimKind::Length(px) => assert!((px - 100.0).abs() < 0.01, "25% of 400 → {px}"),
        other => panic!("{other:?}"),
    }
    let unk = UnitCtx::new(800.0, 100.0, 16.0);
    let zero = CssDim::Percent(0.10).to_box_edge_length_percentage(&unk);
    assert!(
        (lp_length_px(zero).unwrap()).abs() < 0.01,
        "cyclic / unknown CB → 0"
    );
}

#[test]
fn calc_size_parses_and_evals_size_plus_px() {
    let dim = parse_css_dim("calc-size(auto, size + 20px)").expect("parse");
    match &dim {
        CssDim::CalcSize { basis, calc } => {
            assert!(matches!(**basis, CssDim::Auto));
            assert!(calc.contains_size_keyword());
        }
        other => panic!("{other:?}"),
    }
    let ctx = UnitCtx::new(800.0, 600.0, 16.0);
    let px = dim
        .eval_calc_size(100.0, &ctx, PercentAxis::Block)
        .expect("eval");
    assert!(
        (px - 120.0).abs() < 1e-3,
        "calc-size(auto, size + 20px) of 100 → {px}"
    );
}

#[test]
fn calc_size_half_max_content() {
    let dim = parse_css_dim("calc-size(max-content, size * 0.5)").expect("parse");
    let ctx = UnitCtx::new(800.0, 600.0, 16.0);
    let px = dim
        .eval_calc_size(80.0, &ctx, PercentAxis::Inline)
        .expect("eval");
    assert!((px - 40.0).abs() < 1e-3, "got {px}");
}

#[test]
fn px_rem_em_helpers_and_live_root() {
    assert!(matches!(px(4.0), CssDim::Px(v) if (v - 4.0).abs() < 1e-6));
    assert!(matches!(rem(0.5), CssDim::Rem(v) if (v - 0.5).abs() < 1e-6));
    assert!(matches!(em(1.25), CssDim::Em(v) if (v - 1.25).abs() < 1e-6));
    assert!((IntoF32::into_f32(px(4.0)) - 4.0).abs() < 1e-6);
    assert_eq!(parse_css_dim("1rem"), Some(rem(1.0)));
    let rem1 = IntoF32::into_f32(rem(1.0));
    let rem1_str = IntoF32::into_f32("1rem");
    assert!(
        (rem1_str - rem1).abs() < 1e-6,
        "rem(1)={rem1} \"1rem\"={rem1_str} root={}",
        css_root_font_size_px()
    );
    assert!(rem1 > 4.0, "1rem must not collapse to 1px, got {rem1}");
    match dim_kind(rem(1.0).into_dimension()) {
        DimKind::Length(px) => assert!((px - rem1).abs() < 1e-6, "1rem → {px}"),
        other => panic!("{other:?}"),
    }
    let ctx = UnitCtx::new(800.0, 600.0, 24.0);
    assert!((ctx.root_font_size - css_root_font_size_px()).abs() < 1e-6);
    match dim_kind(CssDim::Em(2.0).to_dimension(&ctx)) {
        DimKind::Length(px) => assert!((px - 48.0).abs() < 1e-6, "2em@24 → {px}"),
        other => panic!("{other:?}"),
    }
}
