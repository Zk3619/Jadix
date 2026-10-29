#[test]
fn clone_mutation_and_removal_preserve_other_nodes_and_value_types() {
    let mut original = super::State::default();
    original.set(String::from("old"));
    original.set(7_u32);
    let mut cloned = original.clone();
    cloned.get_mut::<String>().unwrap().push_str(" changed");
    cloned.remove::<u32>();
    assert_eq!(original.get::<String>().unwrap(), "old");
    assert_eq!(original.get::<u32>(), Some(&7));
    assert_eq!(cloned.get::<String>().unwrap(), "old changed");
    assert!(cloned.get::<u32>().is_none());
}
