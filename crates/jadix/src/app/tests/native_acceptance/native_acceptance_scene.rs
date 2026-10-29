const BODY: &str = r##"<Box w=96.0 h=64.0 bg="#ff0000">"HR6 A"</Box>"##;

#[cfg(feature = "macros")]
fn compiled_static_scene() -> Element {
    crate::ui! { <Box w=96.0 h=64.0 bg="#ff0000">"HR6 A"</Box> }
}

fn compiled_builder_scene() -> Element {
    Element::new()
        .hot_reload_node_type("Box")
        .hot_reload_declaration_base(&["w", "h", "bg"], false)
        .w(96.0)
        .h(64.0)
        .bg("#ff0000")
        .child(Element::new().text("HR6 A"))
}
