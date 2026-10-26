//! Retained CPU pixels for generated textures, rehydrated by each window's draw list.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

static RETAINED: Mutex<Option<HashMap<String, Weak<TextureData>>>> = Mutex::new(None);

struct TextureData {
    key: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Own generated RGBA8 pixels while any component or application needs them.
/// Replacing a key makes its newest live handle authoritative. Each window uploads
/// missing pixels only for keys in its own collected draw list.
#[derive(Clone)]
pub struct RgbaTexture(Arc<TextureData>);

impl RgbaTexture {
    /// Retain tightly packed RGBA8 pixels; reject empty or inconsistent dimensions.
    pub fn new(key: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let length = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != length {
            return None;
        }
        let data = Arc::new(TextureData {
            key: key.into(),
            width,
            height,
            rgba,
        });
        RETAINED
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(data.key.clone(), Arc::downgrade(&data));
        crate::queue_image_rgba8(data.key.clone(), width, height, data.rgba.clone());
        Some(Self(data))
    }

    pub fn key(&self) -> &str {
        &self.0.key
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn rgba(&self) -> &[u8] {
        &self.0.rgba
    }
}

impl Drop for TextureData {
    fn drop(&mut self) {
        let mut retained = RETAINED.lock().unwrap();
        if let Some(entries) = retained.as_mut() {
            if entries
                .get(&self.key)
                .is_some_and(|entry| std::ptr::eq(entry.as_ptr(), self))
            {
                entries.remove(&self.key);
            }
        }
    }
}

pub(crate) fn flush_retained_textures<'key>(
    keys: impl IntoIterator<Item = &'key str>,
    mut has_gpu: impl FnMut(&str) -> bool,
) {
    for key in keys {
        if has_gpu(key) {
            continue;
        }
        let data = RETAINED
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|entries| entries.get(key))
            .and_then(Weak::upgrade);
        if let Some(data) = data {
            crate::queue_image_rgba8(key, data.width, data.height, data.rgba.clone());
        }
    }
}

#[cfg(test)]
mod tests {
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
}
