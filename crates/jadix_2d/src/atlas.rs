//! Immutable, bounded image pages, also used by prepared font glyphs.
use crate::{Canvas2D, CanvasImageSource, Image2D, ImageSampling};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasLimits {
    pub page_size: u32,
    pub max_pages: u32,
    pub max_entries: usize,
}
impl Default for AtlasLimits {
    fn default() -> Self {
        Self {
            page_size: 512,
            max_pages: 16,
            max_entries: 16384,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasError {
    UnsupportedFormat,
    InvalidLimits,
    EntryTooLarge,
    BudgetExceeded,
}
impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "2D atlas: {self:?}")
    }
}
impl std::error::Error for AtlasError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRegion {
    pub page: usize,
    /// Integer texel rectangle excluding the transparent one-texel gutter.
    pub rect: [u32; 4],
}
#[derive(Clone, Debug)]
pub struct ImageAtlas(Arc<AtlasData>);
#[derive(Debug)]
struct AtlasData {
    pages: Vec<Image2D>,
    regions: Vec<AtlasRegion>,
}
impl ImageAtlas {
    /// Shelf-pack immutable bitmaps in input order. This is an explicit snapshot:
    /// clones retain page identities; repacking creates new identities. No eviction.
    /// All entries and page budgets are validated before pixel allocation.
    pub fn pack(images: &[Image2D], limits: AtlasLimits) -> Result<Self, AtlasError> {
        let side = limits.page_size;
        if !(4..=2048).contains(&side)
            || limits.max_pages == 0
            || limits.max_pages > 64
            || limits.max_entries == 0
            || limits.max_entries > 65536
        {
            return Err(AtlasError::InvalidLimits);
        }
        if images.len() > limits.max_entries {
            return Err(AtlasError::BudgetExceeded);
        }
        let mut regions = Vec::with_capacity(images.len());
        let (mut x, mut y, mut row, mut page) = (0, 0, 0, 0usize);
        for image in images {
            if image.linear_rgba32f().is_some() {
                return Err(AtlasError::UnsupportedFormat);
            }
            let (w, h) = (image.width(), image.height());
            if w > side - 2 || h > side - 2 {
                return Err(AtlasError::EntryTooLarge);
            }
            if x + w + 2 > side {
                x = 0;
                y += row;
                row = 0;
            }
            if y + h + 2 > side {
                page += 1;
                x = 0;
                y = 0;
                row = 0;
            }
            if page >= limits.max_pages as usize {
                return Err(AtlasError::BudgetExceeded);
            }
            regions.push(AtlasRegion {
                page,
                rect: [x + 1, y + 1, w, h],
            });
            x += w + 2;
            row = row.max(h + 2);
        }
        let page_count = regions.last().map_or(0, |r| r.page + 1);
        let mut pixels = vec![vec![0u8; side as usize * side as usize * 4]; page_count];
        for (image, region) in images.iter().zip(&regions) {
            let [x, y, w, h] = region.rect;
            for row in 0..h as usize {
                let dest = ((y as usize + row) * side as usize + x as usize) * 4;
                let src = row * w as usize * 4;
                pixels[region.page][dest..dest + w as usize * 4]
                    .copy_from_slice(&image.rgba8()[src..src + w as usize * 4]);
            }
        }
        let pages = pixels
            .into_iter()
            .map(|p| Image2D::from_rgba8(side, side, &p).unwrap())
            .collect();
        Ok(Self(Arc::new(AtlasData { pages, regions })))
    }
    #[cfg(feature = "text-layout")]
    pub(crate) fn from_entries(entries: &[crate::AtlasEntry]) -> Self {
        let mut pages = Vec::new();
        let mut regions = Vec::new();
        for entry in entries {
            let page = pages
                .iter()
                .position(|p| p == &entry.image)
                .unwrap_or_else(|| {
                    pages.push(entry.image.clone());
                    pages.len() - 1
                });
            regions.push(AtlasRegion {
                page,
                rect: entry.rect,
            });
        }
        Self(Arc::new(AtlasData { pages, regions }))
    }
    pub fn pages(&self) -> &[Image2D] {
        &self.0.pages
    }
    pub fn regions(&self) -> &[AtlasRegion] {
        &self.0.regions
    }
    pub fn byte_len(&self) -> usize {
        self.0.pages.iter().map(|p| p.rgba8().len()).sum()
    }
    /// Paint an entry through the cropped image pipeline. Inherits CTM,
    /// alpha, clip, mask and group filters; saves/restores caller drawing state.
    /// Invalid indices or nonpositive/nonfinite destination geometry paint nothing.
    pub fn draw<C: Canvas2D + ?Sized>(
        &self,
        canvas: &mut C,
        index: usize,
        dest: [f32; 4],
        sampling: ImageSampling,
    ) {
        let Some(region) = self.0.regions.get(index) else {
            return;
        };
        let [x, y, w, h] = dest;
        if dest.iter().any(|v| !v.is_finite()) || w <= 0. || h <= 0. {
            return;
        }
        canvas.save();
        canvas.set_image_sampling(sampling);
        canvas.draw_image(
            &CanvasImageSource::Region {
                image: &self.0.pages[region.page],
                rect: region.rect,
            },
            x,
            y,
            w,
            h,
        );
        canvas.restore();
    }
}
