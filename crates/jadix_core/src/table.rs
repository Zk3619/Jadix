//! Keyed table order and column geometry, independent of the rendering host.
use crate::virtualization::{ListScrollPosition, VirtualAxis, VirtualError};
pub use crate::virtualization::VirtualError as TableError;
use std::{cmp::Ordering, collections::{BTreeMap, BTreeSet}, ops::Range};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnPin { Left, #[default] None, Right }

#[derive(Clone, Debug, PartialEq)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    pub width: f64,
    pub pin: ColumnPin,
}
impl TableColumn {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self { key: key.into(), label: label.into(), width: 160., pin: ColumnPin::None }
    }
    pub fn width(mut self, width: f64) -> Self { self.width = width; self }
    pub fn pin(mut self, pin: ColumnPin) -> Self { self.pin = pin; self }
}

pub struct TableColumns {
    columns: BTreeMap<String, TableColumn>,
    bands: [VirtualAxis<String>; 3],
    order: Vec<String>,
    indices: BTreeMap<String, usize>,
}

#[derive(Debug, PartialEq)]
pub struct ColumnWindow {
    pub keys: Vec<String>,
    pub visible: Range<usize>,
    pub leading: f64,
    pub trailing: f64,
    pub offset: f64,
}

impl TableColumns {
    pub fn new(columns: Vec<TableColumn>) -> Result<Self, VirtualError> {
        let mut bands = [Vec::new(), Vec::new(), Vec::new()];
        let mut values = BTreeMap::new();
        for column in columns {
            if !column.width.is_finite() || column.width <= 0. || column.width > f32::MAX as f64 {
                return Err(VirtualError::InvalidExtent);
            }
            let band = match column.pin { ColumnPin::Left => 0, ColumnPin::None => 1, ColumnPin::Right => 2 };
            bands[band].push((column.key.clone(), column.width));
            if values.insert(column.key.clone(), column).is_some() { return Err(VirtualError::DuplicateKey); }
        }
        let mut axes = Vec::new();
        let mut order = Vec::new();
        for entries in bands {
            let mut axis = VirtualAxis::new(Vec::<String>::new(), 1.)?;
            order.extend(entries.iter().map(|(key, _)| key.clone()));
            axis.append(entries)?;
            if axis.total() > f32::MAX as f64 { return Err(VirtualError::InvalidExtent); }
            axes.push(axis);
        }
        let indices = order.iter().enumerate().map(|(index, key)| (key.clone(), index)).collect();
        Ok(Self { columns: values, bands: axes.try_into().ok().unwrap(), order, indices })
    }
    pub fn get(&self, key: &str) -> Option<&TableColumn> { self.columns.get(key) }
    pub fn order(&self) -> &[String] { &self.order }
    pub fn index(&self, key: &str) -> Option<usize> { self.indices.get(key).copied() }
    pub fn band(&self, band: usize) -> &VirtualAxis<String> { &self.bands[band] }
    pub fn viewport(&self, width: f64) -> Result<f64, VirtualError> {
        Ok(self.band_viewports(width)?[1])
    }
    /// Keep the center usable when pinned columns exceed the available width.
    /// Each band remains scrollable, including fixed columns outside its clip.
    pub fn band_viewports(&self, width: f64) -> Result<[f64; 3], VirtualError> {
        if !width.is_finite() || width < 0. { return Err(VirtualError::InvalidExtent); }
        let left = self.bands[0].total(); let right = self.bands[2].total();
        let total = left + right;
        let budget = if self.bands[1].is_empty() { width } else { width * (2. / 3.) };
        let scale = if total > budget { budget / total } else { 1. };
        Ok([left * scale, (width - total * scale).max(0.), right * scale])
    }
    pub fn window(&self, offset: f64, viewport: f64, overscan: usize) -> Result<ColumnWindow, VirtualError> {
        self.band_window(1, offset, viewport, overscan)
    }
    pub fn band_window(&self, band: usize, offset: f64, viewport: f64, overscan: usize) -> Result<ColumnWindow, VirtualError> {
        let axis = self.bands.get(band).ok_or(VirtualError::UnknownKey)?;
        let window = axis.viewport_geometry(offset, viewport)?;
        let range = if viewport == 0. { 0..0 } else {
            window.visible.start.saturating_sub(overscan)..window.visible.end.saturating_add(overscan).min(axis.len())
        };
        let leading = axis.position(range.start).unwrap();
        let trailing = axis.total() - axis.position(range.end).unwrap();
        Ok(ColumnWindow { keys: range.map(|i| axis.key(i).unwrap().clone()).collect(), visible: window.visible, leading, trailing, offset: window.offset })
    }
    /// Keep either content edge anchored when columns are resized/reordered.
    /// Otherwise preserve the first visible key, falling forward if it vanished.
    pub fn remap_offset(&self, previous: &Self, offset: f64, old_width: f64, width: f64) -> Result<f64, VirtualError> {
        let old = previous.bands[1].viewport_geometry(offset, previous.viewport(old_width)?)?;
        let viewport = self.viewport(width)?;
        let maximum = (self.bands[1].total() - viewport).max(0.);
        let old_maximum = (previous.bands[1].total() - old.viewport_extent).max(0.);
        if old.offset <= 0.01 { return Ok(0.); }
        if old.offset >= old_maximum - 0.01 { return Ok(maximum); }
        let position = ListScrollPosition { index: old.visible.start,
            offset: old.offset - previous.bands[1].position(old.visible.start).unwrap() };
        let old_keys: Vec<_> = (0..previous.bands[1].len()).map(|i| previous.bands[1].key(i).unwrap().clone()).collect();
        Ok(position.remap_keys(&old_keys, |key| self.bands[1].index(key))?
            .map(|position| self.bands[1].restore_position(position, viewport)).transpose()?.unwrap_or(0.))
    }
    pub fn reveal(&self, key: &str, offset: f64, width: f64) -> Result<f64, VirtualError> {
        self.reveal_in_band(key, offset, width).map(|(band, next)| if band == 1 { next } else { offset })
    }
    pub fn reveal_in_band(&self, key: &str, offset: f64, width: f64) -> Result<(usize, f64), VirtualError> {
        let key = key.to_string();
        let column = self.columns.get(&key).ok_or(VirtualError::UnknownKey)?;
        let band = match column.pin { ColumnPin::Left => 0, ColumnPin::None => 1, ColumnPin::Right => 2 };
        let axis = &self.bands[band]; let viewport = self.band_viewports(width)?[band];
        let index = axis.index(&key).ok_or(VirtualError::UnknownKey)?;
        let start = axis.position(index).unwrap();
        let end = axis.position(index + 1).unwrap();
        let offset = if start < offset { start } else if end > offset + viewport { (end - viewport).max(0.) } else { offset };
        Ok((band, axis.viewport_geometry(offset, viewport)?.offset))
    }
}

