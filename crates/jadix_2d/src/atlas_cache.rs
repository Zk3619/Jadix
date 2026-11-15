//! Incremental bounded atlas with LRU eviction and immutable page snapshots.
use crate::{AtlasError, AtlasLimits, Canvas2D, CanvasImageSource, Image2D, ImageSampling};
use std::{collections::HashMap, hash::Hash};

#[derive(Clone, Debug)]
pub struct AtlasEntry {
    pub image: Image2D,
    pub rect: [u32; 4],
}
impl AtlasEntry {
    pub fn draw<C: Canvas2D + ?Sized>(&self, c: &mut C, dest: [f32; 4], sampling: ImageSampling) {
        c.save();
        c.set_image_sampling(sampling);
        c.draw_image(
            &CanvasImageSource::Region {
                image: &self.image,
                rect: self.rect,
            },
            dest[0],
            dest[1],
            dest[2],
            dest[3],
        );
        c.restore();
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasCacheStats {
    pub entries: usize,
    pub pages: usize,
    pub page_bytes: usize,
    pub hits: u64,
    pub insertions: u64,
    pub evictions: u64,
}
struct Entry {
    page: usize,
    allocation: [u32; 4],
    stamp: u64,
}
struct Page {
    image: Image2D,
    free: Vec<[u32; 4]>,
}
/// Keys retain their full Eq identity (not only a fingerprint). Glyph CacheKey
/// can be used directly. Entries returned by get/insert are independent frozen
/// snapshots, so eviction never corrupts a previously prepared display list.
pub struct AtlasCache<K> {
    limits: AtlasLimits,
    pages: Vec<Page>,
    entries: HashMap<K, Entry>,
    clock: u64,
    stats: AtlasCacheStats,
}
impl<K: Eq + Hash + Clone> AtlasCache<K> {
    pub fn new(limits: AtlasLimits) -> Result<Self, AtlasError> {
        // Reuse the immutable packer's shared limit validation before allocation.
        crate::ImageAtlas::pack(&[], limits)?;
        Ok(Self {
            limits,
            pages: Vec::new(),
            entries: HashMap::new(),
            clock: 0,
            stats: Default::default(),
        })
    }
    pub fn stats(&self) -> AtlasCacheStats {
        AtlasCacheStats {
            entries: self.entries.len(),
            pages: self.pages.len(),
            page_bytes: self.pages.len()
                * self.limits.page_size as usize
                * self.limits.page_size as usize
                * 4,
            ..self.stats
        }
    }
    pub fn get(&mut self, key: &K) -> Option<AtlasEntry> {
        self.clock = self.clock.wrapping_add(1);
        let e = self.entries.get_mut(key)?;
        e.stamp = self.clock;
        self.stats.hits += 1;
        let [x, y, w, h] = e.allocation;
        Some(AtlasEntry {
            image: self.pages[e.page].image.clone(),
            rect: [x + 1, y + 1, w - 2, h - 2],
        })
    }
    pub fn remove(&mut self, key: &K) -> bool {
        let Some(e) = self.entries.remove(key) else {
            return false;
        };
        if !self.entries.values().any(|v| v.page == e.page) {
            self.pages[e.page].free = vec![[0, 0, self.limits.page_size, self.limits.page_size]];
            return true;
        }
        let page = &mut self.pages[e.page];
        page.free.push(e.allocation);
        // Coalesce adjacent free rectangles; no moving of surviving entries.
        loop {
            let mut pair = None;
            'outer: for i in 0..page.free.len() {
                for j in i + 1..page.free.len() {
                    let [ax, ay, aw, ah] = page.free[i];
                    let [bx, by, bw, bh] = page.free[j];
                    if ay == by && ah == bh && (ax + aw == bx || bx + bw == ax) {
                        pair = Some((i, j, [ax.min(bx), ay, aw + bw, ah]));
                        break 'outer;
                    }
                    if ax == bx && aw == bw && (ay + ah == by || by + bh == ay) {
                        pair = Some((i, j, [ax, ay.min(by), aw, ah + bh]));
                        break 'outer;
                    }
                }
            }
            let Some((i, j, rect)) = pair else {
                break;
            };
            page.free.swap_remove(j);
            page.free.swap_remove(i);
            page.free.push(rect);
        }
        true
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.pages.clear();
    }
    /// A hit preserves the stored image; remove first when replacing a key's
    /// content. Supports RGBA8 pages; float images are explicitly rejected.
    pub fn insert(&mut self, key: K, image: &Image2D) -> Result<AtlasEntry, AtlasError> {
        if image.linear_rgba32f().is_some() {
            return Err(AtlasError::UnsupportedFormat);
        }
        let side = self.limits.page_size;
        let w = image.width();
        let h = image.height();
        if w > side - 2 || h > side - 2 {
            return Err(AtlasError::EntryTooLarge);
        }
        if let Some(hit) = self.get(&key) {
            return Ok(hit);
        }
        loop {
            if self.entries.len() < self.limits.max_entries {
                if let Some((page, slot)) = self.pages.iter().enumerate().find_map(|(p, v)| {
                    v.free
                        .iter()
                        .position(|r| r[2] >= w + 2 && r[3] >= h + 2)
                        .map(|i| (p, i))
                }) {
                    let [x, y, rw, rh] = self.pages[page].free.swap_remove(slot);
                    let allocation = [x, y, w + 2, h + 2];
                    if rw > w + 2 {
                        self.pages[page]
                            .free
                            .push([x + w + 2, y, rw - w - 2, h + 2]);
                    }
                    if rh > h + 2 {
                        self.pages[page].free.push([x, y + h + 2, rw, rh - h - 2]);
                    }
                    let mut padded = vec![0; ((w + 2) * (h + 2) * 4) as usize];
                    for row in 0..h as usize {
                        let at = ((row + 1) * (w as usize + 2) + 1) * 4;
                        padded[at..at + w as usize * 4].copy_from_slice(
                            &image.rgba8()[row * w as usize * 4..(row + 1) * w as usize * 4],
                        );
                    }
                    self.pages[page].image = self.pages[page]
                        .image
                        .updated_rgba8(allocation, &padded)
                        .unwrap();
                    self.clock = self.clock.wrapping_add(1);
                    self.stats.insertions += 1;
                    self.entries.insert(
                        key.clone(),
                        Entry {
                            page,
                            allocation,
                            stamp: self.clock,
                        },
                    );
                    return Ok(self.get(&key).unwrap());
                }
                if self.pages.len() < self.limits.max_pages as usize {
                    self.pages.push(Page {
                        image: Image2D::from_rgba8(
                            side,
                            side,
                            &vec![0; side as usize * side as usize * 4],
                        )
                        .unwrap(),
                        free: vec![[0, 0, side, side]],
                    });
                    continue;
                }
            }
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.stamp)
                .map(|(k, _)| k.clone());
            let Some(victim) = victim else {
                return Err(AtlasError::BudgetExceeded);
            };
            self.remove(&victim);
            self.stats.evictions += 1;
        }
    }
}
