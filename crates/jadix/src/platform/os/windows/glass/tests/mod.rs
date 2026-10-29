#[test]
fn accent_module_loads() {
    assert!(super::accent::nt_build_number() > 0);
}
