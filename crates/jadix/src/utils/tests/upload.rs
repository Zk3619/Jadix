use super::*;

#[test]
fn cached_icon_uploads_only_on_gpu_miss() {
    let ensure = || {
        ensure_lucide_icon_texture(Some("circle"), "", 31.25, [31, 47, 63, 255], 1.0)
            .unwrap()
            .0
    };
    let key = ensure();
    assert!(crate::app::pending_images::take_pending_key(&key).is_none());
    assert!(flush_icon_textures(|_| false));
    assert!(crate::app::pending_images::take_pending_key(&key).is_some());
    assert_eq!(ensure(), key);
    assert!(!flush_icon_textures(|_| true));
    assert!(crate::app::pending_images::take_pending_key(&key).is_none());
    assert_eq!(ensure(), key);
    assert!(flush_icon_textures(|_| false));
    assert!(crate::app::pending_images::take_pending_key(&key).is_some());
}
