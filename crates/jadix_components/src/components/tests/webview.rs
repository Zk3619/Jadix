use super::*;

#[test]
fn html_message_handler_survives_reactive_root_hook_replacement() {
    let (_owner, ()) = crate::core::reactivity::create_root(|| {
        let received = Signal::new(String::new());
        let element = webview_html_with_messages(
            "message-lifetime".into(),
            "<p>test</p>".into(),
            move |message| received.set(message),
        );
        let element = element.on_unmount(|| {});
        deliver_message("message-lifetime", "saved".into());
        assert_eq!(received.peek(), "saved");
        drop(element);
        deliver_message("message-lifetime", "stale".into());
        assert_eq!(received.peek(), "saved");
    });
}

#[test]
fn geometry_preserves_document_size_and_clips_the_visible_region() {
    let mut child = Element::new();
    child.set_attr("jadix_webview_id", "geometry");
    child.layout_bounds = [10.0, 20.0, 300.0, 200.0];
    let mut root = Element::new().overflow("hidden").child(child);
    root.layout_bounds = [0.0, 0.0, 150.0, 100.0];
    let specs = collect_specs(&root, [0.0, 0.0, 800.0, 600.0]);
    assert_eq!(specs[0].bounds, [10.0, 20.0, 300.0, 200.0]);
    assert_eq!(specs[0].clip, [10.0, 20.0, 140.0, 80.0]);
    assert!(specs[0].visible);
    root.set_attr("jadix_nav_buried", "1");
    assert!(!collect_specs(&root, [0.0, 0.0, 800.0, 600.0])[0].visible);
}
