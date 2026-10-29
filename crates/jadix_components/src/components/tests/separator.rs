use super::*;

#[test]
fn builder_style_reaches_separator() {
    let el: Element = separator().style(|b| b.p(8.0)).into();
    let pad = crate::layout::lp_length_px(el.style.padding.left).unwrap_or(0.0);
    assert!(
        (pad - 8.0).abs() < 0.01,
        "separator .style(|b| b.p(8)) must land on the line"
    );
}
