use super::*;

#[test]
fn parse_keywords_and_svg_aliases() {
    assert_eq!(
        WritingMode::parse("horizontal-tb"),
        Some(WritingMode::HorizontalTb)
    );
    assert_eq!(
        WritingMode::parse("vertical-rl"),
        Some(WritingMode::VerticalRl)
    );
    assert_eq!(
        WritingMode::parse("vertical-lr"),
        Some(WritingMode::VerticalLr)
    );
    assert_eq!(
        WritingMode::parse("sideways-rl"),
        Some(WritingMode::SidewaysRl)
    );
    assert_eq!(
        WritingMode::parse("sideways-lr"),
        Some(WritingMode::SidewaysLr)
    );
    assert_eq!(WritingMode::parse("tb-rl"), Some(WritingMode::VerticalRl));
    assert_eq!(WritingMode::parse("lr-tb"), Some(WritingMode::HorizontalTb));
    assert_eq!(WritingMode::parse("nope"), None);
}

#[test]
fn vertical_rl_logical_edges() {
    let wm = WritingMode::VerticalRl;
    assert_eq!(wm.inline_start(TextDirection::Ltr), PhysicalSide::Top);
    assert_eq!(wm.inline_end(TextDirection::Ltr), PhysicalSide::Bottom);
    assert_eq!(wm.block_start(), PhysicalSide::Right);
    assert_eq!(wm.block_end(), PhysicalSide::Left);
    assert_eq!(wm.inline_start(TextDirection::Rtl), PhysicalSide::Bottom);
}

#[test]
fn horizontal_rtl_inline_start_is_right() {
    assert_eq!(
        WritingMode::HorizontalTb.inline_start(TextDirection::Rtl),
        PhysicalSide::Right
    );
}

#[test]
fn vertical_rl_flex_row_is_column() {
    let mapped = WritingMode::VerticalRl.map_flex_direction(TextDirection::Ltr, FlexDirection::Row);
    assert_eq!(mapped, FlexDirection::Column);
    assert!(WritingMode::VerticalRl.flip_flex_cross_start(FlexDirection::Row));
}
