//! Portable, keyed virtualization on either axis. Only the viewport, overscan and
//! directional pre-render range own reactive rows.
use super::{Element, IntoElementList, Signal, create_scope, create_signal, primitives};
use jadix_core::presentation::style::{Axis, ScrollBehavior};
use jadix_core::virtualization::{ListOrder, ListScrollAhead, ListScrollAlign, UniformList};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
mod measured;

/// Retains a list viewport when its declarative region is rebuilt.
/// Use one state per list; row scopes still belong to the mounted element.
#[derive(Clone, Default)]
pub struct VirtualListState {
    offset: Rc<Cell<f64>>,
    viewport: Rc<Cell<Option<f64>>>,
}
impl VirtualListState {
    pub fn offset(&self) -> f64 { self.offset.get() }
    pub fn viewport(&self) -> Option<f64> { self.viewport.get() }
}

pub struct VirtualListBuilder {
    state: VirtualListState,
    count: Box<dyn Fn() -> usize>,
    extent: f64,
    gap: f64,
    viewport: Option<Signal<f64>>,
    height: Option<f32>,
    width: Option<f32>,
    axis: Axis,
    order: ListOrder,
    retain_on_move: bool,
    measure_rows: bool,
    data_revision: Option<Signal<u64>>,
    follow_end: bool,
    overscan: usize,
    predict: bool,
    smooth: bool,
    key: Box<dyn Fn(usize) -> String>,
    render: Box<dyn Fn(Signal<usize>) -> Element>,
    focus: Option<Box<dyn Fn() -> Option<(usize, usize, ListScrollAlign, Option<ScrollBehavior>)>>>,
    visible_range: Option<Rc<dyn Fn(Option<std::ops::Range<usize>>)>>,
    element: Element,
}

pub fn virtual_list(
    count: usize,
    item_extent: f64,
    render: impl Fn(usize) -> Element + 'static,
) -> VirtualListBuilder {
    VirtualListBuilder {
        state: VirtualListState::default(),
        count: Box::new(move || count),
        extent: item_extent,
        gap: 0.0,
        viewport: None,
        height: None,
        width: None,
        axis: Axis::Column,
        order: ListOrder::Forward,
        retain_on_move: false,
        measure_rows: false,
        data_revision: None,
        follow_end: false,
        overscan: 2,
        predict: true,
        smooth: false,
        key: Box::new(|index| index.to_string()),
        render: Box::new(move |index| render(index.get())),
        focus: None,
        visible_range: None,
        element: primitives::scroll(),
    }
}

/// Render with a live dataset index. A surviving key retains its owner and
/// local state when rows move; reactive bindings read the updated index.
pub fn virtual_list_indexed(
    count: usize,
    item_extent: f64,
    render: impl Fn(Signal<usize>) -> Element + 'static,
) -> VirtualListBuilder {
    let mut builder = virtual_list(count, item_extent, |_| Element::new());
    builder.render = Box::new(render);
    builder.retain_on_move = true;
    builder
}

