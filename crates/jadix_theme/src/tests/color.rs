use super::*;

#[test]
fn parses_caret_and_selection_tokens() {
    assert_eq!(try_parse_color("caret"), try_parse_color("caret-color"));
    assert_eq!(
        try_parse_color("selection-background"),
        try_parse_color("selection")
    );
    let bg = try_parse_color("selection-background").unwrap();
    let fg = try_parse_color("selection-foreground").unwrap();
    assert_ne!(bg, fg, "selection pair must be inverse colors");
    assert_eq!(try_parse_color("caret"), try_parse_color("foreground"));
    assert_eq!(
        try_parse_color("selection-background"),
        try_parse_color("foreground")
    );
    assert_eq!(
        try_parse_color("selection-foreground"),
        try_parse_color("background")
    );
}

#[test]
fn parses_semantic_token_opacity_suffix() {
    let d10 = try_parse_color("destructive/10").unwrap();
    let destructive = try_parse_color("destructive").unwrap();
    let bg = try_parse_color("background").unwrap();
    let want = [
        destructive[0] * 0.1 + bg[0] * 0.9,
        destructive[1] * 0.1 + bg[1] * 0.9,
        destructive[2] * 0.1 + bg[2] * 0.9,
        1.0,
    ];
    assert!(
        (0..4).all(|i| (d10[i] - want[i]).abs() < 1e-4),
        "destructive/10 should mix over background"
    );
    let p80 = try_parse_color("primary/80").unwrap();
    assert!(p80[0] < destructive[0] || p80[0] > 0.1);
}

#[test]
fn parses_info_success_warning_tokens_aligned_to_tw_600() {
    assert_eq!(try_parse_color("info"), try_parse_color("blue-600"));
    assert_eq!(try_parse_color("success"), try_parse_color("green-600"));
    assert_eq!(try_parse_color("warning"), try_parse_color("orange-600"));
    assert_eq!(try_parse_color("warn"), try_parse_color("warning"));
    let info = try_parse_color("info").unwrap();
    let i10 = try_parse_color("info/10").unwrap();
    let bg = try_parse_color("background").unwrap();
    let want = [
        info[0] * 0.1 + bg[0] * 0.9,
        info[1] * 0.1 + bg[1] * 0.9,
        info[2] * 0.1 + bg[2] * 0.9,
        1.0,
    ];
    assert!(
        (0..4).all(|i| (i10[i] - want[i]).abs() < 1e-4),
        "info/10 should mix over background"
    );
}

#[test]
fn parses_hex_and_rgba_hsl_oklch() {
    let hex = try_parse_color("#ff0000").unwrap();
    assert!((hex[0] - 1.0).abs() < 1e-3);

    let rgb = try_parse_color("rgb(255, 0, 0)").unwrap();
    assert!((rgb[0] - 1.0).abs() < 1e-3 && rgb[1] < 1e-3);

    let modern = try_parse_color("rgb(255 0 0 / 50%)").unwrap();
    assert!((modern[3] - 0.5).abs() < 1e-3);

    let hsl = try_parse_color("hsl(120, 100%, 50%)").unwrap();
    assert!(hsl[1] > 0.9 && hsl[0] < 1e-3);

    let oklch = try_parse_color("oklch(0.63 0.2 25)").unwrap();
    assert!(oklch[0] > 0.5); // reddish
}

#[test]
fn parses_hwb_color_mix_and_relative() {
    let hwb = try_parse_color("hwb(120 20% 10%)").unwrap();
    assert!(hwb[1] > hwb[0] && hwb[1] > hwb[2]); // greenish

    let mixed = try_parse_color("color-mix(in oklab, red 50%, blue)").unwrap();
    assert!(mixed[0] > 0.1 && mixed[2] > 0.1);

    let rel = try_parse_color("rgb(from #ff0000 r g b / 50%)").unwrap();
    assert!((rel[0] - 1.0).abs() < 1e-3);
    assert!((rel[3] - 0.5).abs() < 1e-3);

    let rel_hsl = try_parse_color("hsl(from lime h s l)").unwrap();
    assert!(rel_hsl[1] > 0.8 && rel_hsl[0] < 0.2);
}
