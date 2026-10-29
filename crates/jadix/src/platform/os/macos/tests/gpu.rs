use super::*;

#[test]
fn macos_only_metal() {
    let order = gpu_backend_order(false);
    assert_eq!(
        order.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
        vec!["metal"]
    );
    assert_eq!(gpu_backend_order(true), order);
}