impl VirtualListBuilder {
    /// Measure mounted rows using the host's actual layout, retaining extents by
    /// key. Unseen rows use item_extent until first measured.
    pub fn measure_rows(mut self) -> Self { self.measure_rows = true; self }
    /// Invalidate the keyed index after changing or reordering the dataset.
    /// Scrolling itself only queries the prefix index and mounted rows.
    pub fn data_revision(mut self, revision: Signal<u64>) -> Self { self.data_revision = Some(revision); self }
    /// Keep the trailing edge visible when rows grow or the dataset changes,
    /// provided the user was already at that edge.
    pub fn follow_end(mut self, follow: bool) -> Self { self.follow_end = follow; self }
    pub fn horizontal(mut self) -> Self { self.axis = Axis::Row; self }
    pub fn reversed(mut self, reversed: bool) -> Self {
        self.order = if reversed { ListOrder::Reversed } else { ListOrder::Forward }; self
    }
    pub fn state(mut self, state: VirtualListState) -> Self { self.state = state; self }
    /// Reveal the active row without moving keyboard focus from a search field.
    pub fn focused_index_fn(self, focus: impl Fn() -> Option<usize> + 'static) -> Self {
        self.scroll_request_fn(move || focus().map(|index| (index, 0, ListScrollAlign::Nearest)))
    }
    /// A changed revision reveals the same row again after a user scroll.
    pub fn focused_index_request_fn(self, focus: impl Fn() -> Option<(usize, usize)> + 'static) -> Self {
        self.scroll_request_fn(move || focus().map(|(index, revision)| (index, revision, ListScrollAlign::Nearest)))
    }
    /// Reveal a row with explicit alignment; revisions allow repeated requests.
    pub fn scroll_request_fn(mut self, request: impl Fn() -> Option<(usize, usize, ListScrollAlign)> + 'static) -> Self {
        self.focus = Some(Box::new(move || request().map(|(index, revision, align)| (index, revision, align, None)))); self
    }
    /// Override the default behavior for each reveal, including a repeated row.
    pub fn scroll_request_with_behavior_fn(mut self, request: impl Fn() -> Option<(usize, usize, ListScrollAlign, ScrollBehavior)> + 'static) -> Self {
        self.focus = Some(Box::new(move || request().map(|(index, revision, align, behavior)| (index, revision, align, Some(behavior))))); self
    }
    /// Reports the visible window, excluding prediction and overscan rows.
    pub fn on_visible_range_change(mut self, callback: impl Fn(Option<std::ops::Range<usize>>) + 'static) -> Self {
        self.visible_range = Some(Rc::new(callback)); self
    }
    pub fn count(mut self, count: Signal<usize>) -> Self {
        self.count = Box::new(move || count.get());
        self
    }
    pub fn key(mut self, key: impl Fn(usize) -> String + 'static) -> Self {
        self.key = Box::new(key);
        self
    }
    pub fn gap(mut self, gap: f64) -> Self {
        self.gap = gap;
        self
    }
    /// Optional viewport source for hosts without native viewport notifications.
    pub fn viewport(mut self, viewport: Signal<f64>) -> Self {
        self.viewport = Some(viewport);
        self
    }
    pub fn h(mut self, height: f32) -> Self {
        self.height = Some(height);
        self.element = self.element.h(height);
        self
    }
    pub fn w(mut self, width: f32) -> Self {
        self.width = Some(width);
        self.element = self.element.w(width);
        self
    }
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.element = self.element.id(id.into());
        self
    }
    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows.min(1024);
        self
    }
    /// Apply the same overflow policy as ordinary menu viewports.
    pub fn menu_scroll(mut self) -> Self {
        self.element = self.element.menu_scroll();
        self
    }
    pub fn scroll_ahead(mut self, enabled: bool) -> Self {
        self.predict = enabled;
        self
    }
    /// Animate reveal requests while virtualizing the actual moving viewport.
    pub fn smooth_scroll(mut self, enabled: bool) -> Self {
        self.smooth = enabled;
        self.element = self.element.scroll_behavior(if enabled {
            jadix_core::presentation::style::ScrollBehavior::Smooth
        } else { jadix_core::presentation::style::ScrollBehavior::Instant });
        self
    }
    pub fn build(self) -> Element {
        if self.measure_rows || self.follow_end { return measured::build(self); }
        let Self {
            state,
            count,
            extent,
            gap,
            viewport,
            height,
            width,
            axis,
            order,
            retain_on_move,
            measure_rows: _,
            data_revision: _,
            follow_end: _,
            overscan,
            predict,
            smooth,
            key,
            render,
            focus,
            visible_range,
            mut element,
        } = self;
        UniformList::new(0, extent, gap).expect("invalid virtual list geometry");
        let count: Rc<dyn Fn() -> usize> = Rc::from(count);
        let offset = state.offset;
        let coordinates = move |value| if axis == Axis::Row { [value, 0.0] } else { [0.0, value] };
        if offset.get() > 0.0 { element.descriptor.style.scroll_to = Some(coordinates(offset.get())); }
        let retained_viewport = state.viewport;
        let manually_scrolled = Rc::new(Cell::new(false));
        let view = Rc::new(Cell::new(retained_viewport.get().unwrap_or_else(|| (if axis == Axis::Row { width } else { height }).map(f64::from).unwrap_or(extent))));
        let scroll_offset = offset.clone();
        let manual = manually_scrolled.clone();
        element = element.on_scroll(move |x, y| {
            let next = (if axis == Axis::Row { x } else { y }).max(0.0);
            if (next - scroll_offset.get()).abs() > 0.01 { manual.set(true); }
            scroll_offset.set(next);
        });
        let native_view = view.clone();
        element.on_viewport = Some(Box::new(move |width, height| { let size = if axis == Axis::Row { width } else { height }; native_view.set(size); retained_viewport.set(Some(size)); }));
        if let Some(focus) = focus {
            let position = offset.clone();
            let native_viewport = view.clone();
            let count = count.clone();
            let previous = Cell::new(None);
            element.descriptor_binding = Some(Rc::new(move |descriptor| {
                if manually_scrolled.replace(false) { descriptor.style.scroll_to = None; }
                let next = focus();
                if previous.replace(next) == next { return; }
                if let Some((index, _, align, behavior)) = next {
                    let behavior = behavior.unwrap_or(if smooth { ScrollBehavior::Smooth } else { ScrollBehavior::Instant });
                    let geometry = UniformList::new(count(), extent, gap).expect("invalid virtual list geometry");
                    let height = viewport.map(|value| value.get()).unwrap_or_else(|| native_viewport.get());
                    let Some(index) = order.index(index, count()) else { return; };
                    if let Ok(target) = geometry.scroll_to_index(index, position.get(), height, align, 0.0) {
                        if behavior == ScrollBehavior::Instant { position.set(target); }
                        descriptor.style.scroll_behavior = Some(behavior);
                        descriptor.style.scroll_to = Some(coordinates(target));
                    }
                }
            }));
        }
        let mut content = Element::new().flex_shrink(0.0);
        content = if axis == Axis::Row { content.flex_row() } else { content.flex_col() };
        let size_row = move |row: Element, size: f64| if axis == Axis::Row { row.w(size as f32) } else { row.h(size as f32) };
        let mut previous: Vec<(String, usize, Signal<usize>)> = Vec::new();
        let mut ahead = ListScrollAhead::default();
        let mut previous_visible = None;
        content.children_binding = Some(Box::new(move |children| {
            let count = count();
            let geometry =
                UniformList::new(count, extent, gap).expect("invalid virtual list geometry");
            assert!(
                geometry.total() <= f32::MAX as f64,
                "virtual list extent exceeds layout coordinates"
            );
            let viewport = viewport
                .map(|value| value.get())
                .unwrap_or_else(|| view.get());
            let visible = geometry
                .viewport(offset.get(), viewport)
                .expect("invalid virtual list viewport");
            let lead = ahead
                .update(visible.offset, viewport)
                .expect("invalid virtual list viewport");
            let lead = if predict { lead } else { 0.0 };
            let pre_start = (visible.offset + lead.min(0.0)).max(0.0);
            let pre_end = visible.offset + viewport + lead.max(0.0);
            let pre = geometry
                .viewport(pre_start, pre_end - pre_start)
                .expect("invalid pre-render viewport");
            let start = pre.visible.start.saturating_sub(overscan);
            let end = pre.visible.end.saturating_add(overscan).min(count);
            let wanted: Vec<_> = (start..end).map(|visual| { let index = order.index(visual, count).unwrap(); (key(index), index) }).collect();
            let mut unique = BTreeSet::new();
            assert!(
                wanted.iter().all(|(key, _)| unique.insert(key)),
                "duplicate virtual list key"
            );
            let old_children = std::mem::take(children);
            let mut old: BTreeMap<_, _> = previous
                .drain(..)
                .zip(old_children.into_iter().skip(1))
                .map(|((key, index, live_index), row)| (key, (index, live_index, row)))
                .collect();
            children.push(size_row(Element::new().flex_shrink(0.0), geometry.position(start).unwrap()));
            let mut next = Vec::with_capacity(wanted.len());
            for (visual, (key, index)) in (start..end).zip(&wanted) {
                let retained = old.remove(key).filter(|(old_index, _, _)| retain_on_move || old_index == index);
                let (live_index, row) = if let Some((_, live_index, row)) = retained {
                    if live_index.get() != *index { live_index.set(*index); }
                    (live_index, row)
                } else {
                    let (owner, (live_index, child)) = create_scope(|| {
                        let live_index = create_signal(*index);
                        (live_index, render(live_index))
                    });
                    let mut row = Element::new().child(child).flex_shrink(0.0);
                    row.owned_scopes.push(owner);
                    (live_index, row)
                };
                let size = geometry.position(visual + 1).unwrap() - geometry.position(visual).unwrap();
                children.push(size_row(row, size));
                next.push((key.clone(), *index, live_index));
            }
            children.push(size_row(Element::new().flex_shrink(0.0), geometry.total() - geometry.position(end).unwrap()));
            previous = next;
            // Drop retired elements before their child scope handles are released.
            drop(old);
            let logical_visible = order.range(visible.visible, count).unwrap();
            if previous_visible.as_ref() != Some(&logical_visible) {
                previous_visible = Some(logical_visible.clone());
                if let Some(callback) = &visible_range { callback(Some(logical_visible)); }
            }
        }));
        element.children.push(content);
        element
    }
}
impl From<VirtualListBuilder> for Element {
    fn from(value: VirtualListBuilder) -> Self {
        value.build()
    }
}
impl IntoElementList for VirtualListBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.build()]
    }
}
