use super::*;
use crate::core::tree::Element;

#[test]
fn hot_reload_fold_hover_and_selection_follow_instance_and_expire_on_unmount() {
    let mut root = Element::new().children(vec![
        Element::new().child(Element::new().text("same")),
        Element::new().child(Element::new().text("same")),
    ]);
    let identity = format!("runtime:{}", root.children[0].inspector_instance_id);
    DevToolsState::resolve_selection(&root);
    DevToolsState::set_selected(vec![0], identity.clone(), Vec::new(), (0.0, 0.0));
    DevToolsState::set_hover_highlight(vec![0], identity.clone(), Vec::new(), (0.0, 0.0));
    DevToolsState::toggle_collapsed(&[0]);
    let before = crate::core::hot_reload::instances(&root);
    root.children.swap(0, 1);
    DevToolsState::record_commit(&jadix_hot_reload::CommitAck {
        build: "fixture".into(),
        revision: 1,
        status: jadix_hot_reload::CommitStatus::Applied,
        reason: String::new(),
        changes: crate::core::hot_reload::changes(&before, &root),
    });
    DevToolsState::resolve_selection(&root);
    assert_eq!(DevToolsState::selected_path(), vec![1]);
    assert_eq!(DevToolsState::overlay_path(), vec![1]);
    assert!(!DevToolsState::is_collapsed(&[1]));
    assert!(DevToolsState::is_collapsed(&[0]));
    root.children.remove(1);
    DevToolsState::resolve_selection(&root);
    assert!(DevToolsState::selected_path_key().is_empty());
    assert_eq!(DevToolsState::selected_identity(), identity);
    assert!(!DevToolsState::has_hover_highlight());
    assert!(DevToolsState::is_collapsed(&[0]));
}
