use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualError {
    InvalidExtent,
    DuplicateKey,
    UnknownKey,
    Capacity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScrollAnchor<Key> {
    pub key: Key,
    pub within: f64,
}

/// The first row intersecting the viewport and its depth in that row, in
/// logical coordinates. This excludes overscan and pinned rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListScrollPosition {
    pub index: usize,
    pub offset: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VirtualWindow {
    pub visible: Range<usize>,
    pub mounted: Vec<usize>,
    pub offset: f64,
    pub total: f64,
}

pub struct VirtualAxis<Key> {
    keys: Vec<Key>,
    indices: BTreeMap<Key, usize>,
    extents: Vec<f64>,
    sums: Vec<f64>,
    total: f64,
}

impl<Key: Clone + Ord> VirtualAxis<Key> {
    pub fn new(keys: impl IntoIterator<Item = Key>, estimate: f64) -> Result<Self, VirtualError> {
        if !estimate.is_finite() || estimate <= 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        Self::from_extents(keys.into_iter().map(|key| (key, estimate)))
    }

    fn from_extents(items: impl IntoIterator<Item = (Key, f64)>) -> Result<Self, VirtualError> {
        let mut result = Self {
            keys: Vec::new(),
            indices: BTreeMap::new(),
            extents: Vec::new(),
            sums: vec![0.0],
            total: 0.0,
        };
        for (key, extent) in items {
            if result.keys.len() == 1_000_000 {
                return Err(VirtualError::Capacity);
            }
            if !extent.is_finite() || extent <= 0.0 || !(result.total + extent).is_finite() {
                return Err(VirtualError::InvalidExtent);
            }
            let index = result.keys.len();
            if result.indices.insert(key.clone(), index).is_some() {
                return Err(VirtualError::DuplicateKey);
            }
            result.keys.push(key);
            result.extents.push(extent);
            result.total += extent;
        }
        result.sums.resize(result.keys.len() + 1, 0.0);
        for (index, extent) in result.extents.iter().copied().enumerate() {
            let index = index + 1;
            result.sums[index] += extent;
            let parent = index + (index & index.wrapping_neg());
            if parent < result.sums.len() {
                result.sums[parent] += result.sums[index];
            }
        }
        Ok(result)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn key(&self, index: usize) -> Option<&Key> {
        self.keys.get(index)
    }

    pub fn index(&self, key: &Key) -> Option<usize> {
        self.indices.get(key).copied()
    }

    pub fn total(&self) -> f64 {
        self.total
    }

    pub fn extent(&self, key: &Key) -> Option<f64> {
        self.index(key).map(|index| self.extents[index])
    }

    pub fn position(&self, index: usize) -> Option<f64> {
        if index > self.len() {
            return None;
        }
        let mut cursor = index;
        let mut sum = 0.0;
        while cursor != 0 {
            sum += self.sums[cursor];
            cursor -= cursor & cursor.wrapping_neg();
        }
        Some(sum)
    }

    pub fn measure(&mut self, key: &Key, extent: f64) -> Result<bool, VirtualError> {
        if !extent.is_finite() || extent <= 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        let index = self.index(key).ok_or(VirtualError::UnknownKey)?;
        let difference = extent - self.extents[index];
        let total = self.total + difference;
        if !total.is_finite() || total <= 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        if difference == 0.0 {
            return Ok(false);
        }
        let mut cursor = index + 1;
        while cursor < self.sums.len() {
            self.sums[cursor] += difference;
            cursor += cursor & cursor.wrapping_neg();
        }
        self.extents[index] = extent;
        self.total = total;
        Ok(true)
    }

    /// Append keyed extents without rebuilding the existing prefix index.
    ///
    /// Validation is atomic: duplicate keys, invalid extents and capacity errors
    /// leave the axis unchanged. Work is amortized O(k log(n + k)) for k new
    /// rows, including ordered-key lookups; Vec growth can copy existing storage.
    pub fn append(
        &mut self,
        items: impl IntoIterator<Item = (Key, f64)>,
    ) -> Result<usize, VirtualError> {
        let mut pending = Vec::new();
        let mut seen = BTreeSet::new();
        let mut total = self.total;
        for (key, extent) in items {
            if pending.len() >= 1_000_000 - self.len() {
                return Err(VirtualError::Capacity);
            }
            if !extent.is_finite() || extent <= 0.0 || !(total + extent).is_finite() {
                return Err(VirtualError::InvalidExtent);
            }
            if self.indices.contains_key(&key) || !seen.insert(key.clone()) {
                return Err(VirtualError::DuplicateKey);
            }
            total += extent;
            pending.push((key, extent));
        }
        let count = pending.len();
        self.keys.reserve(count);
        self.extents.reserve(count);
        self.sums.reserve(count);
        for (key, extent) in pending {
            let index = self.len();
            let cursor = index + 1;
            let start = cursor - (cursor & cursor.wrapping_neg());
            // A new Fenwick node may span old rows (especially at powers of two).
            let mut sum = extent;
            let mut previous = index;
            while previous > start {
                sum += self.sums[previous];
                previous -= previous & previous.wrapping_neg();
            }
            self.indices.insert(key.clone(), index);
            self.keys.push(key);
            self.extents.push(extent);
            self.sums.push(sum);
        }
        self.total = total;
        Ok(count)
    }

    pub fn replace(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
        estimate: f64,
    ) -> Result<(), VirtualError> {
        *self = self.replacement(keys, estimate)?;
        Ok(())
    }

    pub fn replacement(
        &self,
        keys: impl IntoIterator<Item = Key>,
        estimate: f64,
    ) -> Result<Self, VirtualError> {
        if !estimate.is_finite() || estimate <= 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        Self::from_extents(keys.into_iter().map(|key| {
            let extent = self.extent(&key).unwrap_or(estimate);
            (key, extent)
        }))
    }

    fn at(&self, offset: f64) -> usize {
        let mut index = 0;
        let mut sum = 0.0;
        let mut step = self.sums.len().next_power_of_two() / 2;
        while step > 0 {
            let next = index + step;
            if next < self.sums.len() && sum + self.sums[next] <= offset {
                sum += self.sums[next];
                index = next;
            }
            step /= 2;
        }
        index
    }

    fn viewport(offset: f64, viewport: f64) -> Result<(), VirtualError> {
        if [offset, viewport]
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
        {
            Err(VirtualError::InvalidExtent)
        } else {
            Ok(())
        }
    }

    pub fn anchor(&self, offset: f64) -> Result<Option<ScrollAnchor<Key>>, VirtualError> {
        Self::viewport(offset, 0.0)?;
        if self.is_empty() {
            return Ok(None);
        }
        let index = self.at(offset).min(self.len() - 1);
        Ok(Some(ScrollAnchor {
            key: self.keys[index].clone(),
            within: (offset - self.position(index).unwrap()).clamp(0.0, self.extents[index]),
        }))
    }

    pub fn restore_anchor(
        &self,
        anchor: &ScrollAnchor<Key>,
        viewport: f64,
    ) -> Result<f64, VirtualError> {
        Self::viewport(anchor.within, viewport)?;
        let index = self.index(&anchor.key).ok_or(VirtualError::UnknownKey)?;
        let start = self.position(index).unwrap();
        // Rows are half-open: clamping to the end would anchor the next key
        // when this row shrinks past the saved within-row offset.
        let end = self.position(index + 1).unwrap();
        Ok((start + anchor.within)
            .min(end.next_down().max(start))
            .min((self.total - viewport).max(0.0)))
    }

    /// Query the actual viewport rather than the mounted (overscanned) window.
    /// Empty lists and zero-sized viewports have no scroll position.
    pub fn scroll_position(
        &self,
        offset: f64,
        viewport: f64,
    ) -> Result<Option<ListScrollPosition>, VirtualError> {
        Self::viewport(offset, viewport)?;
        if self.is_empty() || viewport == 0.0 {
            return Ok(None);
        }
        let offset = offset.min((self.total - viewport).max(0.0));
        let index = self.at(offset).min(self.len() - 1);
        Ok(Some(ListScrollPosition {
            index,
            offset: offset - self.position(index).unwrap(),
        }))
    }

    /// Measure one row and return the adjusted offset for the same keyed row
    /// at the viewport's start. Both growth and shrinkage above it are handled.
    /// `follow_end` explicitly keeps an end-anchored viewport at the new end;
    /// callers turn it off when the user scrolls away. Invalid inputs leave
    /// the index unchanged. The adapter must apply the returned offset.
    pub fn measure_preserving_scroll(
        &mut self,
        key: &Key,
        extent: f64,
        offset: f64,
        viewport: f64,
        follow_end: bool,
    ) -> Result<f64, VirtualError> {
        Self::viewport(offset, viewport)?;
        let offset = offset.min((self.total - viewport).max(0.0));
        let at_end = follow_end && offset == (self.total - viewport).max(0.0);
        let anchor = self.anchor(offset)?;
        self.measure(key, extent)?;
        if at_end {
            return Ok((self.total - viewport).max(0.0));
        }
        match anchor {
            Some(anchor) => self.restore_anchor(&anchor, viewport),
            None => Ok(0.0),
        }
    }

    /// Reveal a focused row with minimal movement. A partially visible row
    /// larger than the viewport retains the offset; a hidden large row aligns
    /// its start. Empty viewports do not trigger a focus scroll.
    pub fn reveal(&self, key: &Key, offset: f64, viewport: f64) -> Result<f64, VirtualError> {
        Self::viewport(offset, viewport)?;
        let index = self.index(key).ok_or(VirtualError::UnknownKey)?;
        let start = self.position(index).unwrap();
        let end = start + self.extents[index];
        let offset = offset.min((self.total - viewport).max(0.0));
        let delta =
            crate::presentation::input::focus_reveal_delta(start - offset, end - offset, viewport);
        Ok((offset + delta).clamp(0.0, (self.total - viewport).max(0.0)))
    }

    pub fn window(
        &self,
        offset: f64,
        viewport: f64,
        overscan: f64,
        pinned: &[Key],
        capacity: usize,
    ) -> Result<VirtualWindow, VirtualError> {
        Self::viewport(offset, viewport)?;
        if !overscan.is_finite() || overscan < 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        let offset = offset.min((self.total - viewport).max(0.0));
        let start = self.at((offset - overscan).max(0.0));
        let end_pixel = (offset + viewport + overscan).min(self.total);
        let end_index = self.at(end_pixel);
        let end = if self.position(end_index) == Some(end_pixel) {
            end_index
        } else {
            (end_index + 1).min(self.len())
        };
        let visible = if viewport == 0.0 {
            start..start
        } else {
            start..end
        };
        if visible.len() > capacity || pinned.len() > capacity {
            return Err(VirtualError::Capacity);
        }
        let mut mounted: BTreeSet<_> = visible.clone().collect();
        for key in pinned {
            mounted.insert(self.index(key).ok_or(VirtualError::UnknownKey)?);
            if mounted.len() > capacity {
                return Err(VirtualError::Capacity);
            }
        }
        Ok(VirtualWindow {
            visible,
            mounted: mounted.into_iter().collect(),
            offset,
            total: self.total,
        })
    }

    /// Build a window that keeps the focused row and its adjacent rows mounted.
    /// This lets sequential keyboard navigation reach rows beyond the viewport;
    /// `reveal` can then scroll each newly focused row into view.
    pub fn window_with_focus(
        &self,
        offset: f64,
        viewport: f64,
        overscan: f64,
        pinned: &[Key],
        focused: Option<&Key>,
        capacity: usize,
    ) -> Result<VirtualWindow, VirtualError> {
        let Some(focused) = focused else {
            return self.window(offset, viewport, overscan, pinned, capacity);
        };
        let index = self.index(focused).ok_or(VirtualError::UnknownKey)?;
        let start = index.saturating_sub(1);
        let end = (index + 2).min(self.len());
        let mut mounted_keys = Vec::with_capacity(pinned.len() + end - start);
        mounted_keys.extend_from_slice(pinned);
        mounted_keys.extend(self.keys[start..end].iter().cloned());
        self.window(offset, viewport, overscan, &mounted_keys, capacity)
    }
}

pub struct VirtualGrid<Row, Column> {
    pub rows: VirtualAxis<Row>,
    pub columns: VirtualAxis<Column>,
}

impl<Row: Clone + Ord, Column: Clone + Ord> VirtualGrid<Row, Column> {
    pub fn window(
        &self,
        offset: [f64; 2],
        viewport: [f64; 2],
        overscan: [f64; 2],
        pinned_rows: &[Row],
        pinned_columns: &[Column],
        capacity: usize,
    ) -> Result<[VirtualWindow; 2], VirtualError> {
        let rows = self
            .rows
            .window(offset[1], viewport[1], overscan[1], pinned_rows, capacity)?;
        let columns = self.columns.window(
            offset[0],
            viewport[0],
            overscan[0],
            pinned_columns,
            capacity,
        )?;
        if rows
            .mounted
            .len()
            .checked_mul(columns.mounted.len())
            .is_none_or(|count| count > capacity)
        {
            return Err(VirtualError::Capacity);
        }
        Ok([columns, rows])
    }
}
