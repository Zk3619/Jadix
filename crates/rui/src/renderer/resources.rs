//! Unified GPU texture pool (path / canvas / image / scene3d / capture).
//!
//! Wraps the existing `GpuImageTexture` map with size metadata, per-frame touch,
//! and LRU eviction so later effect/3D passes share one lifetime policy.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use super::graph::PassKind;
use crate::app::image_registry::GpuImageTexture;

const DEFAULT_MAX_ENTRIES: usize = 256;

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
    Capture,
    Effect,
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
        } else if key.starts_with("rui_capture:") {
            Self::Capture
        } else if key.starts_with("rui_fx:") {
            Self::Effect
        } else if key.starts_with("rui-src:") || key.starts_with("rui_src:") {
            Self::Image
        } else {
            Self::Other
        }
    }

    pub fn pass_kind(self) -> Option<PassKind> {
        match self {
            Self::Path | Self::Canvas | Self::Image => Some(PassKind::Geometry2d),
            Self::Scene3d => Some(PassKind::Scene3d),
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
}

impl TexturePool {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            meta: HashMap::new(),
            frame: 0,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    pub fn insert_with_size(&mut self, key: String, tex: GpuImageTexture, width: u32, height: u32) {
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
        let mut aged: Vec<(u64, String)> = self
            .meta
            .iter()
            .map(|(k, m)| (m.last_frame, k.clone()))
            .collect();
        aged.sort_by_key(|(f, _)| *f);
        let drop_n = self.map.len().saturating_sub(self.max_entries);
        for (_, k) in aged.into_iter().take(drop_n) {
            self.map.remove(&k);
            self.meta.remove(&k);
        }
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
            ResourceKind::from_key("rui_capture:x"),
            ResourceKind::Capture
        );
        assert_eq!(ResourceKind::from_key("rui_fx:blur"), ResourceKind::Effect);
        assert_eq!(ResourceKind::from_key("rui-src:a.png"), ResourceKind::Image);
    }
}
