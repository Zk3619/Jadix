use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    time::Duration,
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

impl ListScrollPosition {
    /// Resolve a saved position against a new key index. A removed anchor falls
    /// forward to the next surviving old row, then backward to the preceding
    /// survivor, at its start. No shared keys yields None. The lookup must
    /// describe a unique new key set; unchanged anchors retain their depth.
    pub fn remap_keys<Key>(
        self,
        previous: &[Key],
        mut index_of: impl FnMut(&Key) -> Option<usize>,
    ) -> Result<Option<Self>, VirtualError> {
        VirtualAxis::<usize>::viewport(self.offset, 0.0)?;
        let Some(key) = previous.get(self.index) else {
            return Ok(None);
        };
        if let Some(index) = index_of(key) {
            return Ok(Some(Self { index, ..self }));
        }
        Ok(previous[self.index + 1..]
            .iter()
            .chain(previous[..self.index].iter().rev())
            .find_map(|key| index_of(key).map(|index| Self { index, offset: 0.0 })))
    }
}

fn restore_list_position(start: f64, end: f64, total: f64, depth: f64, viewport: f64) -> f64 {
    // Preserve the half-open row even when its extent shrank since capture.
    (start + depth)
        .min(end.next_down().max(start))
        .min((total - viewport).max(0.0))
}

/// Map dataset indices to visual row order on either axis. The mapping is its
/// own inverse and requires no reversed collection or per-row storage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListOrder {
    #[default]
    Forward,
    Reversed,
}

impl ListOrder {
    /// Convert one row index in either direction. The end sentinel is not a row.
    pub fn index(self, index: usize, count: usize) -> Option<usize> {
        if index >= count {
            return None;
        }
        Some(match self {
            Self::Forward => index,
            Self::Reversed => count - 1 - index,
        })
    }

    /// Convert a half-open row range in either direction. Invalid bounds return
    /// None; empty ranges remain empty at their corresponding boundary.
    pub fn range(self, range: Range<usize>, count: usize) -> Option<Range<usize>> {
        if range.start > range.end || range.end > count {
            return None;
        }
        Some(match self {
            Self::Forward => range,
            Self::Reversed => count - range.end..count - range.start,
        })
    }
}

/// Logical dataset edges near the actual viewport, independent of mounted
/// overscan/prediction and the rendering axis. An empty dataset can request its
/// first page at the end, but never requests preceding items at the start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListLoadEdges {
    pub start: bool,
    pub end: bool,
}

impl ListLoadEdges {
    /// Thresholds are [logical start, logical end], measured in viewport extents.
    /// Positive overscroll clamps to the content edge; a zero viewport is inert.
    /// Rejects negative/nonfinite geometry or thresholds without allocating.
    pub fn new(
        offset: f64,
        viewport: f64,
        total: f64,
        thresholds: [f64; 2],
        nonempty: bool,
        order: ListOrder,
    ) -> Result<Self, VirtualError> {
        for value in [offset, viewport, total, thresholds[0], thresholds[1]] {
            if !value.is_finite() || value < 0.0 {
                return Err(VirtualError::InvalidExtent);
            }
        }
        if viewport == 0.0 {
            return Ok(Self::default());
        }
        let maximum = (total - viewport).max(0.0);
        let offset = offset.min(maximum);
        let distances = match order {
            ListOrder::Forward => [offset, maximum - offset],
            ListOrder::Reversed => [maximum - offset, offset],
        };
        Ok(Self {
            start: nonempty && distances[0] <= viewport * thresholds[0],
            end: distances[1] <= viewport * thresholds[1],
        })
    }

    /// Request each edge once per approach. Data/count changes rearm an edge
    /// while still nearby; callers retain loading/exhaustion/error retry policy.
    pub fn entered(self, previous: Self, data_changed: bool) -> Self {
        Self {
            start: self.start && (!previous.start || data_changed),
            end: self.end && (!previous.end || data_changed),
        }
    }
}

/// Align content shorter than its viewport on either axis. Scrolling and
/// following the end are separate policies. Longer content receives no padding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListContentAlign {
    #[default]
    Start,
    Center,
    End,
}

impl ListContentAlign {
    /// Leading and trailing logical padding. Start preserves the natural extent;
    /// Center and End fill the viewport, including when the content is empty.
    pub fn padding(self, content: f64, viewport: f64) -> Result<[f64; 2], VirtualError> {
        VirtualAxis::<usize>::viewport(content, viewport)?;
        let free = (viewport - content).max(0.0);
        Ok(match self {
            Self::Start => [0.0, 0.0],
            Self::Center => [free * 0.5, free - free * 0.5],
            Self::End => [free, 0.0],
        })
    }
}

/// Main-axis content surrounding virtual rows, independent of the renderer and
/// row index. Insets include scrolling headers/footers; short-content alignment
/// applies to the complete content. All queries use physical content coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListContent {
    rows: f64,
    insets: [f64; 2],
    total: f64,
}

impl ListContent {
    pub fn new(
        rows: f64,
        insets: [f64; 2],
        viewport: f64,
        align: ListContentAlign,
    ) -> Result<Self, VirtualError> {
        VirtualAxis::<usize>::viewport(rows, viewport)?;
        VirtualAxis::<usize>::viewport(insets[0], insets[1])?;
        let total = rows + insets[0] + insets[1];
        let padding = align.padding(total, viewport)?;
        Ok(Self {
            rows,
            insets: [insets[0] + padding[0], insets[1] + padding[1]],
            total: total + padding[0] + padding[1],
        })
    }

    pub fn insets(self) -> [f64; 2] {
        self.insets
    }

    pub fn total(self) -> f64 {
        self.total
    }

    pub fn max_offset(self, viewport: f64) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(viewport, 0.0)?;
        Ok((self.total - viewport).max(0.0))
    }

    /// Project the intersecting row region, then restore the full viewport's
    /// coordinates and size. A header/footer-only viewport has no visible rows.
    /// The supplied query must describe the same row extent as this content.
    pub fn viewport(
        self,
        offset: f64,
        viewport: f64,
        query: impl FnOnce(f64, f64) -> Result<ListViewport, VirtualError>,
    ) -> Result<ListViewport, VirtualError> {
        VirtualAxis::<usize>::viewport(offset, viewport)?;
        let offset = offset.min(self.max_offset(viewport)?);
        let start = (offset - self.insets[0]).clamp(0.0, self.rows);
        let end = (offset + viewport - self.insets[0]).clamp(0.0, self.rows);
        let mut state = query(start, end - start)?;
        state.offset = offset;
        state.viewport_extent = viewport;
        state.total_extent = self.total;
        Ok(state)
    }

    /// Borrow a parent viewport while retaining this content's slots and full
    /// physical extent. The row query uses row-relative coordinates.
    pub fn viewport_in_parent(
        self,
        parent: ParentViewport,
        query: impl FnOnce(f64, f64) -> Result<ListViewport, VirtualError>,
    ) -> Result<ListViewport, VirtualError> {
        parent.viewport(self.total, |offset, extent| {
            self.viewport(offset, extent, query)
        })
    }

    /// Capture only offsets inside the row region. Scrolling in a header or
    /// footer must not turn into a row anchor after data or measurement changes.
    pub fn anchor_offset(self, offset: f64) -> Result<Option<f64>, VirtualError> {
        VirtualAxis::<usize>::viewport(offset, 0.0)?;
        let relative = offset - self.insets[0];
        Ok((relative >= 0.0 && relative < self.rows).then_some(relative))
    }

    pub fn scroll_to(
        self,
        start: f64,
        extent: f64,
        current: f64,
        viewport: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(start, extent)?;
        VirtualAxis::<usize>::viewport(current, viewport)?;
        if start + extent > self.rows {
            return Err(VirtualError::InvalidExtent);
        }
        align.offset(
            start + self.insets[0],
            extent,
            self.total,
            current,
            viewport,
            extra,
        )
    }

    /// Restore row depth, preserving the half-open boundary and clamping only
    /// against the complete content end, including any scrolling footer.
    pub fn restore(
        self,
        start: f64,
        end: f64,
        depth: f64,
        viewport: f64,
    ) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(start, end)?;
        VirtualAxis::<usize>::viewport(depth, viewport)?;
        if end <= start || end > self.rows {
            return Err(VirtualError::InvalidExtent);
        }
        Ok(restore_list_position(
            start + self.insets[0],
            end + self.insets[0],
            self.total,
            depth,
            viewport,
        ))
    }

    /// Bounds are row-relative `(start, end)` coordinates. Viewport-fraction
    /// thresholds retain the full viewport denominator, including headers.
    pub fn viewable_items(
        self,
        state: &ListViewport,
        policy: ListViewability,
        bounds: impl Fn(usize) -> (f64, f64),
    ) -> Result<Range<usize>, VirtualError> {
        policy.validate()?;
        Ok(policy.filter(state, |index| {
            let (start, end) = bounds(index);
            (start + self.insets[0], end + self.insets[0])
        }))
    }

    /// Bounds are row-relative `(start, extent)`. The last pin stops at the row
    /// region's end so that it cannot cover a scrolling footer.
    pub fn sticky_header(
        self,
        state: &ListViewport,
        headers: &ListStickyHeaders,
        inset: f64,
        bounds: impl Fn(usize) -> (f64, f64),
    ) -> Result<Option<ListStickyHeader>, VirtualError> {
        if !inset.is_finite() {
            return Err(VirtualError::InvalidExtent);
        }
        if state.offset >= self.total || state.offset + state.viewport_extent <= 0.0 {
            return Ok(None);
        }
        let mut rows = state.clone();
        rows.total_extent = self.insets[0] + self.rows;
        headers.layout(&rows, inset, |index| {
            let (start, extent) = bounds(index);
            (start + self.insets[0], extent)
        })
    }
}

