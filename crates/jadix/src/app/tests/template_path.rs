use super::AppState;
use crate::core::tree::Element;

fn tree() -> Element {
    Element::new().children(vec![
        Element::new().template_id("a"),
        Element::new().children(vec![Element::new().template_id("b")]),
    ])
}

#[test]
fn finds_child_index_path() {
    let root = tree();
    let mut path = Vec::new();
    assert!(AppState::find_template_path(&root, "b", &mut path));
    assert_eq!(path, vec![1, 0]);

    path.clear();
    assert!(!AppState::find_template_path(&root, "missing", &mut path));
    assert!(path.is_empty());
}

#[test]
fn path_validation_rejects_stale_and_out_of_range() {
    let root = tree();
    assert!(AppState::template_path_matches(&root, &[1, 0], "b"));
    // Same shape, wrong node: the cached path must not be trusted blindly.
    assert!(!AppState::template_path_matches(&root, &[1, 0], "a"));
    // Shrunk subtree.
    assert!(!AppState::template_path_matches(&root, &[9], "a"));
}
