//! Unified GPU texture pool (path / canvas / image / scene3d / capture).
//!
//! Wraps the existing `GpuImageTexture` map with size metadata, per-frame touch,
//! and LRU eviction so later effect/3D passes share one lifetime policy.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use super::graph::PassKind;
use crate::app::image_registry::GpuImageTexture;

const DEFAULT_MAX_ENTRIES: usize = 512;

fn texture_pool_stale_frames() -> Option<u64> {
    static MAX: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("RUI_TEXTURE_POOL_STALE_FRAMES")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
    })
}

#[derive(Clone, Copy, Debug)]
pub struct TextureMeta {
    pub width: u32,
    pub height: u32,
    pub last_frame: u64,
    pub kind: ResourceKind,
}

/// Origin of a pooled texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Path,
    Canvas,
    Image,
    Scene3d,
    Shader,
    Capture,
    Effect,
    /// CSS / API background gradients (`rui-grad:…`).
    Gradient,
    /// Sticky Bake / MSDF text (`rui_text_xform_*` / `rui_text_msdf_*`).
    TextXform,
    Other,
}

impl ResourceKind {
    pub fn from_key(key: &str) -> Self {
        if key.starts_with("rui_path:") {
            Self::Path
        } else if key.starts_with("rui_canvas:") {
            Self::Canvas
        } else if key.starts_with("rui_scene3d:") {
            Self::Scene3d
        } else if key.starts_with("rui_shader:") {
            Self::Shader
        } else if key.starts_with("rui_capture:")
            || key.starts_with("rui_filter_cap:")
            || key.starts_with("rui_zoom_cap:")
        {
            Self::Capture
        } else if key.starts_with("rui_fx:") || key.starts_with("rui_filter:") {
            Self::Effect
        } else if key.starts_with("rui-src:") || key.starts_with("rui_src:") {
            Self::Image
        } else if key.starts_with("rui-grad:") {
            Self::Gradient
        } else if key.starts_with("rui_text_xform_") || key.starts_with("rui_text_msdf_") {
            Self::TextXform
        } else {
            Self::Other
        }
    }

    pub fn pass_kind(self) -> Option<PassKind> {
        match self {
            Self::Path => Some(PassKind::Path2d),
            Self::Canvas | Self::Image | Self::Gradient | Self::TextXform => {
                Some(PassKind::Geometry2d)
            }
            Self::Scene3d => Some(PassKind::Scene3d),
            Self::Shader => Some(PassKind::Shader),
            Self::Capture => Some(PassKind::SubtreeCapture),
            Self::Effect => Some(PassKind::Effect),
            Self::Other => None,
        }
    }
}

pub struct TexturePool {
    map: HashMap<String, GpuImageTexture>,
    meta: HashMap<String, TextureMeta>,
    frame: u64,
    max_entries: usize,
    /// GPU objects created via [`Self::insert_with_size`] (not in-place overwrites).
    create_count: u64,
}

