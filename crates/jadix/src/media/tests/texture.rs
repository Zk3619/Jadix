use super::*;
use crate::app::pending_images::take_pending_key;

#[test]
fn window_draws_rehydrate_retained_pixels_after_eviction() {
    let texture = RgbaTexture::new("retained-rehydrate", 1, 1, vec![1, 2, 3, 255]).unwrap();
    take_pending_key(texture.key());
    flush_retained_textures([texture.key()], |_| true);
    assert!(take_pending_key(texture.key()).is_none());
    for _window in 0..2 {
        flush_retained_textures([texture.key()], |_| false);
        assert_eq!(
            take_pending_key(texture.key()),
            Some((vec![1, 2, 3, 255], 1, 1))
        );
    }
    drop(texture);
    flush_retained_textures(["retained-rehydrate"], |_| false);
    assert!(take_pending_key("retained-rehydrate").is_none());
}

#[test]
fn old_handle_drop_cannot_remove_replacement_pixels() {
    let old = RgbaTexture::new("retained-replace", 1, 1, vec![0; 4]).unwrap();
    let new = RgbaTexture::new("retained-replace", 1, 1, vec![255; 4]).unwrap();
    take_pending_key(new.key());
    drop(old);
    flush_retained_textures([new.key()], |_| false);
    assert_eq!(take_pending_key(new.key()).unwrap().0, vec![255; 4]);
}

#[test]
fn invalid_dimensions_are_rejected_without_uploads() {
    assert!(RgbaTexture::new("retained-invalid", 0, 1, vec![]).is_none());
    assert!(RgbaTexture::new("retained-invalid", 2, 1, vec![0; 4]).is_none());
    assert!(take_pending_key("retained-invalid").is_none());
}
