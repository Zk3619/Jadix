use super::*;

#[test]
fn opaque_fingerprints_ignore_display_spacing_but_preserve_rust_tokens() {
    let source = r#"<Box>"old" {Element::new().text("captured")}</Box>"#;
    let spaced = r#"<Box>"new" { Element :: new () . text ("captured") }</Box>"#;
    assert_eq!(template_signature(source), template_signature(spaced));
    assert_ne!(
        template_signature(source),
        template_signature(&source.replace("captured", "different"))
    );
    assert_ne!(
        template_signature(source),
        template_signature(&source.replace("captured", "cap tured"))
    );
    let comparison = r#"<Box>{left >= right}</Box>"#;
    assert_ne!(
        template_signature(comparison),
        template_signature(&comparison.replace(">=", "> ="))
    );
    let binding = r#"<Box w={width.get()}>"old"</Box>"#;
    assert_eq!(
        template_signature(binding),
        template_signature(&binding.replace("width.get()", "width . get ()"))
    );
}

#[test]
fn numeric_style_literals_remain_data_without_hiding_rust_slots() {
    let old = r#"fn view() { ui! { <Box w=96.0 h=64 p=-2.5>"old"</Box> } }"#;
    let next = old
        .replace("96.0", "128.0")
        .replace("64", "72")
        .replace("-2.5", "-4.5")
        .replace("old", "new");
    assert_eq!(source_signature(old), source_signature(&next));
    let tree = template_tree(r#"<Box w=96.0 h=64 p=-2.5>"old"</Box>"#).unwrap();
    let crate::TemplateNode::Element { attributes, .. } = tree else {
        panic!("numeric declarations became an opaque slot")
    };
    assert_eq!(
        attributes,
        vec![
            ("w".into(), "96.0".into()),
            ("h".into(), "64".into()),
            ("p".into(), "-2.5".into())
        ]
    );
    assert_ne!(
        source_signature(old),
        source_signature(&old.replace("w=96.0", "w={compute()}"))
    );
    assert_ne!(
        source_signature(old),
        source_signature(&old.replace("Box", "Text"))
    );
    assert!(matches!(
        template_tree(r#"<Box w=calculate()>"old"</Box>"#),
        None | Some(crate::TemplateNode::Slot { .. })
    ));
}

#[test]
fn both_adapters_share_the_same_template_signature() {
    let body = r#"<Box p="4">"你好" {computed()}</Box>"#;
    assert_eq!(
        source_signature(&format!("ui! {{ {body} }}")),
        template_signature(body).map(|signature| format!("ui!{{{signature}}}"))
    );
    assert_eq!(
        template_signature(body),
        template_signature(&body.replace("你好", "更新").replace("4", "8"))
    );
    assert_ne!(
        template_signature(body),
        template_signature(&body.replace("computed()", "different()"))
    );
    assert_ne!(
        template_signature(body),
        template_signature(&body.replace("Box", "Text"))
    );
}

#[test]
fn declaration_add_remove_and_class_order_share_the_compiled_shape() {
    let plain = r#"<Box>"value"</Box>"#;
    let styled = r#"<Box class="hover:bg-red-500 p-4" p="8">"value"</Box>"#;
    assert_eq!(template_signature(plain), template_signature(styled));
    assert_ne!(
        declarations_signature(plain),
        declarations_signature(styled)
    );
    assert_eq!(
        template_signature(styled),
        template_signature(&styled.replace("hover:bg-red-500 p-4", "p-4 hover:bg-red-500"))
    );
    assert_ne!(
        template_signature(styled),
        template_signature(&styled.replace("p=\"8\"", "p={compute()}"))
    );
}

#[test]
fn finite_structure_moves_preserve_the_same_compiled_rust_contract() {
    let before = r#"<Box><Text>"before"</Text>{captured.clone()}</Box>"#;
    let after =
        r#"<Box><HStack p="8">{captured.clone()}<Text>"after"</Text></HStack><Box /></Box>"#;
    assert_eq!(template_signature(before), template_signature(after));
    assert_ne!(
        template_signature(before),
        template_signature(&after.replace("captured.clone()", "new_expression()"))
    );
    assert_ne!(
        template_signature(before),
        template_signature(
            &after
                .replace("<Box>", "<HStack>")
                .replace("</Box>", "</HStack>")
        )
    );
    assert!(template_tree("<Box><Text></Box>").is_none());
}

#[test]
fn raw_strings_are_data_but_rust_slots_remain_opaque() {
    let plain = r#"<Text>"one" {format!("captured")}</Text>"#;
    let raw = r##"<Text>r#"two"# {format!("captured")}</Text>"##;
    assert_eq!(template_signature(plain), template_signature(raw));
    assert_ne!(
        template_signature(plain),
        template_signature(&plain.replace("captured", "changed"))
    );
}
