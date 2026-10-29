use super::*;
use crate::style::{parse_color, LineStyle, TextAlign, TextAlignDeclaration, TextOverflow};

#[test]
fn paint_color_dual_accepts_token_and_array() {
    let from_token = "primary".into_paint_color();
    let parsed = parse_color("primary");
    assert_eq!(from_token, parsed);
    let arr = [0.1, 0.2, 0.3, 1.0];
    assert_eq!(arr.into_paint_color(), arr);
    assert_eq!((&arr).into_paint_color(), arr);
}

#[test]
fn overflow_dual_accepts_keyword_and_enum() {
    assert_eq!("hidden".into_overflow(), Overflow::Hidden);
    assert_eq!(Overflow::Clip.into_overflow(), Overflow::Clip);
    assert_eq!(overflow_as_str(parse_overflow_keyword("auto")), "scroll");
}

#[test]
fn justify_and_items_parse_tailwind_aliases() {
    assert_eq!(
        "between".into_justify_content(),
        JustifyContent::SPACE_BETWEEN
    );
    assert_eq!("center".into_align_items(), AlignItems::CENTER);
    assert_eq!("wrap-reverse".into_flex_wrap(), FlexWrap::WrapReverse);
}

#[test]
fn local_enums_from_str_and_self() {
    let overflow: TextOverflow = "ellipsis".into();
    assert_eq!(overflow, TextOverflow::Ellipsis);
    let align: TextAlign = "center".into();
    assert_eq!(align, TextAlign::Center);
    let decl: TextAlignDeclaration = "inherit".into();
    assert_eq!(decl, TextAlignDeclaration::Inherit);
    let decl2: TextAlignDeclaration = TextAlign::Right.into();
    assert_eq!(decl2, TextAlignDeclaration::Value(TextAlign::Right));
    let style: LineStyle = "dashed".into();
    assert_eq!(style, LineStyle::Dashed);
    assert_eq!(LineStyle::Solid, LineStyle::from("solid"));
}

#[test]
fn cursor_kind_and_inherit() {
    assert_eq!(
        CursorKind::Text.into_cursor().map(|c| c.kind),
        Some(CursorKind::Text)
    );
    assert_eq!("inherit".into_cursor(), None);
    assert_eq!(
        "pointer".into_cursor().map(|c| c.kind),
        Some(CursorKind::Pointer)
    );
}
