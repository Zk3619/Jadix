//! `ui!` auto-lowers `format!(…, signal.get())` text children to `bind_text`.

#[test]
fn format_with_signal_get_lowers_to_bind_text() {
    use jadix::core::reactivity::use_signal;
    use jadix::core::tree::Element;

    jadix::app::reactive::reset_registered_anchors();
    let n = use_signal("__jadix_test_fmt_bind_ext", || 0_i32);
    n.set_silent(0);
    let el = jadix::ui! {
        <Box>{ format!("n={}", n.get()) }</Box>
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
    jadix::app::reactive::reset_registered_anchors();
}