/// Position a row within the viewport. Fraction(0/0.5/1) matches Start/Center/End.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ListScrollAlign {
    #[default]
    Nearest,
    Start,
    Center,
    End,
    Fraction(f64),
}

impl ListScrollAlign {
    pub fn validate(self) -> Result<(), VirtualError> {
        match self {
            Self::Fraction(f) if !f.is_finite() || !(0.0..=1.0).contains(&f) => {
                Err(VirtualError::InvalidExtent)
            }
            _ => Ok(()),
        }
    }

    fn offset(
        self,
        start: f64,
        extent: f64,
        total: f64,
        current: f64,
        viewport: f64,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        self.validate()?;
        if !extra.is_finite() {
            return Err(VirtualError::InvalidExtent);
        }
        let max = (total - viewport).max(0.0);
        let current = current.min(max);
        if viewport == 0.0 {
            return Ok(current);
        }
        let fraction = match self {
            Self::Nearest => {
                let delta = crate::presentation::input::focus_reveal_delta(
                    start - current,
                    start + extent - current,
                    viewport,
                );
                return Ok((current + delta - extra).clamp(0.0, max));
            }
            Self::Start => 0.0,
            Self::Center => 0.5,
            Self::End => 1.0,
            Self::Fraction(f) => f,
        };
        Ok((start - (viewport - extent) * fraction - extra).clamp(0.0, max))
    }
}

/// Actual viewport geometry, excluding overscan and pinned/focused rows.
/// `visible` is half-open and includes partially visible rows. Queries allocate
/// no row collection; empty data or a zero viewport produces an empty range.
/// Parent queries retain the signed local offset and the full ancestor viewport,
/// even when the region is partially or completely outside that viewport.
#[derive(Clone, Debug, PartialEq)]
pub struct ListViewport {
    pub visible: Range<usize>,
    pub offset: f64,
    pub viewport_extent: f64,
    pub total_extent: f64,
    pub item_count: usize,
}

/// A content region borrowing an ancestor's viewport on one physical axis.
/// The adapter supplies its natural start in the parent's scrolling content;
/// this model neither discovers ancestors nor owns their scroll state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParentViewport {
    parent_offset: f64,
    extent: f64,
    content_start: f64,
}

impl ParentViewport {
    pub fn new(parent_offset: f64, extent: f64, content_start: f64) -> Result<Self, VirtualError> {
        if [parent_offset, extent, content_start]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(VirtualError::InvalidExtent);
        }
        Ok(Self {
            parent_offset,
            extent,
            content_start,
        })
    }

    /// Signed local offset: negative before the region reaches the viewport.
    pub fn offset(self) -> f64 {
        self.parent_offset - self.content_start
    }

    pub fn extent(self) -> f64 {
        self.extent
    }

    /// Preserve content below a replaced local interval when its old end was
    /// entirely above the viewport. An insertion at the viewport top counts;
    /// changes at/before the region's unscrolled start do not hide new content.
    /// `parent_total` is the parent's content extent AFTER the change. Always
    /// clamp to that parent's range, including when no compensation applies.
    /// Feed the returned snapshot into subsequent changes in the same layout
    /// and apply its parent offset to the actual scroll owner once finished.
    pub fn preserve_change(
        self,
        before: Range<f64>,
        after_extent: f64,
        parent_total: f64,
    ) -> Result<Self, VirtualError> {
        VirtualAxis::<usize>::viewport(before.start, before.end)?;
        VirtualAxis::<usize>::viewport(after_extent, parent_total)?;
        let after_end = self.content_start + before.start + after_extent;
        if before.end < before.start || !after_end.is_finite() || after_end > parent_total {
            return Err(VirtualError::InvalidExtent);
        }
        let delta = after_extent - (before.end - before.start);
        let offset = if self.extent > 0.0 && self.offset() > 0.0 && before.end <= self.offset() {
            self.parent_offset + delta
        } else {
            self.parent_offset
        };
        Ok(Self {
            parent_offset: offset.clamp(0.0, (parent_total - self.extent).max(0.0)),
            ..self
        })
    }

    /// Offset to apply to the ancestor scroll owner after navigation/anchoring.
    pub fn parent_offset(self) -> f64 {
        self.parent_offset
    }

    /// Intersect before querying an owned geometry model, then restore the full
    /// parent viewport for overlap thresholds and sticky-header placement.
    /// Wholly offscreen regions stay empty instead of clamping to an end row.
    /// The query must describe exactly `total` units of local content.
    pub fn viewport(
        self,
        total: f64,
        query: impl FnOnce(f64, f64) -> Result<ListViewport, VirtualError>,
    ) -> Result<ListViewport, VirtualError> {
        let (start, extent) = self.intersection(total)?;
        let mut state = query(start, extent)?;
        state.offset = self.offset();
        state.viewport_extent = self.extent;
        state.total_extent = total;
        Ok(state)
    }

    fn intersection(self, total: f64) -> Result<(f64, f64), VirtualError> {
        VirtualAxis::<usize>::viewport(total, 0.0)?;
        let offset = self.offset();
        let start = offset.clamp(0.0, total);
        // A finite extent may overflow at the far end; clipping infinity to a
        // finite content end is intentional and never reaches the query.
        let end = (offset + self.extent).clamp(0.0, total);
        Ok((start, end - start))
    }

    /// Parent-space navigation. Row start/extent are local physical coordinates
    /// including any authored slots. Clamp to the parent's complete scroll range,
    /// so aligning a short embedded list does not clamp to that list's end.
    pub fn scroll_to(
        self,
        start: f64,
        extent: f64,
        parent_total: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(start, extent)?;
        VirtualAxis::<usize>::viewport(parent_total, 0.0)?;
        let start = self.content_start + start;
        if !start.is_finite() || extent > parent_total || start > parent_total - extent {
            return Err(VirtualError::InvalidExtent);
        }
        align.offset(
            start,
            extent,
            parent_total,
            self.parent_offset,
            self.extent,
            extra,
        )
    }

    /// Approach each local edge in parent space. Unlike an owned scroll view,
    /// passing completely beyond the corresponding edge disarms it. Prefetch
    /// distances and overlap policies use the full parent viewport denominator.
    pub fn load_edges(
        self,
        total: f64,
        thresholds: [f64; 2],
        nonempty: bool,
        order: ListOrder,
    ) -> Result<ListLoadEdges, VirtualError> {
        self.intersection(total)?;
        if thresholds.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(VirtualError::InvalidExtent);
        }
        if self.extent == 0.0 {
            return Ok(ListLoadEdges::default());
        }
        let offset = self.offset();
        let bottom = offset + self.extent;
        let (start_threshold, end_threshold) = if order == ListOrder::Forward {
            (thresholds[0], thresholds[1])
        } else {
            (thresholds[1], thresholds[0])
        };
        let start = bottom >= 0.0 && offset <= start_threshold * self.extent;
        let end = offset <= total && total - bottom <= end_threshold * self.extent;
        let (start, end) = if order == ListOrder::Forward {
            (start, end)
        } else {
            (end, start)
        };
        Ok(ListLoadEdges {
            start: nonempty && start,
            end,
        })
    }
}

/// One pinned row. Translation is a nonnegative displacement from its natural
/// content position; it does not change row measurement or total content size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListStickyHeader {
    pub index: usize,
    pub translation: f64,
}

/// Ordered section-header indices. Construction sorts and deduplicates once;
/// queries binary-search headers, never scan intervening data rows. Indices past
/// the current dataset are ignored, so truncation needs no header-table rebuild.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListStickyHeaders {
    indices: Vec<usize>,
}

impl ListStickyHeaders {
    pub fn new(indices: impl IntoIterator<Item = usize>) -> Self {
        let mut indices: Vec<_> = indices.into_iter().collect();
        indices.sort_unstable();
        indices.dedup();
        Self { indices }
    }

    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    fn layout(
        &self,
        state: &ListViewport,
        inset: f64,
        bounds: impl Fn(usize) -> (f64, f64),
    ) -> Result<Option<ListStickyHeader>, VirtualError> {
        if !inset.is_finite() {
            return Err(VirtualError::InvalidExtent);
        }
        if state.item_count == 0 || state.viewport_extent == 0.0 {
            return Ok(None);
        }
        let at = state.offset + inset;
        let end = self
            .indices
            .partition_point(|&i| i < state.item_count && bounds(i).0 <= at);
        let Some(&index) = end.checked_sub(1).and_then(|i| self.indices.get(i)) else {
            return Ok(None);
        };
        let (start, extent) = bounds(index);
        let next = self
            .indices
            .get(end)
            .filter(|&&i| i < state.item_count)
            .map_or(state.total_extent, |&i| bounds(i).0);
        let position = at.min(next - extent).max(start);
        Ok(Some(ListStickyHeader {
            index,
            translation: position - start,
        }))
    }
}

/// Proportion required for a partially visible row. Fully visible rows always
/// qualify. Fractions must be finite and in 0..=1; zero still requires overlap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ListViewability {
    #[default]
    AnyVisible,
    ItemFraction(f64),
    ViewportFraction(f64),
}

