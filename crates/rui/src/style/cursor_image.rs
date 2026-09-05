//! 自定义光标：用 [`crate::media`] 解码后 `from_rgba` → 缓存 `CustomCursor`。
//!
//! 多 `url()` 回退、画布注册表见本模块。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use winit::event_loop::ActiveEventLoop;
use winit::window::{CustomCursor, Window};

use crate::media::{DecodeOptions, DecodedImage, decode_bytes, decode_data_url, load_src};

use super::cursor::{CursorUrl, CursorValue};

type DecodedCursor = DecodedImage;

struct RegistryEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

static REGISTRY: Mutex<Option<HashMap<String, RegistryEntry>>> = Mutex::new(None);

/// 注册一张 RGBA8 光标图（画布栅格化结果）。`url(#id)` 或 `url(cursor:id)` 引用。
///
/// `rgba.len()` 必须为 `width * height * 4`。
pub fn register_cursor_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    let id = id.into();
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        log::warn!("register_cursor_rgba8({id}): invalid size or buffer, ignored");
        return;
    }
    let mut g = REGISTRY.lock().unwrap();
    g.get_or_insert_with(HashMap::new).insert(
        id,
        RegistryEntry {
            width,
            height,
            rgba,
        },
    );
}

fn lookup_registered(id: &str) -> Option<DecodedCursor> {
    let g = REGISTRY.lock().unwrap();
    let map = g.as_ref()?;
    let e = map.get(id)?;
    Some(DecodedCursor {
        width: e.width,
        height: e.height,
        rgba: e.rgba.clone(),
        intrinsic_hotspot: None,
        animation: None,
    })
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct CursorCacheKey {
    src: String,
    hx: u16,
    hy: u16,
}

#[derive(Default)]
pub(crate) struct CustomCursorCache {
    ready: HashMap<CursorCacheKey, CustomCursor>,
    failed: HashSet<CursorCacheKey>,
    decoded: HashMap<String, DecodedCursor>,
    decode_failed: HashSet<String>,
}

impl CustomCursorCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn decoded(&mut self, src: &str) -> Option<DecodedCursor> {
        if let Some(d) = self.decoded.get(src) {
            return Some(d.clone());
        }
        if self.decode_failed.contains(src) {
            return None;
        }
        match load_decoded(src) {
            LoadResult::Ready(d) => {
                self.decoded.insert(src.to_string(), d.clone());
                Some(d)
            }
            LoadResult::FailSticky => {
                self.decode_failed.insert(src.to_string());
                None
            }
            LoadResult::Skip => None,
        }
    }
}

enum LoadResult {
    Ready(DecodedCursor),
    /// 缺文件 / 坏数据 / 远程 URL：记住以免每帧重试。
    FailSticky,
    /// 注册表尚未写入：本帧跳过，稍后可成功。
    Skip,
}

fn registry_id(src: &str) -> Option<&str> {
    let s = src.trim();
    if let Some(rest) = s.strip_prefix('#') {
        return Some(rest);
    }
    if let Some(rest) = s.strip_prefix("cursor:") {
        return Some(rest);
    }
    None
}

fn load_decoded(src: &str) -> LoadResult {
    let src = src.trim();
    if src.is_empty() {
        return LoadResult::FailSticky;
    }
    if let Some(id) = registry_id(src) {
        return match lookup_registered(id) {
            Some(d) => LoadResult::Ready(d),
            None => LoadResult::Skip,
        };
    }
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return LoadResult::FailSticky;
    }
    if lower.starts_with("data:") {
        let Some(bytes) = decode_data_url(src) else {
            return LoadResult::FailSticky;
        };
        return match decode_bytes(&bytes, DecodeOptions::cursor()) {
            Some(d) => LoadResult::Ready(d),
            None => LoadResult::FailSticky,
        };
    }
    match load_src(src, DecodeOptions::cursor()) {
        Some(d) => LoadResult::Ready(d),
        None => LoadResult::FailSticky,
    }
}

fn resolve_hotspot(img: &CursorUrl, decoded: &DecodedCursor) -> Option<(u16, u16)> {
    let w = decoded.width as i32;
    let h = decoded.height as i32;
    let (x, y) = match img.hotspot {
        Some((x, y)) => (x, y),
        None => {
            if let Some((hx, hy)) = decoded.intrinsic_hotspot {
                (hx as i32, hy as i32)
            } else {
                (0, 0)
            }
        }
    };
    if x < 0 || y < 0 || x >= w || y >= h {
        return None;
    }
    Some((x as u16, y as u16))
}

fn try_custom(
    event_loop: &ActiveEventLoop,
    cache: &mut CustomCursorCache,
    img: &CursorUrl,
) -> Option<CustomCursor> {
    let decoded = cache.decoded(&img.src)?;
    let (hx, hy) = resolve_hotspot(img, &decoded)?;
    let key = CursorCacheKey {
        src: img.src.clone(),
        hx,
        hy,
    };
    if let Some(c) = cache.ready.get(&key) {
        return Some(c.clone());
    }
    if cache.failed.contains(&key) {
        return None;
    }
    let source = match CustomCursor::from_rgba(
        decoded.rgba,
        decoded.width as u16,
        decoded.height as u16,
        hx,
        hy,
    ) {
        Ok(s) => s,
        Err(_) => {
            cache.failed.insert(key);
            return None;
        }
    };
    let cursor = event_loop.create_custom_cursor(source);
    cache.ready.insert(key, cursor.clone());
    Some(cursor)
}

pub(crate) fn apply_cursor(
    window: &Window,
    event_loop: Option<&ActiveEventLoop>,
    cache: &mut CustomCursorCache,
    value: Option<CursorValue>,
) {
    let value = value.unwrap_or_else(|| CursorValue::keyword(super::CursorKind::Default));
    if let Some(el) = event_loop {
        for img in &value.images {
            if let Some(c) = try_custom(el, cache, img) {
                window.set_cursor_visible(true);
                window.set_cursor(c);
                return;
            }
        }
    }
    value.kind.apply_to_window(window);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookup() {
        register_cursor_rgba8("test-cur", 1, 1, vec![10, 20, 30, 40]);
        let d = lookup_registered("test-cur").unwrap();
        assert_eq!(d.rgba, vec![10, 20, 30, 40]);
    }

    #[test]
    fn hotspot_out_of_bounds_skips() {
        let d = DecodedCursor {
            width: 8,
            height: 8,
            rgba: vec![0; 8 * 8 * 4],
            intrinsic_hotspot: None,
            animation: None,
        };
        let img = CursorUrl {
            src: "x".into(),
            hotspot: Some((8, 0)),
        };
        assert!(resolve_hotspot(&img, &d).is_none());
        let img = CursorUrl {
            src: "x".into(),
            hotspot: Some((0, 0)),
        };
        assert_eq!(resolve_hotspot(&img, &d), Some((0, 0)));
    }
}
