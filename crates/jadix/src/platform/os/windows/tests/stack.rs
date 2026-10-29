use super::*;

#[test]
fn reserve_matches_link_args() {
    assert_eq!(EXE_STACK_RESERVE, 8 * 1024 * 1024);
    assert!(msvc_link_arg().ends_with(&EXE_STACK_RESERVE.to_string()));
    assert!(gnu_link_arg().contains(&EXE_STACK_RESERVE.to_string()));
}
