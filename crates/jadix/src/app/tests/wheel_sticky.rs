use super::wheel_sticky_matches_deepest;

#[test]
fn nested_deepest_breaks_outer_sticky() {
    let chain = vec!["inner".into(), "outer".into()];
    assert!(!wheel_sticky_matches_deepest("outer", &chain));
    assert!(wheel_sticky_matches_deepest("inner", &chain));
}

#[test]
fn empty_chain_does_not_keep_sticky() {
    assert!(!wheel_sticky_matches_deepest("outer", &[]));
}
