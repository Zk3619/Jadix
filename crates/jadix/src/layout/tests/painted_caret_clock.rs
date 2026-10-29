use super::LayoutTree;

#[test]
fn requests_are_frame_local_and_window_local() {
    let mut primary = LayoutTree::new();
    let mut secondary = LayoutTree::new();
    primary.request_caret_blink();
    assert!(!secondary.take_caret_blink_request());
    assert!(primary.take_caret_blink_request());
    assert!(!primary.take_caret_blink_request());
    secondary.request_caret_blink();
    assert!(!primary.take_caret_blink_request());
    assert!(secondary.take_caret_blink_request());
}