impl ListViewability {
    pub fn validate(self) -> Result<(), VirtualError> {
        match self {
            Self::AnyVisible => Ok(()),
            Self::ItemFraction(f) | Self::ViewportFraction(f)
                if f.is_finite() && (0.0..=1.0).contains(&f) =>
            {
                Ok(())
            }
            _ => Err(VirtualError::InvalidExtent),
        }
    }

    fn filter(self, viewport: &ListViewport, bounds: impl Fn(usize) -> (f64, f64)) -> Range<usize> {
        let mut range = viewport.visible.clone();
        let top = viewport.offset;
        let bottom = top + viewport.viewport_extent;
        let accepts = |index| {
            let (start, end) = bounds(index);
            let overlap = (end.min(bottom) - start.max(top)).max(0.0);
            overlap > 0.0
                && ((start >= top && end <= bottom)
                    || match self {
                        Self::AnyVisible => true,
                        Self::ItemFraction(f) => overlap / (end - start) >= f,
                        Self::ViewportFraction(f) => overlap / viewport.viewport_extent >= f,
                    })
        };
        // Only the two boundary rows can be partial. No scan of interior rows.
        if !range.is_empty() && !accepts(range.start) {
            range.start += 1;
        }
        if !range.is_empty() && !accepts(range.end - 1) {
            range.end -= 1;
        }
        range
    }
}

/// Sorted row indices, independent of mounted overscan/pins. Dataset revisions
/// are the caller's responsibility; these are indices, not persistent keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewableItems {
    pub viewable: Vec<usize>,
    pub previous: Vec<usize>,
}

impl ViewableItems {
    /// Newly viewable rows followed by rows that left, without allocating deltas.
    pub fn changed(&self) -> impl Iterator<Item = (usize, bool)> + '_ {
        self.viewable
            .iter()
            .copied()
            .filter(|index| self.previous.binary_search(index).is_err())
            .map(|index| (index, true))
            .chain(
                self.previous
                    .iter()
                    .copied()
                    .filter(|index| self.viewable.binary_search(index).is_err())
                    .map(|index| (index, false)),
            )
    }
}

/// Per-row continuous dwell tracking with a caller-supplied monotonic clock.
/// Storage is bounded by the candidate viewport, never the full dataset. Stable
/// polls allocate nothing; only entering rows and delivered changes allocate.
#[derive(Default)]
pub struct ListViewabilityTracker {
    minimum: Duration,
    entered: BTreeMap<usize, Duration>,
    reported: Vec<usize>,
    next_deadline: Option<Duration>,
}

impl ListViewabilityTracker {
    pub fn new(minimum: Duration) -> Self {
        Self {
            minimum,
            ..Self::default()
        }
    }

    /// Restart dwell after a dataset revision or visibility suspension. The next
    /// update reports withdrawals; retained index values do not inherit old dwell.
    pub fn restart(&mut self) {
        self.entered.clear();
        self.next_deadline = None;
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.next_deadline
    }

    pub fn update(&mut self, candidates: Range<usize>, now: Duration) -> Option<ViewableItems> {
        self.next_deadline = None;
        if self.minimum.is_zero() {
            if candidates.clone().eq(self.reported.iter().copied()) {
                return None;
            }
            let viewable: Vec<_> = candidates.collect();
            return Some(ViewableItems {
                previous: std::mem::replace(&mut self.reported, viewable.clone()),
                viewable,
            });
        }
        self.entered.retain(|index, _| candidates.contains(index));
        for index in candidates {
            let since = *self.entered.entry(index).or_insert(now);
            if now.saturating_sub(since) < self.minimum {
                if let Some(deadline) = since.checked_add(self.minimum) {
                    self.next_deadline =
                        Some(self.next_deadline.map_or(deadline, |d| d.min(deadline)));
                }
            }
        }
        let qualified = || {
            self.entered.iter().filter_map(|(&index, &since)| {
                (now.saturating_sub(since) >= self.minimum).then_some(index)
            })
        };
        if qualified().eq(self.reported.iter().copied()) {
            return None;
        }
        let viewable: Vec<_> = qualified().collect();
        Some(ViewableItems {
            previous: std::mem::replace(&mut self.reported, viewable.clone()),
            viewable,
        })
    }
}

/// Constant-space directional render prediction, independent of row geometry.
/// Feed synchronized physical offsets, then expand the mount window by the
/// returned signed pixel distance. Actual visibility must use the original viewport.
/// At most one viewport is added, in the direction of travel. This is a distance
/// heuristic, not a velocity estimate: no clock, allocation or frame scheduling.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListScrollAhead {
    previous: Option<(f64, f64)>,
    lead: f64,
}

impl ListScrollAhead {
    /// Rebase after dataset changes, anchoring or explicit navigation.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Predict twice the last nonzero step, capped at one viewport. A smaller
    /// same-direction step retains 75% of the prior prediction. Stationary
    /// samples retain the window, avoiding repeated mount/unmount work at rest.
    /// Direction reversals switch immediately; jumps over one viewport, viewport
    /// resizing and hidden viewports discard history. Invalid input is atomic.
    pub fn update(&mut self, offset: f64, viewport: f64) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(offset, viewport)?;
        let previous = self.previous.replace((offset, viewport));
        let Some((old_offset, old_viewport)) = previous else {
            return Ok(0.0);
        };
        let step = offset - old_offset;
        if viewport == 0.0 || viewport != old_viewport || step.abs() > viewport {
            self.lead = 0.0;
        } else if step != 0.0 {
            let predicted = step.abs().min(viewport * 0.5) * 2.0;
            let retained = if step.signum() == self.lead.signum() {
                self.lead.abs() * 0.75
            } else {
                0.0
            };
            self.lead = step.signum() * predicted.max(retained).min(viewport);
        }
        Ok(self.lead)
    }
}

/// Uniform row geometry with explicit space between rows, never after the last.
/// Queries use constant space and O(1) arithmetic even for very large datasets.
#[derive(Clone, Copy, Debug)]
pub struct UniformList {
    count: usize,
    extent: f64,
    gap: f64,
    total: f64,
}

impl UniformList {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn new(count: usize, extent: f64, gap: f64) -> Result<Self, VirtualError> {
        let total = count as f64 * extent + count.saturating_sub(1) as f64 * gap;
        if !extent.is_finite()
            || extent <= 0.0
            || !gap.is_finite()
            || gap < 0.0
            || !(extent + gap).is_finite()
            || !total.is_finite()
        {
            return Err(VirtualError::InvalidExtent);
        }
        Ok(Self {
            count,
            extent,
            gap,
            total,
        })
    }

    pub fn total(&self) -> f64 {
        self.total
    }

    /// A row start, or the content end for index == count.
    pub fn position(&self, index: usize) -> Option<f64> {
        (index <= self.count).then(|| (index as f64 * (self.extent + self.gap)).min(self.total))
    }

    /// Capture the row containing an offset, including its trailing gap. Unlike
    /// scroll_position this is independent of the viewport's intersecting rows.
    pub fn anchor(&self, offset: f64) -> Result<Option<ListScrollPosition>, VirtualError> {
        VirtualAxis::<usize>::viewport(offset, 0.0)?;
        Ok((self.count > 0).then(|| {
            let index = ((offset / (self.extent + self.gap)).floor() as usize).min(self.count - 1);
            ListScrollPosition {
                index,
                offset: (offset - self.position(index).unwrap())
                    .min(self.position(index + 1).unwrap() - self.position(index).unwrap()),
            }
        }))
    }

    pub fn restore_position(
        &self,
        position: ListScrollPosition,
        viewport: f64,
    ) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(position.offset, viewport)?;
        if position.index >= self.count {
            return Err(VirtualError::UnknownKey);
        }
        Ok(restore_list_position(
            self.position(position.index).unwrap(),
            self.position(position.index + 1).unwrap(),
            self.total,
            position.offset,
            viewport,
        ))
    }

    pub fn viewport(&self, offset: f64, viewport: f64) -> Result<ListViewport, VirtualError> {
        VirtualAxis::<usize>::viewport(offset, viewport)?;
        let offset = offset.min((self.total - viewport).max(0.0));
        let stride = self.extent + self.gap;
        let mut start = ((offset / stride).floor() as usize).min(self.count);
        if start < self.count && offset >= self.position(start).unwrap() + self.extent {
            start += 1;
        }
        let end = if viewport == 0.0 {
            start
        } else {
            (((offset + viewport).min(self.total) / stride).ceil() as usize)
                .min(self.count)
                .max(start)
        };
        Ok(ListViewport {
            visible: start..end,
            offset,
            viewport_extent: viewport,
            total_extent: self.total,
            item_count: self.count,
        })
    }

    /// Query the intersection with an ancestor viewport without end clamping.
    pub fn viewport_in_parent(&self, parent: ParentViewport) -> Result<ListViewport, VirtualError> {
        parent.viewport(self.total, |offset, extent| self.viewport(offset, extent))
    }

    pub fn scroll_to_index(
        &self,
        index: usize,
        offset: f64,
        viewport: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        let state = self.viewport(offset, viewport)?;
        if index >= self.count {
            return Err(VirtualError::UnknownKey);
        }
        align.offset(
            self.position(index).unwrap(),
            self.extent,
            self.total,
            state.offset,
            viewport,
            extra,
        )
    }

    pub fn viewable_items(
        &self,
        offset: f64,
        viewport: f64,
        policy: ListViewability,
    ) -> Result<Range<usize>, VirtualError> {
        policy.validate()?;
        let state = self.viewport(offset, viewport)?;
        Ok(policy.filter(&state, |i| {
            let start = self.position(i).unwrap();
            (start, start + self.extent)
        }))
    }

    /// Pin the latest section header at a signed viewport inset. The next
    /// header pushes it away; the final header stops at the content end.
    /// O(log h) queries, with no allocation (h = header count).
    pub fn sticky_header(
        &self,
        headers: &ListStickyHeaders,
        offset: f64,
        viewport: f64,
        inset: f64,
    ) -> Result<Option<ListStickyHeader>, VirtualError> {
        headers.layout(&self.viewport(offset, viewport)?, inset, |i| {
            (self.position(i).unwrap(), self.extent)
        })
    }

    /// First intersecting row, with zero depth when the viewport starts in a gap.
    /// A viewport containing only a gap has no scroll position.
    pub fn scroll_position(
        &self,
        offset: f64,
        viewport: f64,
    ) -> Result<Option<ListScrollPosition>, VirtualError> {
        let state = self.viewport(offset, viewport)?;
        Ok((!state.visible.is_empty()).then(|| ListScrollPosition {
            index: state.visible.start,
            offset: (state.offset - self.position(state.visible.start).unwrap()).max(0.0),
        }))
    }
}