impl TexturePool {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            meta: HashMap::new(),
            frame: 0,
            max_entries: DEFAULT_MAX_ENTRIES,
            create_count: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        if let Some(max_age) = texture_pool_stale_frames() {
            self.evict_stale(max_age);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    pub fn create_count(&self) -> u64 {
        self.create_count
    }

    /// In-place `write_texture` when the pooled slot matches `width`×`height`×`format`.
    /// Does not bump [`Self::create_count`].
    pub fn try_overwrite_rgba8(
        &mut self,
        key: &str,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> bool {
        let Some(gpu) = self.map.get(key) else {
            return false;
        };
        if !crate::app::image_registry::texture_allows_rgba8_overwrite(
            &gpu._texture,
            width,
            height,
            format,
        ) {
            return false;
        }
        if !crate::app::image_registry::write_rgba8_texels(
            queue,
            &gpu._texture,
            rgba,
            width,
            height,
        ) {
            return false;
        }
        self.touch(key);
        true
    }

    /// Entries not touched for more than `max_age` frames (relative to current frame).
    pub fn stale_count(&self, max_age: u64) -> usize {
        if max_age == 0 {
            return 0;
        }
        self.meta
            .values()
            .filter(|m| self.frame.saturating_sub(m.last_frame) > max_age)
            .count()
    }

    /// Drop entries older than `max_age` frames without a draw-time `touch`.
    pub fn evict_stale(&mut self, max_age: u64) {
        if max_age == 0 {
            return;
        }
        let frame = self.frame;
        let stale: Vec<String> = self
            .meta
            .iter()
            .filter(|(_, m)| frame.saturating_sub(m.last_frame) > max_age)
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            self.map.remove(&k);
            self.meta.remove(&k);
        }
    }

    pub fn insert_with_size(&mut self, key: String, tex: GpuImageTexture, width: u32, height: u32) {
        self.create_count = self.create_count.saturating_add(1);
        let kind = ResourceKind::from_key(&key);
        self.meta.insert(
            key.clone(),
            TextureMeta {
                width,
                height,
                last_frame: self.frame,
                kind,
            },
        );
        self.map.insert(key, tex);
        self.evict_if_needed();
    }

    pub fn touch(&mut self, key: &str) {
        if let Some(m) = self.meta.get_mut(key) {
            m.last_frame = self.frame;
        }
    }

    pub fn get(&self, key: &str) -> Option<&GpuImageTexture> {
        self.map.get(key)
    }

    pub fn meta(&self, key: &str) -> Option<TextureMeta> {
        self.meta.get(key).copied()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }

    pub fn as_map(&self) -> &HashMap<String, GpuImageTexture> {
        &self.map
    }

    pub fn as_map_mut(&mut self) -> &mut HashMap<String, GpuImageTexture> {
        &mut self.map
    }

    pub fn evict_if_needed(&mut self) {
        if self.map.len() <= self.max_entries {
            return;
        }
        let frame = self.frame;
        // Never drop textures touched/inserted this frame — they are still referenced by
        // the in-flight draw list. Preferring "keep gradients" previously filled the pool
        // with grads and then thrashing-evicted other (or even current) draws → blank UI.
        let mut aged: Vec<(u64, String)> = self
            .meta
            .iter()
            .filter(|(_, m)| m.last_frame != frame)
            .map(|(k, m)| (m.last_frame, k.clone()))
            .collect();
        aged.sort_by_key(|(f, _)| *f);
        let drop_n = self.map.len().saturating_sub(self.max_entries);
        for (_, k) in aged.into_iter().take(drop_n) {
            self.map.remove(&k);
            self.meta.remove(&k);
        }
        // Soft-cap: if everything left is in-use this frame, allow temporary overshoot.
    }

    pub fn dump(&self) -> String {
        let mut keys: Vec<_> = self.meta.iter().collect();
        keys.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = format!("texture-pool n={} frame={}\n", keys.len(), self.frame);
        for (k, m) in keys {
            out.push_str(&format!(
                "  {k} {}x{} {:?} last={}\n",
                m.width, m.height, m.kind, m.last_frame
            ));
        }
        out
    }
}

impl Default for TexturePool {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for TexturePool {
    type Target = HashMap<String, GpuImageTexture>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for TexturePool {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

#[cfg(test)]
mod tests {
    use super::ResourceKind;

    #[test]
    fn resource_kind_from_key() {
        assert_eq!(ResourceKind::from_key("rui_path:ab"), ResourceKind::Path);
        assert_eq!(
            ResourceKind::from_key("rui_canvas:01"),
            ResourceKind::Canvas
        );
        assert_eq!(
            ResourceKind::from_key("rui_scene3d:8x8"),
            ResourceKind::Scene3d
        );
        assert_eq!(
            ResourceKind::from_key("rui_shader:01"),
            ResourceKind::Shader
        );
        assert_eq!(
            ResourceKind::from_key("rui_capture:x"),
            ResourceKind::Capture
        );
        assert_eq!(
            ResourceKind::from_key("rui_filter_cap:x"),
            ResourceKind::Capture
        );
        assert_eq!(
            ResourceKind::from_key("rui_zoom_cap:x"),
            ResourceKind::Capture
        );
        assert_eq!(ResourceKind::from_key("rui_fx:blur"), ResourceKind::Effect);
        assert_eq!(
            ResourceKind::from_key("rui_filter:1x1"),
            ResourceKind::Effect
        );
        assert_eq!(ResourceKind::from_key("rui-src:a.png"), ResourceKind::Image);
        assert_eq!(
            ResourceKind::from_key("rui-grad:deadbeef"),
            ResourceKind::Gradient
        );
        assert_eq!(
            ResourceKind::from_key("rui_text_xform_v3:abc"),
            ResourceKind::TextXform
        );
        assert_eq!(
            ResourceKind::from_key("rui_text_msdf_v2:abc"),
            ResourceKind::TextXform
        );
    }
}