/// Dataset keys and a filtered/sorted/paginated view. Payloads and editor drafts
/// remain in the caller's keyed store; scrolling never evaluates the query.
pub struct TableRows {
    all: Vec<String>,
    visible: Vec<String>,
    indices: BTreeMap<String, usize>,
    selected: BTreeSet<String>,
}
impl TableRows {
    pub fn new(keys: Vec<String>) -> Result<Self, VirtualError> {
        VirtualAxis::new(keys.clone(), 1.)?;
        let indices = keys.iter().enumerate().map(|(i, key)| (key.clone(), i)).collect();
        Ok(Self { all: keys.clone(), visible: keys, indices, selected: BTreeSet::new() })
    }
    pub fn visible(&self) -> &[String] { &self.visible }
    pub fn index(&self, key: &str) -> Option<usize> { self.indices.get(key).copied() }
    pub fn selected(&self) -> &BTreeSet<String> { &self.selected }
    pub fn set_rows(&mut self, keys: Vec<String>) -> Result<(), VirtualError> {
        let mut next = Self::new(keys)?;
        next.selected = self.selected.iter().filter(|key| next.indices.contains_key(*key)).cloned().collect();
        *self = next;
        Ok(())
    }
    pub fn select(&mut self, key: &str, additive: bool) -> Result<(), VirtualError> {
        if !self.indices.contains_key(key) { return Err(VirtualError::UnknownKey); }
        if !additive { self.selected.clear(); }
        if !self.selected.insert(key.into()) && additive { self.selected.remove(key); }
        Ok(())
    }
    /// The last page is clamped after filtering; a zero page size is rejected
    /// before changing the old view or selection.
    pub fn query(&mut self, mut filter: impl FnMut(&str) -> bool,
        mut compare: impl FnMut(&str, &str) -> Ordering, page: Option<(usize, usize)>) -> Result<(), VirtualError> {
        if page.is_some_and(|(_, size)| size == 0) { return Err(VirtualError::InvalidExtent); }
        let mut keys: Vec<_> = self.all.iter().filter(|key| filter(key)).cloned().collect();
        keys.sort_by(|a, b| compare(a, b));
        if let Some((index, size)) = page {
            let index = index.min(keys.len().saturating_sub(1) / size);
            keys = keys.into_iter().skip(index * size).take(size).collect();
        }
        self.indices = keys.iter().enumerate().map(|(i, key)| (key.clone(), i)).collect();
        self.visible = keys;
        Ok(())
    }
}
