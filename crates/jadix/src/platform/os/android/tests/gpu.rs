use super::*;

#[test]
fn android_opaque_leads_with_gles() {
    assert_eq!(
        gpu_backend_order(false).first().map(|(_, n)| *n),
        Some("gl")
    );
}
