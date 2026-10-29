#[test]
fn element_style_does_not_claim_inherited_text_as_local() {
    let mut root = Element::new()
        .text_size(27.0)
        .font_weight(700.0)
        .text_color(parse_text_color("#123456"))
        .children([
            Element::new().text("inherited"),
            Element::new().text("explicit").text_size(27.0),
        ]);
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    let inherited = element_style_declarations(&root.children[0]);
    assert!(!inherited.contains("font-size:"), "{inherited}");
    assert!(!inherited.contains("font-weight:"), "{inherited}");
    assert!(!inherited.contains("color:"), "{inherited}");
    assert!(element_style_declarations(&root.children[1]).contains("font-size: 27px;"));
    let inherited = inherited_text_rows(&root, &[0]);
    assert!(inherited
        .iter()
        .any(|(name, value)| name.starts_with("font-size") && value == "27px"));
    root.text_style.size = Some(31.0);
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    assert_eq!(root.children[0].text_style.size, Some(31.0));
    assert_eq!(root.children[1].text_style.size, Some(27.0));
    assert!(!element_style_declarations(&root.children[0]).contains("font-size:"));
}
