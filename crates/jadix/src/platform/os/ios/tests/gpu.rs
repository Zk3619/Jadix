use super::*;

#[test]
fn ios_only_metal() {
    assert_eq!(
        gpu_backend_order(false)
            .iter()
            .map(|(_, n)| *n)
            .collect::<Vec<_>>(),
        vec!["metal"]
    );
    assert_eq!(gpu_backend_order(true), gpu_backend_order(false));
}