/// Two-dimensional viewport. Coordinates and size arrays use `[x, y]` order.
#[derive(Clone, Debug, PartialEq)]
pub struct GridViewport {
    pub columns: ListViewport,
    pub rows: ListViewport,
}

/// A bounded rectangular mount window, distinct from the actual visible cells.
#[derive(Clone, Debug, PartialEq)]
pub struct UniformGridWindow {
    pub viewport: GridViewport,
    pub columns: Range<usize>,
    pub rows: Range<usize>,
}

/// Fixed-size two-dimensional virtualization with O(1) geometry and storage.
/// Composes the same gap-aware axes as UniformList; no per-cell index is built.
#[derive(Clone, Copy, Debug)]
pub struct UniformGrid {
    columns: UniformList,
    rows: UniformList,
}

fn extend_scroll_ahead(
    axis: &UniformList,
    viewport: &ListViewport,
    mounted: &mut Range<usize>,
    ahead: f64,
) -> Result<(), VirtualError> {
    if !ahead.is_finite() {
        return Err(VirtualError::InvalidExtent);
    }
    if ahead == 0.0 || viewport.viewport_extent == 0.0 {
        return Ok(());
    }
    let ahead = ahead.clamp(-viewport.viewport_extent, viewport.viewport_extent);
    let start = (viewport.offset + ahead.min(0.0)).max(0.0);
    // Clip before adding, so very large finite viewports never overflow.
    let end = viewport.offset
        + viewport
            .viewport_extent
            .min((axis.total() - viewport.offset).max(0.0));
    let end = end + ahead.max(0.0).min((axis.total() - end).max(0.0));
    let extra = axis.viewport(start, end - start)?.visible;
    mounted.start = mounted.start.min(extra.start);
    mounted.end = mounted.end.max(extra.end);
    Ok(())
}

impl UniformGrid {
    /// Counts are `(rows, columns)`; sizes and gaps are `[width, height]`.
    /// Either zero count makes the whole grid empty, including both extents.
    pub fn new(
        rows: usize,
        columns: usize,
        cell_size: [f64; 2],
        gap: [f64; 2],
    ) -> Result<Self, VirtualError> {
        let empty = rows == 0 || columns == 0;
        Ok(Self {
            columns: UniformList::new(if empty { 0 } else { columns }, cell_size[0], gap[0])?,
            rows: UniformList::new(if empty { 0 } else { rows }, cell_size[1], gap[1])?,
        })
    }

    pub fn total(&self) -> [f64; 2] {
        [self.columns.total(), self.rows.total()]
    }

    /// Gap-aware column and row geometry, in [x, y] order. Copies retain O(1)
    /// storage and share UniformList's anchoring and navigation semantics.
    pub fn axes(&self) -> [UniformList; 2] {
        [self.columns, self.rows]
    }

    pub fn cell_origin(&self, row: usize, column: usize) -> Option<[f64; 2]> {
        if row >= self.rows.count || column >= self.columns.count {
            return None;
        }
        Some([self.columns.position(column)?, self.rows.position(row)?])
    }

    /// Overscan is a number of columns/rows on each edge. The cell product is
    /// checked before allocation, including usize overflow. Zero viewports mount
    /// nothing; an offset inside a gap retains the axes' half-open visibility.
    pub fn window(
        &self,
        offset: [f64; 2],
        viewport: [f64; 2],
        overscan: [usize; 2],
        capacity: usize,
    ) -> Result<UniformGridWindow, VirtualError> {
        let columns = self.columns.viewport(offset[0], viewport[0])?;
        let rows = self.rows.viewport(offset[1], viewport[1])?;
        let expand = |axis: &ListViewport, extra: usize| {
            axis.visible.start.saturating_sub(extra)
                ..axis.visible.end.saturating_add(extra).min(axis.item_count)
        };
        let (mounted_columns, mounted_rows) = if viewport.contains(&0.0) {
            (0..0, 0..0)
        } else {
            (expand(&columns, overscan[0]), expand(&rows, overscan[1]))
        };
        if mounted_columns
            .len()
            .checked_mul(mounted_rows.len())
            .is_none_or(|count| count > capacity)
        {
            return Err(VirtualError::Capacity);
        }
        Ok(UniformGridWindow {
            viewport: GridViewport { columns, rows },
            columns: mounted_columns,
            rows: mounted_rows,
        })
    }

    /// Parent coordinates are [horizontal, vertical]. Wholly offscreen grids
    /// mount nothing, including overscan, and capacity still bounds real cells.
    pub fn window_in_parent(
        &self,
        parent: [ParentViewport; 2],
        overscan: [usize; 2],
        capacity: usize,
    ) -> Result<UniformGridWindow, VirtualError> {
        let (x, width) = parent[0].intersection(self.columns.total())?;
        let (y, height) = parent[1].intersection(self.rows.total())?;
        let mut window = self.window([x, y], [width, height], overscan, capacity)?;
        window.viewport.columns.offset = parent[0].offset();
        window.viewport.columns.viewport_extent = parent[0].extent();
        window.viewport.rows.offset = parent[1].offset();
        window.viewport.rows.viewport_extent = parent[1].extent();
        Ok(window)
    }

    /// Add signed physical-pixel prediction on each axis, capped at one viewport
    /// per axis. Actual visibility and the base overscan window stay unchanged.
    /// If prediction exceeds capacity, return the base window instead; an
    /// over-budget base window still fails. Geometry and storage remain O(1).
    pub fn window_with_scroll_ahead(
        &self,
        offset: [f64; 2],
        viewport: [f64; 2],
        overscan: [usize; 2],
        capacity: usize,
        ahead: [f64; 2],
    ) -> Result<UniformGridWindow, VirtualError> {
        let base = self.window(offset, viewport, overscan, capacity)?;
        let mut predicted = base.clone();
        extend_scroll_ahead(
            &self.columns,
            &base.viewport.columns,
            &mut predicted.columns,
            ahead[0],
        )?;
        extend_scroll_ahead(
            &self.rows,
            &base.viewport.rows,
            &mut predicted.rows,
            ahead[1],
        )?;
        if viewport.contains(&0.0)
            || predicted
                .columns
                .len()
                .checked_mul(predicted.rows.len())
                .is_none_or(|count| count > capacity)
        {
            return Ok(base);
        }
        Ok(predicted)
    }

    /// Predict only inside the parent's intersection with this grid. Reports
    /// keep the full signed parent viewport; offscreen grids mount nothing.
    pub fn window_in_parent_with_scroll_ahead(
        &self,
        parent: [ParentViewport; 2],
        overscan: [usize; 2],
        capacity: usize,
        ahead: [f64; 2],
    ) -> Result<UniformGridWindow, VirtualError> {
        let (x, width) = parent[0].intersection(self.columns.total())?;
        let (y, height) = parent[1].intersection(self.rows.total())?;
        if width == 0.0 || height == 0.0 {
            if ahead.iter().any(|v| !v.is_finite()) {
                return Err(VirtualError::InvalidExtent);
            }
            return self.window_in_parent(parent, overscan, capacity);
        }
        let mut window =
            self.window_with_scroll_ahead([x, y], [width, height], overscan, capacity, ahead)?;
        window.viewport.columns.offset = parent[0].offset();
        window.viewport.columns.viewport_extent = parent[0].extent();
        window.viewport.rows.offset = parent[1].offset();
        window.viewport.rows.viewport_extent = parent[1].extent();
        Ok(window)
    }

    /// Return absolute parent offsets for both axes without clipping to this
    /// child's extent. Validate the cell before computing either target.
    pub fn scroll_to_cell_in_parent(
        &self,
        row: usize,
        column: usize,
        parent: [ParentViewport; 2],
        parent_total: [f64; 2],
        align: [ListScrollAlign; 2],
        extra: [f64; 2],
    ) -> Result<[f64; 2], VirtualError> {
        let origin = self
            .cell_origin(row, column)
            .ok_or(VirtualError::UnknownKey)?;
        Ok([
            parent[0].scroll_to(
                origin[0],
                self.columns.extent,
                parent_total[0],
                align[0],
                extra[0],
            )?,
            parent[1].scroll_to(
                origin[1],
                self.rows.extent,
                parent_total[1],
                align[1],
                extra[1],
            )?,
        ])
    }

