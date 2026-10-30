use super::*;

#[test]
fn icon_inserts_glyph_between_button_labels() {
    let body = r#"<HStack><Button variant="outline">"LG"</Button><Button><Icon name="star" color="primary-foreground" size="0.75rem" /></Button><Button>"Leading"</Button></HStack>"#;
    let v = extract_text_literals_tree_order(body);
    let star = icon_glyph_char("star").unwrap();
    assert_eq!(v, vec!["LG".to_string(), star, "Leading".to_string()]);
}

/// 源码顺序：先 `<Icon/>` 再 `"Suffix"` → 与 `Button::content_slots` 一致 [glyph, Suffix]。
#[test]
fn text_after_icon_matches_runtime_dfs_order() {
    let body = r#"<Button><Icon name="heart" />"Suffix"</Button>"#;
    let v = extract_text_literals_tree_order(body);
    let heart = icon_glyph_char("heart").unwrap();
    assert_eq!(v, vec![heart, "Suffix".to_string()]);
}

/// 源码顺序：先 `"Prefix"` 再 `<Icon/>` → [Prefix, glyph]。
#[test]
fn text_before_icon_matches_runtime_dfs_order() {
    let body = r#"<Button>"Prefix"<Icon name="star" /></Button>"#;
    let v = extract_text_literals_tree_order(body);
    let star = icon_glyph_char("star").unwrap();
    assert_eq!(v, vec!["Prefix".to_string(), star]);
}

#[test]
fn scroll_area_label_text_size_is_element_index_two() {
    let body = r#"<ScrollArea id="scroll-navigation"><VStack><Label text_size="1.25rem" font_weight="700">"导航"</Label></VStack></ScrollArea>"#;
    let attrs = extract_style_attributes_tree_order(body);
    let size = attrs
        .iter()
        .find(|a| a.name == "text_size")
        .expect("text_size");
    assert_eq!(size.element_index, 2);
    assert_eq!(size.value, "1.25rem");
    let weight = attrs
        .iter()
        .find(|a| a.name == "font_weight")
        .expect("font_weight");
    assert_eq!(weight.element_index, 2);
}

#[test]
fn numeric_style_attributes_reach_the_runtime_in_tree_order() {
    let attributes = extract_style_attributes_tree_order(
        r#"<Box w=128.0 p=-2.5><Text text_size=14.0>"value"</Text></Box>"#,
    );
    assert_eq!(
        attributes
            .iter()
            .map(|attribute| (
                attribute.element_index,
                attribute.name.as_str(),
                attribute.value.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, "w", "128.0"),
            (0, "p", "-2.5"),
            (1, "text_size", "14.0")
        ]
    );
}
