//! `ui!` auto-lowers `format!(…, signal.get())` text children to `bind_text`.

#[test]
fn format_with_signal_get_lowers_to_bind_text() {
    use rui::core::reactivity::use_signal;
    use rui::core::tree::Element;

    rui::app::reactive::reset_registered_anchors();
    let n = use_signal("__rui_test_fmt_bind_ext", || 0_i32);
    n.set_silent(0);
    let el = rui::ui! {
        <Rect>{ format!("n={}", n.get()) }</Rect>
    };
    fn find_bind(el: &Element) -> bool {
        if el
            .template_id
            .as_deref()
            .is_some_and(|id| id.contains(":bind#"))
            && el.text.as_deref() == Some("n=0")
        {
            return true;
        }
        el.children.iter().any(find_bind)
    }
    assert!(
        find_bind(&el),
        "expected auto bind_text leaf from format!(…, n.get())"
    );
    rui::app::reactive::reset_registered_anchors();
}