    /// Align both axes atomically. Invalid cells never move either axis.
    pub fn scroll_to_cell(
        &self,
        row: usize,
        column: usize,
        offset: [f64; 2],
        viewport: [f64; 2],
        align: [ListScrollAlign; 2],
        extra: [f64; 2],
    ) -> Result<[f64; 2], VirtualError> {
        Ok([
            self.columns
                .scroll_to_index(column, offset[0], viewport[0], align[0], extra[0])?,
            self.rows
                .scroll_to_index(row, offset[1], viewport[1], align[1], extra[1])?,
        ])
    }
}

/// A flat dataset laid out in responsive, uniform rows. Geometry remains O(1);
/// the final row may be partial. Width is divided equally among fitting columns.
#[derive(Clone, Copy, Debug)]
pub struct UniformWrap {
    count: usize,
    columns: usize,
    size: [f64; 2],
    grid: UniformGrid,
    content: ListContent,
}

impl UniformWrap {
    pub fn new(
        count: usize,
        width: f64,
        minimum_width: f64,
        item_height: f64,
        gap: [f64; 2],
    ) -> Result<Self, VirtualError> {
        if !width.is_finite() || width <= 0.0 || !minimum_width.is_finite() || minimum_width <= 0.0
        {
            return Err(VirtualError::InvalidExtent);
        }
        // Validate gaps and row extent before using them to choose columns.
        UniformList::new(0, minimum_width, gap[0])?;
        UniformList::new(0, item_height, gap[1])?;
        let stride = minimum_width + gap[0];
        let columns = ((width / stride + gap[0] / stride).floor() as usize)
            .max(1)
            .min(count.max(1));
        let size = [
            (width - columns.saturating_sub(1) as f64 * gap[0]) / columns as f64,
            item_height,
        ];
        let grid = UniformGrid::new(count.div_ceil(columns), columns, size, gap)?;
        Ok(Self {
            count,
            columns,
            size,
            grid,
            content: ListContent::new(grid.total()[1], [0.0; 2], 0.0, ListContentAlign::Start)?,
        })
    }

    /// Align complete rows shorter than the measured vertical viewport. Reapply
    /// after resizing; this changes origins and content geometry, never row size.
    /// Start restores the natural extent. No per-item storage is introduced.
    pub fn with_content_align(
        self,
        viewport: f64,
        align: ListContentAlign,
    ) -> Result<Self, VirtualError> {
        self.with_content([0.0; 2], viewport, align)
    }

    /// Surround rows with scrolling content, then align the complete content.
    /// Insets are [before rows, after rows], excluding inter-row gaps. Reapply
    /// after resizing; every wrap query uses these physical coordinates.
    pub fn with_content(
        mut self,
        insets: [f64; 2],
        viewport: f64,
        align: ListContentAlign,
    ) -> Result<Self, VirtualError> {
        self.content = ListContent::new(self.grid.total()[1], insets, viewport, align)?;
        Ok(self)
    }

    /// Physical row insets, including short-content alignment padding.
    pub fn content(&self) -> ListContent {
        self.content
    }

    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn columns(&self) -> usize {
        self.columns
    }
    pub fn cell_size(&self) -> [f64; 2] {
        self.size
    }
    /// Natural row grid, before surrounding content and alignment. Use total and
    /// item_origin for the physical content geometry of this wrap.
    pub fn grid(&self) -> UniformGrid {
        self.grid
    }
    /// Physical size. Empty wraps retain their available width for surrounding
    /// content, while the natural grid and data windows remain empty.
    pub fn total(&self) -> [f64; 2] {
        [
            if self.is_empty() {
                self.size[0]
            } else {
                self.grid.total()[0]
            },
            self.content.total(),
        ]
    }

    pub fn cell(&self, index: usize) -> Option<[usize; 2]> {
        (index < self.count).then(|| [index / self.columns, index % self.columns])
    }

    /// Invalid coordinates, including empty cells in the final row, have no item.
    pub fn index(&self, row: usize, column: usize) -> Option<usize> {
        if column >= self.columns {
            return None;
        }
        row.checked_mul(self.columns)?
            .checked_add(column)
            .filter(|i| *i < self.count)
    }

    pub fn item_origin(&self, index: usize) -> Option<[f64; 2]> {
        let [row, column] = self.cell(index)?;
        let [x, y] = self.grid.cell_origin(row, column)?;
        Some([x, y + self.content.insets()[0]])
    }

    /// Flat data indices meeting the vertical overlap policy. Items fill the
    /// available width, so all real items in a qualifying row qualify together.
    /// Excludes gaps and final-row padding; independent of the mounted window.
    pub fn viewable_items(
        &self,
        offset: f64,
        viewport: f64,
        policy: ListViewability,
    ) -> Result<Range<usize>, VirtualError> {
        let state = self.content.viewport(offset, viewport, |start, extent| {
            self.grid.rows.viewport(start, extent)
        })?;
        let rows = self.content.viewable_items(&state, policy, |index| {
            let start = self.grid.rows.position(index).unwrap();
            (start, start + self.size[1])
        })?;
        Ok(rows.start.saturating_mul(self.columns).min(self.count)
            ..rows.end.saturating_mul(self.columns).min(self.count))
    }

    /// Flat data indices under the full parent viewport overlap policy.
    pub fn viewable_items_in_parent(
        &self,
        parent: ParentViewport,
        policy: ListViewability,
    ) -> Result<Range<usize>, VirtualError> {
        let state = self.content.viewport_in_parent(parent, |offset, extent| {
            self.grid.rows.viewport(offset, extent)
        })?;
        let rows = self.content.viewable_items(&state, policy, |index| {
            let start = self.grid.rows.position(index).unwrap();
            (start, start + self.size[1])
        })?;
        Ok(rows.start.saturating_mul(self.columns).min(self.count)
            ..rows.end.saturating_mul(self.columns).min(self.count))
    }

    /// Mount whole rows, checking the real item count rather than padding cells
    /// in the last row. Width is the available width supplied to the constructor.
    pub fn window(
        &self,
        offset: f64,
        viewport: f64,
        overscan: usize,
        capacity: usize,
    ) -> Result<UniformGridWindow, VirtualError> {
        let rows = self.content.viewport(offset, viewport, |start, extent| {
            self.grid.rows.viewport(start, extent)
        })?;
        let mut columns = self.grid.columns.viewport(0.0, self.total()[0])?;
        columns.total_extent = self.total()[0];
        let mounted = if viewport == 0.0 || self.is_empty() {
            0..0
        } else {
            rows.visible.start.saturating_sub(overscan)
                ..rows
                    .visible
                    .end
                    .saturating_add(overscan)
                    .min(rows.item_count)
        };
        let start = if mounted.start == rows.item_count {
            self.count
        } else {
            mounted.start * self.columns
        };
        let end = if mounted.end == rows.item_count {
            self.count
        } else {
            mounted.end * self.columns
        };
        if end - start > capacity {
            return Err(VirtualError::Capacity);
        }
        Ok(UniformGridWindow {
            viewport: GridViewport { columns, rows },
            columns: if mounted.is_empty() {
                0..0
            } else {
                0..self.columns
            },
            rows: mounted,
        })
    }

    /// Borrow the parent's vertical viewport, preserving surrounding content.
    /// No rows are mounted when the entire wrap is outside the viewport.
    pub fn window_in_parent(
        &self,
        parent: ParentViewport,
        overscan: usize,
        capacity: usize,
    ) -> Result<UniformGridWindow, VirtualError> {
        let (offset, extent) = parent.intersection(self.content.total())?;
        let mut window = self.window(offset, extent, overscan, capacity)?;
        window.viewport.rows.offset = parent.offset();
        window.viewport.rows.viewport_extent = parent.extent();
        Ok(window)
    }

    /// Vertical prediction with the same bounded fallback as UniformGrid.
    /// Capacity counts real items, including a partial last row, without padding.
    pub fn window_with_scroll_ahead(
        &self,
        offset: f64,
        viewport: f64,
        overscan: usize,
        capacity: usize,
        ahead: f64,
    ) -> Result<UniformGridWindow, VirtualError> {
        let base = self.window(offset, viewport, overscan, capacity)?;
        let mut predicted = base.clone();
        if !ahead.is_finite() {
            return Err(VirtualError::InvalidExtent);
        }
        if ahead != 0.0 && viewport > 0.0 {
            let offset = base.viewport.rows.offset;
            let ahead = ahead.clamp(-viewport, viewport);
            let start = (offset + ahead.min(0.0)).max(0.0);
            let end = offset + viewport.min((self.content.total() - offset).max(0.0));
            let end = end + ahead.max(0.0).min((self.content.total() - end).max(0.0));
            let extra = self
                .content
                .viewport(start, end - start, |start, extent| {
                    self.grid.rows.viewport(start, extent)
                })?
                .visible;
            predicted.rows.start = predicted.rows.start.min(extra.start);
            predicted.rows.end = predicted.rows.end.max(extra.end);
        }
        let start = predicted
            .rows
            .start
            .saturating_mul(self.columns)
            .min(self.count);
        let end = predicted
            .rows
            .end
            .saturating_mul(self.columns)
            .min(self.count);
        if end - start > capacity {
            return Ok(base);
        }
        predicted.columns = if predicted.rows.is_empty() {
            0..0
        } else {
            0..self.columns
        };
        Ok(predicted)
    }

