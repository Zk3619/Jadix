use super::*;

#[test]
fn tokens_chrome_is_transparent_and_muted_foreground() {
    let t = ChartTheme::tokens();
    assert_eq!(t.background, parse_color("transparent"));
    assert_eq!(t.muted, parse_color("muted-foreground"));
    assert_eq!(t.grid, parse_color("border/50"));
    assert_eq!(t.palette.len(), 5);
}

#[test]
fn unknown_tooltip_indicator_falls_back_to_dot() {
    assert_eq!(TooltipIndicator::from("dot"), TooltipIndicator::Dot);
    assert_eq!(TooltipIndicator::from("line"), TooltipIndicator::Line);
    assert_eq!(TooltipIndicator::from("dashed"), TooltipIndicator::Dashed);
    assert_eq!(TooltipIndicator::from("nope"), TooltipIndicator::Dot);
}

#[test]
fn palette_presets_nonempty() {
    assert_eq!(ChartTheme::neutral().palette.len(), 5);
    assert_eq!(ChartTheme::vibrant().palette.len(), 5);
    assert_eq!(ChartTheme::monochrome().palette.len(), 5);
}
