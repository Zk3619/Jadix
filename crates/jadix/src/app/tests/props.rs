use super::*;

#[test]
fn resolve_children_unpacks_fragment() {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    let frag = frag
        .child(Element::new().text("a"))
        .child(Element::new().text("b"));
    let c = resolve_children(Some(frag));
    assert_eq!(c.len(), 2);
    assert_eq!(c.as_slice()[0].text.as_deref(), Some("a"));
    let packed = c.into_element().expect("frag");
    assert_eq!(
        packed.get_attr("jadix_fragment").map(String::as_str),
        Some("1")
    );
    assert_eq!(packed.children.len(), 2);
}

#[test]
fn merge_base_prop_applies_earlier_then_later() {
    let earlier = Some(Box::new(|mut b: BaseComponentBuilder| {
        b.el.set_attr("a", "1");
        b
    })
        as Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>);
    let later = Some(Box::new(|mut b: BaseComponentBuilder| {
        b.el.set_attr("b", "1");
        b
    })
        as Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>);
    let merged = merge_base_prop(earlier, later).expect("both");
    let out = merged(BaseComponentBuilder::new(Element::new())).el;
    assert_eq!(out.get_attr("a").map(String::as_str), Some("1"));
    assert_eq!(out.get_attr("b").map(String::as_str), Some("1"));
}