    /// Parent prediction counts real items, including partial final rows, and
    /// keeps the full parent viewport for viewability and navigation reports.
    pub fn window_in_parent_with_scroll_ahead(
        &self,
        parent: ParentViewport,
        overscan: usize,
        capacity: usize,
        ahead: f64,
    ) -> Result<UniformGridWindow, VirtualError> {
        let (offset, extent) = parent.intersection(self.content.total())?;
        let mut window =
            self.window_with_scroll_ahead(offset, extent, overscan, capacity, ahead)?;
        window.viewport.rows.offset = parent.offset();
        window.viewport.rows.viewport_extent = parent.extent();
        Ok(window)
    }

    /// Align an item in its parent, including header/leading content, without
    /// reducing the parent's scroll range to this child's extent.
    pub fn scroll_to_item_in_parent(
        &self,
        index: usize,
        parent: ParentViewport,
        parent_total: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        let origin = self.item_origin(index).ok_or(VirtualError::UnknownKey)?;
        parent.scroll_to(origin[1], self.size[1], parent_total, align, extra)
    }

    /// The first intersecting row's first item and its depth. No data means no
    /// anchor. A viewport containing only a gap does not identify an item.
    pub fn anchor(
        &self,
        offset: f64,
        viewport: f64,
    ) -> Result<Option<ListScrollPosition>, VirtualError> {
        VirtualAxis::<usize>::viewport(offset, viewport)?;
        let offset = offset.min(self.content.max_offset(viewport)?);
        let Some(offset) = self.content.anchor_offset(offset)? else {
            return Ok(None);
        };
        Ok(self
            .grid
            .rows
            .scroll_position(offset, viewport)?
            .map(|p| ListScrollPosition {
                index: p.index * self.columns,
                offset: p.offset,
            }))
    }

    /// Preserve an item's row and pixel depth after a width/column change.
    /// The adapter may keep the same anchor across successive reflows until the
    /// user scrolls; using each new row's first item would accumulate drift.
    pub fn restore_anchor(
        &self,
        anchor: ListScrollPosition,
        viewport: f64,
    ) -> Result<f64, VirtualError> {
        VirtualAxis::<usize>::viewport(anchor.offset, viewport)?;
        let [row, _] = self.cell(anchor.index).ok_or(VirtualError::UnknownKey)?;
        let start = self.grid.rows.position(row).unwrap();
        self.content.restore(
            start,
            (start + self.size[1]).min(self.grid.rows.total()),
            anchor.offset.min(self.size[1].next_down()),
            viewport,
        )
    }

    pub fn scroll_to_item(
        &self,
        index: usize,
        offset: f64,
        viewport: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        let [row, _] = self.cell(index).ok_or(VirtualError::UnknownKey)?;
        let start = self.grid.rows.position(row).unwrap();
        self.content.scroll_to(
            start,
            self.size[1].min(self.grid.rows.total() - start),
            offset,
            viewport,
            align,
            extra,
        )
    }
}

impl ListViewport {
    /// O(1) row navigation. Positive extra offset leaves space before the row.
    pub fn uniform_scroll_to(
        item_count: usize,
        item_extent: f64,
        index: usize,
        offset: f64,
        viewport_extent: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        UniformList::new(item_count, item_extent, 0.0)?.scroll_to_index(
            index,
            offset,
            viewport_extent,
            align,
            extra,
        )
    }

    pub fn uniform_viewable_items(
        item_count: usize,
        item_extent: f64,
        offset: f64,
        viewport_extent: f64,
        policy: ListViewability,
    ) -> Result<Range<usize>, VirtualError> {
        UniformList::new(item_count, item_extent, 0.0)?.viewable_items(
            offset,
            viewport_extent,
            policy,
        )
    }

