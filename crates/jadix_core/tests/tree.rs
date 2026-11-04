use jadix_core::{tree::*, virtualization::VirtualAxis};

fn branch(key: u64, children: Children<u64>) -> TreeNode<u64> {
    TreeNode {
        key,
        label: format!("Folder {key} / 中文 😀"),
        enabled: true,
        children,
    }
}

#[test]
fn keyboard_navigation_range_selection_and_collapse_restore_visible_parent() {
    let mut disabled = TreeNode::leaf(2, "Disabled");
    disabled.enabled = false;
    let mut tree = Tree::new(
        vec![
            branch(
                0,
                Children::Loaded(vec![
                    TreeNode::leaf(1, "One"),
                    disabled,
                    TreeNode::leaf(3, "Three"),
                ]),
            ),
            TreeNode::leaf(4, "Four"),
        ],
        TreeLimits::default(),
    )
    .unwrap();
    assert_eq!(tree.focused(), Some(&0));
    assert_eq!(tree.focus(&1), Err(TreeError::HiddenKey));
    tree.command(TreeCommand::Child).unwrap();
    assert!(tree.expanded(&0));
    tree.command(TreeCommand::Child).unwrap();
    assert_eq!(tree.focused(), Some(&1));
    tree.select(&1, false, false).unwrap();
    tree.command(TreeCommand::Next).unwrap();
    assert_eq!(tree.focused(), Some(&3));
    tree.select(&4, true, false).unwrap();
    assert_eq!(
        tree.selected().iter().copied().collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    tree.focus(&3).unwrap();
    tree.set_expanded(&0, false).unwrap();
    assert_eq!(tree.focused(), Some(&0));
    assert_eq!(
        tree.visible().iter().map(|row| row.key).collect::<Vec<_>>(),
        vec![0, 4]
    );
    tree.command(TreeCommand::Last).unwrap();
    assert_eq!(tree.focused(), Some(&4));
    tree.command(TreeCommand::First).unwrap();
    assert_eq!(tree.focused(), Some(&0));
    tree.set_expanded(&0, true).unwrap();
    assert_eq!(
        tree.visible()[1],
        TreeRow {
            key: 1,
            depth: 1,
            position: 1,
            siblings: 3
        }
    );
    tree.focus(&3).unwrap();
    tree.command(TreeCommand::Parent).unwrap();
    assert_eq!(tree.focused(), Some(&0));
    tree.select(&3, false, true).unwrap();
    assert!(!tree.selected().contains(&3));
    assert_eq!(tree.select(&2, true, false), Err(TreeError::Disabled));
}

#[test]
fn lazy_completions_are_atomic_bound_to_tree_and_cancelled_generations() {
    let mut tree = Tree::new(vec![branch(0, Children::Unloaded)], TreeLimits::default()).unwrap();
    let request = tree.set_expanded(&0, true).unwrap().unwrap();
    assert!(tree.loading(&0));
    assert!(tree.set_expanded(&0, true).unwrap().is_none());
    let mut other = Tree::new(vec![branch(0, Children::Unloaded)], TreeLimits::default()).unwrap();
    let other_request = other.set_expanded(&0, true).unwrap().unwrap();
    assert_eq!(
        tree.complete_load(&other_request, vec![]),
        Err(TreeError::StaleLoad)
    );
    tree.set_expanded(&0, false).unwrap();
    assert_eq!(
        tree.complete_load(&request, vec![TreeNode::leaf(1, "late")]),
        Err(TreeError::StaleLoad)
    );
    let next = tree.set_expanded(&0, true).unwrap().unwrap();
    assert!(next.generation() > request.generation());
    assert_eq!(
        tree.complete_load(&next, vec![TreeNode::leaf(0, "duplicate")]),
        Err(TreeError::DuplicateKey)
    );
    assert_eq!(tree.len(), 1);
    assert!(tree.loading(&0));
    tree.cancel_load(&next).unwrap();
    assert_eq!(tree.complete_load(&next, vec![]), Err(TreeError::StaleLoad));
    let final_request = tree.set_expanded(&0, true).unwrap().unwrap();
    tree.complete_load(&final_request, vec![TreeNode::leaf(1, "ready / 中文 😀")])
        .unwrap();
    assert_eq!(tree.visible().len(), 2);
    assert!(!tree.loading(&0));
    assert_eq!(tree.label(&1), Some("ready / 中文 😀"));
    assert_eq!(
        tree.complete_load(&final_request, vec![]),
        Err(TreeError::StaleLoad)
    );
}

#[test]
fn large_tree_virtual_window_keeps_business_keys_and_limits_mounting() {
    let nodes = (0..100_000)
        .map(|key| TreeNode::leaf(key, format!("File {key}")))
        .collect();
    let mut tree = Tree::new(nodes, TreeLimits::default()).unwrap();
    tree.focus(&50_000).unwrap();
    tree.select(&50_000, false, false).unwrap();
    let axis = VirtualAxis::new(tree.visible().iter().map(|row| row.key), 32.0).unwrap();
    let window = axis
        .window(1_600_000.0, 480.0, 64.0, &[50_000], 128)
        .unwrap();
    assert!(window.mounted.len() < 25);
    assert!(window
        .mounted
        .iter()
        .any(|index| axis.key(*index) == Some(&50_000)));
    assert_eq!(tree.focused(), Some(&50_000));
    assert_eq!(tree.len(), 100_000);
}

#[test]
fn invalid_load_never_spends_node_text_or_depth_budget() {
    let limits = TreeLimits {
        nodes: 3,
        depth: 1,
        label_bytes: 128,
    };
    let mut tree = Tree::new(vec![branch(0, Children::Unloaded)], limits).unwrap();
    let request = tree.set_expanded(&0, true).unwrap().unwrap();
    assert_eq!(
        tree.complete_load(
            &request,
            vec![
                TreeNode::leaf(1, "a"),
                TreeNode::leaf(2, "b"),
                TreeNode::leaf(3, "c")
            ]
        ),
        Err(TreeError::Budget)
    );
    assert_eq!(
        tree.complete_load(
            &request,
            vec![branch(1, Children::Loaded(vec![TreeNode::leaf(2, "deep")]))]
        ),
        Err(TreeError::Budget)
    );
    assert_eq!(
        tree.complete_load(&request, vec![TreeNode::leaf(1, "界".repeat(100))]),
        Err(TreeError::Budget)
    );
    assert_eq!(tree.len(), 1);
    tree.complete_load(&request, vec![TreeNode::leaf(1, "valid")])
        .unwrap();
    assert_eq!(tree.len(), 2);
    let duplicate = Tree::new(vec![TreeNode::leaf(0, "a"), TreeNode::leaf(0, "b")], limits);
    assert!(matches!(duplicate, Err(TreeError::DuplicateKey)));
}

#[test]
fn collapsing_an_ancestor_cancels_descendant_loads_and_initial_shift_selection_uses_focus() {
    let mut tree = Tree::new(
        vec![
            branch(0, Children::Loaded(vec![branch(1, Children::Unloaded)])),
            TreeNode::leaf(2, "last"),
        ],
        TreeLimits::default(),
    )
    .unwrap();
    tree.select(&2, true, false).unwrap();
    assert_eq!(
        tree.selected().iter().copied().collect::<Vec<_>>(),
        vec![0, 2]
    );
    tree.set_expanded(&0, true).unwrap();
    let request = tree.set_expanded(&1, true).unwrap().unwrap();
    assert!(tree.request_pending(&request));
    tree.set_expanded(&0, false).unwrap();
    assert!(!tree.request_pending(&request));
    assert_eq!(
        tree.complete_load(&request, vec![]),
        Err(TreeError::StaleLoad)
    );
    tree.set_expanded(&0, true).unwrap();
    let next = tree.set_expanded(&1, true).unwrap().unwrap();
    assert!(next.generation() > request.generation());
}
