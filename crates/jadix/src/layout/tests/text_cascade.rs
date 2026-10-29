use super::*;

#[test]
fn decoration_color_none_is_current_color_not_parent_rgb() {
    // Parent root defaults to white text; child sets primary-like black via attr.
    let mut root = Element::new();
    root.text_style.color = Some([255, 255, 255, 255]);
    root.text_style.decoration_color = None;

    let mut child = Element::new().text("Link").text_color([0, 0, 0, 255]);
    // Stale pinned RGB without attr — cascade must clear back to currentColor.
    child.text_style.decoration_color = Some([255, 255, 255, 255]);
    assert!(!child.attrs.contains_key("text_decoration_color"));
    root.children.push(child);

    cascade_inheritable_text_style_for_test(&mut root);
    let leaf = &root.children[0];
    assert_eq!(leaf.text_style.color_resolved(), [0, 0, 0, 255]);
    assert_eq!(
        leaf.text_style.decoration_color, None,
        "None = currentColor at draw; must not keep parent's white RGB"
    );
}

#[test]
fn decoration_color_explicit_still_inherits() {
    let mut root = Element::new();
    root.text_style.decoration_color = Some([239, 68, 68, 255]); // red-500-ish
    let child = Element::new().text("x").text_color([0, 0, 0, 255]);
    root.children.push(child);

    let parent_text = root.text_style.as_ref().clone();
    cascade_inheritable_text_style(&mut root.inner.children[0], Some(parent_text));
    assert_eq!(
        root.children[0].text_style.decoration_color,
        Some([239, 68, 68, 255])
    );
}