    /// Constant-space geometry for a uniform list, without building an axis.
    pub fn uniform(
        item_count: usize,
        item_extent: f64,
        offset: f64,
        viewport_extent: f64,
    ) -> Result<Self, VirtualError> {
        UniformList::new(item_count, item_extent, 0.0)?.viewport(offset, viewport_extent)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VirtualWindow {
    pub visible: Range<usize>,
    pub mounted: Vec<usize>,
    pub offset: f64,
    pub total: f64,
}

// Keep the contiguous window implicit until every axis passes its budget.
// Only pins outside that range need ordered storage during planning.
struct VirtualWindowPlan {
    visible: Range<usize>,
    pinned: BTreeSet<usize>,
    offset: f64,
    total: f64,
}

impl VirtualWindowPlan {
    fn len(&self) -> usize {
        self.visible.len() + self.pinned.len()
    }

    fn finish(self) -> VirtualWindow {
        let mut mounted = Vec::with_capacity(self.len());
        let before = self.pinned.range(..self.visible.start);
        let after = self.pinned.range(self.visible.end..);
        mounted.extend(before.copied());
        mounted.extend(self.visible.clone());
        mounted.extend(after.copied());
        VirtualWindow {
            visible: self.visible,
            mounted,
            offset: self.offset,
            total: self.total,
        }
    }
}

pub struct VirtualAxis<Key> {
    keys: Vec<Key>,
    indices: BTreeMap<Key, usize>,
    extents: Vec<f64>,
    sums: Vec<f64>,
    total: f64,
    gap: f64,
    estimate: Option<f64>,
    unknown_sums: Vec<usize>,
    unknown_count: usize,
}

impl<Key: Clone + Ord> VirtualAxis<Key> {
    /// Unmeasured rows share an adjustable estimate. Updating that estimate is
    /// O(1); measured rows and prefix queries remain O(log n).
    pub fn estimated(keys: impl IntoIterator<Item = Key>, estimate: f64) -> Result<Self, VirtualError> {
        let mut axis = Self::new(keys, estimate)?;
        axis.mark_unknown(estimate, |_| true);
        Ok(axis)
    }

    fn mark_unknown(&mut self, estimate: f64, unknown: impl Fn(&Key) -> bool) {
        self.estimate = Some(estimate);
        self.unknown_count = 0;
        self.unknown_sums = vec![0; self.len() + 1];
        self.sums.fill(0.0);
        self.total = 0.0;
        for i in 0..self.len() {
            if unknown(&self.keys[i]) {
                self.extents[i] = 0.0;
                self.unknown_count += 1;
                self.unknown_sums[i + 1] += 1;
            } else {
                self.total += self.extents[i];
            }
            let cursor = i + 1;
            self.sums[cursor] += self.extents[i];
            let parent = cursor + (cursor & cursor.wrapping_neg());
            if parent < self.sums.len() {
                self.sums[parent] += self.sums[cursor];
                self.unknown_sums[parent] += self.unknown_sums[cursor];
            }
        }
    }

    fn extent_at(&self, index: usize) -> f64 {
        let extent = self.extents[index];
        if extent == 0.0 { self.estimate.unwrap() } else { extent }
    }

    fn row_total(&self) -> f64 {
        self.total + self.unknown_count as f64 * self.estimate.unwrap_or(0.0)
    }

    /// Infer unseen row sizes from all retained measurements, without walking
    /// the dataset on every layout/scroll sample. Capture an anchor first.
    pub fn estimate_from_measurements(&mut self) -> Result<bool, VirtualError> {
        let measured = self.len() - self.unknown_count;
        if self.estimate.is_none() || measured == 0 { return Ok(false); }
        let estimate = self.total / measured as f64;
        if !estimate.is_finite() || estimate <= 0.0
            || !(self.total + self.unknown_count as f64 * estimate
                + self.len().saturating_sub(1) as f64 * self.gap).is_finite() {
            return Err(VirtualError::InvalidExtent);
        }
        let changed = self.estimate != Some(estimate) && self.unknown_count > 0;
        self.estimate = Some(estimate);
        Ok(changed)
    }
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
            gap: 0.0,
            estimate: None,
            unknown_sums: Vec::new(),
            unknown_count: 0,
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
        self.row_total() + self.len().saturating_sub(1) as f64 * self.gap
    }

    /// Set space between rows without rebuilding the prefix index. Extents stay
    /// row-only; replacement/append retain spacing. Invalid updates are atomic.
    pub fn set_gap(&mut self, gap: f64) -> Result<bool, VirtualError> {
        if !gap.is_finite()
            || gap < 0.0
            || !(self.row_total() + self.len().saturating_sub(1) as f64 * gap).is_finite()
        {
            return Err(VirtualError::InvalidExtent);
        }
        let changed = self.gap != gap;
        self.gap = gap;
        Ok(changed)
    }

    pub fn extent(&self, key: &Key) -> Option<f64> {
        self.index(key).map(|index| self.extent_at(index))
    }

    pub fn is_measured(&self, key: &Key) -> bool {
        self.index(key).is_some_and(|index| self.extents[index] > 0.0)
    }

    pub fn position(&self, index: usize) -> Option<f64> {
        if index > self.len() {
            return None;
        }
        let mut cursor = index;
        let mut sum = 0.0;
        while cursor != 0 {
            sum += self.sums[cursor];
            sum += self.unknown_sums.get(cursor).copied().unwrap_or(0) as f64 * self.estimate.unwrap_or(0.0);
            cursor -= cursor & cursor.wrapping_neg();
        }
        Some(sum + index.min(self.len().saturating_sub(1)) as f64 * self.gap)
    }

    pub fn measure(&mut self, key: &Key, extent: f64) -> Result<bool, VirtualError> {
        if !extent.is_finite() || extent <= 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        let index = self.index(key).ok_or(VirtualError::UnknownKey)?;
        let difference = extent - self.extents[index];
        let total = self.total + difference;
        let unknown = self.extents[index] == 0.0;
        let effective_total = total + (self.unknown_count - usize::from(unknown)) as f64 * self.estimate.unwrap_or(0.0);
        if !total.is_finite()
            || total <= 0.0
            || !(effective_total + self.len().saturating_sub(1) as f64 * self.gap).is_finite()
        {
            return Err(VirtualError::InvalidExtent);
        }
        if difference == 0.0 {
            return Ok(false);
        }
        let mut cursor = index + 1;
        while cursor < self.sums.len() {
            self.sums[cursor] += difference;
            if unknown { self.unknown_sums[cursor] -= 1; }
            cursor += cursor & cursor.wrapping_neg();
        }
        self.extents[index] = extent;
        self.total = total;
        self.unknown_count -= usize::from(unknown);
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
        if !(total + self.unknown_count as f64 * self.estimate.unwrap_or(0.0)
            + (self.len() + count).saturating_sub(1) as f64 * self.gap).is_finite() {
            return Err(VirtualError::InvalidExtent);
        }
        self.keys.reserve(count);
        self.extents.reserve(count);
        self.sums.reserve(count);
        for (key, extent) in pending {
            let index = self.len();
            let cursor = index + 1;
            let start = cursor - (cursor & cursor.wrapping_neg());
            // A new Fenwick node may span old rows (especially at powers of two).
            let mut sum = extent;
            let mut unknowns = 0;
            let mut previous = index;
            while previous > start {
                sum += self.sums[previous];
                unknowns += self.unknown_sums.get(previous).copied().unwrap_or(0);
                previous -= previous & previous.wrapping_neg();
            }
            self.indices.insert(key.clone(), index);
            self.keys.push(key);
            self.extents.push(extent);
            self.sums.push(sum);
            if self.estimate.is_some() { self.unknown_sums.push(unknowns); }
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

    /// Replace a keyed row interval with explicit extents. Keys removed by this
    /// edit may be reused; all resulting keys must be unique. Validation and
    /// rebuilding are atomic, retain the gap, and cost O((n + k) log(n + k)).
    pub fn splice(
        &mut self,
        range: Range<usize>,
        items: impl IntoIterator<Item = (Key, f64)>,
    ) -> Result<(), VirtualError> {
        *self = self.spliced(range, items)?;
        Ok(())
    }

    fn spliced(
        &self,
        range: Range<usize>,
        items: impl IntoIterator<Item = (Key, f64)>,
    ) -> Result<Self, VirtualError> {
        if range.start > range.end || range.end > self.len() {
            return Err(VirtualError::UnknownKey);
        }
        let existing = |range: Range<usize>| range.map(|i| (self.keys[i].clone(), self.extent_at(i)));
        let mut result = Self::from_extents(
            existing(0..range.start)
                .chain(items)
                .chain(existing(range.end..self.len())),
        )?;
        if let Some(estimate) = self.estimate {
            result.mark_unknown(estimate, |key| self.index(key).is_some_and(|i|
                !range.contains(&i) && self.extents[i] == 0.0));
        }
        result.set_gap(self.gap)?;
        Ok(result)
    }

    /// Splice in an ancestor viewport. Only edits entirely before its top
    /// compensate; deleting an intersecting row lets the new content show.
    /// The supplied parent total is AFTER the edit, including surrounding page
    /// content. An axis wholly above the viewport can still anchor that page.
    /// Neither the axis nor the scroll snapshot changes on validation failure.
    pub fn splice_in_parent(
        &mut self,
        range: Range<usize>,
        items: impl IntoIterator<Item = (Key, f64)>,
        parent: ParentViewport,
        parent_total: f64,
    ) -> Result<ParentViewport, VirtualError> {
        let next = self.spliced(range.clone(), items)?;
        if parent.content_start > parent_total || next.total() > parent_total - parent.content_start
        {
            return Err(VirtualError::InvalidExtent);
        }
        let next_end = next.len() - (self.len() - range.end);
        // Include the gap before the edit: removing a final row also removes
        // that gap, while inserting into an empty axis creates none.
        let after = next.splice_bounds(range.start..next_end);
        let before = self.splice_bounds(range);
        let parent = parent.preserve_change(before, after.end - after.start, parent_total)?;
        *self = next;
        Ok(parent)
    }

    fn splice_bounds(&self, range: Range<usize>) -> Range<f64> {
        let start = if range.start == 0 {
            0.0
        } else {
            self.position(range.start - 1).unwrap() + self.extent_at(range.start - 1)
        };
        start..self.position(range.end).unwrap()
    }

    pub fn replacement(
        &self,
        keys: impl IntoIterator<Item = Key>,
        estimate: f64,
    ) -> Result<Self, VirtualError> {
        if !estimate.is_finite() || estimate <= 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        let mut result = Self::from_extents(keys.into_iter().map(|key| {
            let extent = self.extent(&key).unwrap_or(estimate);
            (key, extent)
        }))?;
        if let Some(current) = self.estimate {
            result.mark_unknown(current, |key| self.index(key).is_none_or(|i| self.extents[i] == 0.0));
        }
        result.set_gap(self.gap)?;
        Ok(result)
    }

    fn at(&self, offset: f64) -> usize {
        if offset >= self.total() {
            return self.len();
        }
        let mut index = 0;
        let mut sum = 0.0;
        let mut step = self.sums.len().next_power_of_two() / 2;
        while step > 0 {
            let next = index + step;
            let extent = self.sums.get(next).copied().unwrap_or(0.0)
                + self.unknown_sums.get(next).copied().unwrap_or(0) as f64 * self.estimate.unwrap_or(0.0);
            if next < self.sums.len() && sum + extent + next as f64 * self.gap <= offset {
                sum += extent;
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
            within: (offset - self.position(index).unwrap()).clamp(
                0.0,
                self.extent_at(index)
                    + if index + 1 < self.len() {
                        self.gap
                    } else {
                        0.0
                    },
            ),
        }))
    }

    pub fn restore_anchor(
        &self,
        anchor: &ScrollAnchor<Key>,
        viewport: f64,
    ) -> Result<f64, VirtualError> {
        Self::viewport(anchor.within, viewport)?;
        let index = self.index(&anchor.key).ok_or(VirtualError::UnknownKey)?;
        self.restore_position(
            ListScrollPosition {
                index,
                offset: anchor.within,
            },
            viewport,
        )
    }

    /// Restore an already resolved row position using measured geometry.
    pub fn restore_position(
        &self,
        position: ListScrollPosition,
        viewport: f64,
    ) -> Result<f64, VirtualError> {
        Self::viewport(position.offset, viewport)?;
        if position.index >= self.len() {
            return Err(VirtualError::UnknownKey);
        }
        Ok(restore_list_position(
            self.position(position.index).unwrap(),
            self.position(position.index + 1).unwrap(),
            self.total(),
            position.offset,
            viewport,
        ))
    }

    /// Query the actual viewport rather than the mounted (overscanned) window.
    /// Empty lists and zero-sized viewports have no scroll position.
    pub fn scroll_position(
        &self,
        offset: f64,
        viewport: f64,
    ) -> Result<Option<ListScrollPosition>, VirtualError> {
        let state = self.viewport_geometry(offset, viewport)?;
        Ok((!state.visible.is_empty()).then(|| ListScrollPosition {
            index: state.visible.start,
            offset: (state.offset - self.position(state.visible.start).unwrap()).max(0.0),
        }))
    }

    /// Read measured viewport geometry in O(log n), without materializing rows.
    pub fn viewport_geometry(
        &self,
        offset: f64,
        viewport: f64,
    ) -> Result<ListViewport, VirtualError> {
        Self::viewport(offset, viewport)?;
        let offset = offset.min((self.total() - viewport).max(0.0));
        let mut start = self.at(offset);
        if start < self.len() && offset >= self.position(start).unwrap() + self.extent_at(start) {
            start += 1;
        }
        let end_pixel = (offset + viewport).min(self.total());
        let end = self.at(end_pixel);
        let end = if viewport == 0.0 {
            start
        } else if self.position(end) == Some(end_pixel) {
            end
        } else {
            (end + 1).min(self.len())
        };
        Ok(ListViewport {
            visible: start..end.max(start),
            offset,
            viewport_extent: viewport,
            total_extent: self.total(),
            item_count: self.len(),
        })
    }

    /// Measured intersection with an ancestor viewport in O(log n), without
    /// allocating row collections or clamping to this region's scroll range.
    pub fn viewport_in_parent(&self, parent: ParentViewport) -> Result<ListViewport, VirtualError> {
        parent.viewport(self.total(), |offset, extent| {
            self.viewport_geometry(offset, extent)
        })
    }

    /// Measured counterpart of UniformList::sticky_header. Reuses the prefix
    /// index; O(log h * log n) queries with no allocation or intervening-row scan.
    pub fn sticky_header(
        &self,
        headers: &ListStickyHeaders,
        offset: f64,
        viewport: f64,
        inset: f64,
    ) -> Result<Option<ListStickyHeader>, VirtualError> {
        headers.layout(&self.viewport_geometry(offset, viewport)?, inset, |i| {
            (self.position(i).unwrap(), self.extent_at(i))
        })
    }

    /// Thresholded measured geometry in O(log n). Only boundary rows are queried;
    /// complete interior rows always qualify, regardless of their height.
    pub fn viewable_items(
        &self,
        offset: f64,
        viewport: f64,
        policy: ListViewability,
    ) -> Result<Range<usize>, VirtualError> {
        policy.validate()?;
        let viewport = self.viewport_geometry(offset, viewport)?;
        Ok(policy.filter(&viewport, |index| {
            let start = self.position(index).unwrap();
            (start, start + self.extent_at(index))
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
        let offset = offset.min((self.total() - viewport).max(0.0));
        let at_end = follow_end && offset == (self.total() - viewport).max(0.0);
        let anchor = self.anchor(offset)?;
        self.measure(key, extent)?;
        if at_end {
            return Ok((self.total() - viewport).max(0.0));
        }
        match anchor {
            Some(anchor) => self.restore_anchor(&anchor, viewport),
            None => Ok(0.0),
        }
    }

    /// Measure a row using the ancestor's anchoring rule, in O(log n). The
    /// parent origin is the row axis's natural start (including any preceding
    /// slots); `parent_total` includes the new measurement. Partially visible
    /// rows do not compensate, and offsets never clamp to this axis's end.
    /// Apply the returned snapshot before measuring another row in the batch.
    pub fn measure_in_parent(
        &mut self,
        key: &Key,
        extent: f64,
        parent: ParentViewport,
        parent_total: f64,
    ) -> Result<ParentViewport, VirtualError> {
        let index = self.index(key).ok_or(VirtualError::UnknownKey)?;
        if parent.content_start > parent_total
            || self.total() + (extent - self.extent_at(index)) > parent_total - parent.content_start
        {
            return Err(VirtualError::InvalidExtent);
        }
        let start = self.position(index).unwrap();
        let parent =
            parent.preserve_change(start..start + self.extent_at(index), extent, parent_total)?;
        self.measure(key, extent)?;
        Ok(parent)
    }

    /// Reveal a focused row with minimal movement. A partially visible row
    /// larger than the viewport retains the offset; a hidden large row aligns
    /// its start. Empty viewports do not trigger a focus scroll.
    pub fn reveal(&self, key: &Key, offset: f64, viewport: f64) -> Result<f64, VirtualError> {
        let index = self.index(key).ok_or(VirtualError::UnknownKey)?;
        self.scroll_to_index(index, offset, viewport, ListScrollAlign::Nearest, 0.0)
    }

    /// O(log N) navigation using measured extents and estimates for unseen rows.
    pub fn scroll_to_index(
        &self,
        index: usize,
        offset: f64,
        viewport: f64,
        align: ListScrollAlign,
        extra: f64,
    ) -> Result<f64, VirtualError> {
        Self::viewport(offset, viewport)?;
        if index >= self.len() { return Err(VirtualError::UnknownKey); }
        let extent = self.extent_at(index);
        align.offset(
            self.position(index).unwrap(),
            extent,
            self.total(),
            offset,
            viewport,
            extra,
        )
    }

    pub fn window(
        &self,
        offset: f64,
        viewport: f64,
        overscan: f64,
        pinned: &[Key],
        capacity: usize,
    ) -> Result<VirtualWindow, VirtualError> {
        Ok(self
            .window_plan(offset, viewport, overscan, pinned, capacity)?
            .finish())
    }

    fn window_plan(
        &self,
        offset: f64,
        viewport: f64,
        overscan: f64,
        pinned: &[Key],
        capacity: usize,
    ) -> Result<VirtualWindowPlan, VirtualError> {
        Self::viewport(offset, viewport)?;
        if !overscan.is_finite() || overscan < 0.0 {
            return Err(VirtualError::InvalidExtent);
        }
        let offset = offset.min((self.total() - viewport).max(0.0));
        let start_pixel = (offset - overscan).max(0.0);
        let end_pixel = (offset + viewport + overscan).min(self.total());
        let visible = if viewport == 0.0 {
            let start = self.viewport_geometry(offset, 0.0)?.visible.start;
            start..start
        } else {
            self.viewport_geometry(start_pixel, (end_pixel - start_pixel).max(0.0))?
                .visible
        };
        if visible.len() > capacity {
            return Err(VirtualError::Capacity);
        }
        let mut plan = VirtualWindowPlan {
            visible,
            pinned: BTreeSet::new(),
            offset,
            total: self.total(),
        };
        for key in pinned {
            let index = self.index(key).ok_or(VirtualError::UnknownKey)?;
            if !plan.visible.contains(&index) {
                plan.pinned.insert(index);
            }
            if plan.len() > capacity {
                return Err(VirtualError::Capacity);
            }
        }
        Ok(plan)
    }

    /// Build a measured-row mount window in an ancestor's viewport. Completely
    /// offscreen or zero-sized intersections suppress overscan, but explicit
    /// pins remain mounted. The returned offset stays in signed local space.
    pub fn window_in_parent(
        &self,
        parent: ParentViewport,
        overscan: f64,
        pinned: &[Key],
        capacity: usize,
    ) -> Result<VirtualWindow, VirtualError> {
        let (offset, viewport) = parent.intersection(self.total())?;
        let mut window = self.window(offset, viewport, overscan, pinned, capacity)?;
        window.offset = parent.offset();
        Ok(window)
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
        let mut window = self.window(offset, viewport, overscan, pinned, capacity)?;
        for index in start..end {
            if let Err(position) = window.mounted.binary_search(&index) {
                if window.mounted.len() == capacity {
                    return Err(VirtualError::Capacity);
                }
                window.mounted.insert(position, index);
            }
        }
        Ok(window)
    }

    /// Keep focus and its neighbors mounted in parent mode, even when the
    /// content is offscreen. Capacity counts the distinct union of visible,
    /// overscanned, pinned and focus-adjacent rows, as in owned scroll mode.
    pub fn window_with_focus_in_parent(
        &self,
        parent: ParentViewport,
        overscan: f64,
        pinned: &[Key],
        focused: Option<&Key>,
        capacity: usize,
    ) -> Result<VirtualWindow, VirtualError> {
        let (offset, viewport) = parent.intersection(self.total())?;
        let mut window =
            self.window_with_focus(offset, viewport, overscan, pinned, focused, capacity)?;
        window.offset = parent.offset();
        Ok(window)
    }
}

pub struct VirtualGrid<Row, Column> {
    pub rows: VirtualAxis<Row>,
    pub columns: VirtualAxis<Column>,
}

impl<Row: Clone + Ord, Column: Clone + Ord> VirtualGrid<Row, Column> {
    /// Coordinates and returned windows are [columns, rows]. Capacity bounds
    /// each distinct axis index set and their cell product, checked before
    /// allocating either mount vector. A zero-area viewport or empty axis
    /// suppresses ordinary mounting on both axes; explicit pins remain.
    pub fn window(
        &self,
        offset: [f64; 2],
        viewport: [f64; 2],
        overscan: [f64; 2],
        pinned_rows: &[Row],
        pinned_columns: &[Column],
        capacity: usize,
    ) -> Result<[VirtualWindow; 2], VirtualError> {
        // Validate the original extents before suppressing both axes: hidden
        // geometry must not conceal a malformed dimension.
        VirtualAxis::<Row>::viewport(offset[1], viewport[1])?;
        VirtualAxis::<Column>::viewport(offset[0], viewport[0])?;
        let offset = [
            offset[0].min((self.columns.total() - viewport[0]).max(0.0)),
            offset[1].min((self.rows.total() - viewport[1]).max(0.0)),
        ];
        let viewport = if viewport.contains(&0.0) || self.rows.is_empty() || self.columns.is_empty()
        {
            [0.0; 2]
        } else {
            viewport
        };
        let rows =
            self.rows
                .window_plan(offset[1], viewport[1], overscan[1], pinned_rows, capacity)?;
        let columns = self.columns.window_plan(
            offset[0],
            viewport[0],
            overscan[0],
            pinned_columns,
            capacity,
        )?;
        if rows
            .len()
            .checked_mul(columns.len())
            .is_none_or(|count| count > capacity)
        {
            return Err(VirtualError::Capacity);
        }
        Ok([columns.finish(), rows.finish()])
    }

    /// Mount a measured grid inside an ancestor's rectangular viewport.
    /// Overscan starts at the clipped intersection. Wholly offscreen grids
    /// suppress ordinary mounting on both axes, while explicit row/column pins
    /// remain subject to the same axis and cell budgets as owned scrolling.
    /// Returned offsets retain signed local coordinates and full axis extents.
    pub fn window_in_parent(
        &self,
        parent: [ParentViewport; 2],
        overscan: [f64; 2],
        pinned_rows: &[Row],
        pinned_columns: &[Column],
        capacity: usize,
    ) -> Result<[VirtualWindow; 2], VirtualError> {
        let (x, width) = parent[0].intersection(self.columns.total())?;
        let (y, height) = parent[1].intersection(self.rows.total())?;
        let mut window = self.window(
            [x, y],
            [width, height],
            overscan,
            pinned_rows,
            pinned_columns,
            capacity,
        )?;
        window[0].offset = parent[0].offset();
        window[1].offset = parent[1].offset();
        Ok(window)
    }
}
