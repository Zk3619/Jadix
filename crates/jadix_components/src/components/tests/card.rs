use super::*;

#[test]
fn responsive_card_footer_wraps_actions_without_horizontal_overflow() {
    use crate::core::tree::LayoutTree;
    let footer: Element = card_footer()
        .child(Element::new().w(160.0).min_w(160.0).h(36.0))
        .child(Element::new().w(160.0).min_w(160.0).h(36.0))
        .into();
    let mut root = Element::new().w(320.0).child(footer);
    LayoutTree::new().build_and_layout(&mut root, 320.0, 600.0, [-1.0, -1.0]);
    let footer = &root.children[0];
    assert!(footer.children[1].layout_bounds[1] > footer.children[0].layout_bounds[1]);
    for action in &footer.children {
        assert!(action.layout_bounds[0] + action.layout_bounds[2] <= 320.5);
    }
}

#[test]
fn card_title_picks_up_stylesheet_slot_rule() {
    use crate::style::{clear_stylesheets, parse_stylesheet, register_stylesheet};
    clear_stylesheets();
    let sheet = parse_stylesheet(
        r#"
[[rule]]
selectors = ["[data-slot=card-title]"]
classes = "italic"
"#,
    )
    .unwrap();
    register_stylesheet(sheet).unwrap();
    let el = CardTitle(CardTitleProps {
        children: Some(Element::from(text("Hi"))),
        ..Default::default()
    });
    let cs = el.class_set.as_ref().expect("class_set");
    assert!(
        cs.base.iter().any(|c| c == "italic"),
        "#[component] exit must bind [data-slot=card-title]; got {:?}",
        cs.base
    );
    assert_eq!(
        el.attrs.get("data-jadix-sheet").map(String::as_str),
        Some("1")
    );
    clear_stylesheets();
}

#[test]
fn builder_header_title_class_and_style() {
    let el: Element = card()
        .class("w-full")
        .style(|b| b.gap(8.0))
        .header_with(|h| {
            h.class("px-2")
                .title_with(|t| t.class("font-bold").style(|b| b.text_size(20.0)).text("Hi"))
        })
        .into();
    assert!(!el.children.is_empty());
    let header = &el.children[0];
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(header, 20.0),
        "title slot .style(|b| b.text_size(20)) must land on the title tree"
    );
}
