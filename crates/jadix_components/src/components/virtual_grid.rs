//! Fixed-size two-dimensional virtualization backed by shared UniformGrid.
//!
//! The viewport, overscan, optional prediction and live interaction owners are mounted within the
//! configured budget. Coordinate identity
//! survives scrolling and metric/count changes; data_key refreshes the dataset.
//! Grid axes and VirtualWrap items can opt into stable business keys and
//! edit-position restoration.
//! Selection, keyboard navigation and variable cell sizes belong to
//! higher-level consumers. All size/offset arrays use [x, y] order.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
    time::Duration,
};

use crate::components::{base::SlotChrome, scroll_area::scroll_area};
use crate::core::{
    reactivity::{Signal, use_signal},
    scroll_viewport::{
        ScrollViewportSignalKeys, cancel_scroll_end_x, cancel_scroll_end_y,
        scroll_content_h_signal_key, scroll_content_w_signal_key,
    },
    tree::{Element, LayoutStatus},
};
use crate::utils::virtual_list::subtree_needs_virtual_retention;
pub use crate::virtualization::{GridViewport, UniformGridWindow};
use crate::virtualization::{
    ListContentAlign, ListLoadEdges, ListOrder, ListScrollAhead, ListScrollAlign,
    ListScrollPosition, ListViewability, ListViewabilityTracker, ParentViewport, UniformGrid,
    UniformWrap, ViewableItems, VirtualError,
};

const CELL_ATTR: &str = "__jadix_vgrid_cell";
const INDEX_ATTR: &str = "__jadix_vgrid_index";
const ERROR_ATTR: &str = "data-virtual-grid-error";
const SLOT_ATTR: &str = "__jadix_vwrap_slot";

type ViewportCallback = Rc<dyn Fn(Option<GridViewport>)>;

#[derive(Clone, Copy)]
enum Navigation {
    Cell {
        row: usize,
        column: usize,
        align: [ListScrollAlign; 2],
        extra: [f64; 2],
    },
    End,
    Item {
        index: usize,
        align: ListScrollAlign,
        extra: f64,
    },
}

#[derive(Default)]
struct Geometry {
    grid: Cell<Option<UniformGrid>>,
    wrap: Cell<Option<UniformWrap>>,
    viewport: RefCell<Option<GridViewport>>,
    request: Cell<Option<Navigation>>,
    binding: RefCell<Option<(crate::prelude::UiNode, Weak<()>, Signal<()>)>>,
}

/// Use a distinct handle per mounted grid. Navigation waits for actual viewport
/// dimensions; the most recent request wins. It is a jump on both axes.
#[derive(Clone, Default)]
pub struct VirtualGridHandle(Rc<Geometry>);

impl PartialEq for VirtualGridHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for VirtualGridHandle {}

impl VirtualGridHandle {
    /// Navigate a flat item in a VirtualWrap. Other grid kinds ignore this request.
    pub fn scroll_to_item(&self, index: usize, align: ListScrollAlign, extra: f64) -> bool {
        if align.validate().is_err() || !extra.is_finite() {
            return false;
        }
        self.request(Navigation::Item {
            index,
            align,
            extra,
        })
    }

    pub fn item_origin(&self, index: usize) -> Option<[f64; 2]> {
        self.0.wrap.get()?.item_origin(index)
    }

    pub fn scroll_to_cell(
        &self,
        row: usize,
        column: usize,
        align: [ListScrollAlign; 2],
        extra: [f64; 2],
    ) -> bool {
        if align.iter().any(|a| a.validate().is_err()) || extra.iter().any(|v| !v.is_finite()) {
            return false;
        }
        self.request(Navigation::Cell {
            row,
            column,
            align,
            extra,
        })
    }

    pub fn scroll_to_end(&self) -> bool {
        self.request(Navigation::End)
    }

    fn request(&self, request: Navigation) -> bool {
        let binding = self.0.binding.borrow();
        let Some((node, _, _)) = binding
            .as_ref()
            .filter(|(_, owner, lifetime)| owner.strong_count() > 0 && lifetime.is_alive())
        else {
            return false;
        };
        self.0.request.set(Some(request));
        node.mark_layout();
        true
    }

    pub fn viewport(&self) -> Option<GridViewport> {
        self.0.viewport.borrow().clone()
    }

    pub fn cell_origin(&self, row: usize, column: usize) -> Option<[f64; 2]> {
        if let Some(wrap) = self.0.wrap.get() {
            return wrap.item_origin(wrap.index(row, column)?);
        }
        self.0.grid.get()?.cell_origin(row, column)
    }
}

pub struct VirtualGridBuilder {
    id: String,
    handle: VirtualGridHandle,
    rows: usize,
    columns: usize,
    row_count_fn: Option<Rc<dyn Fn() -> usize>>,
    column_count_fn: Option<Rc<dyn Fn() -> usize>>,
    size: [f32; 2],
    size_fn: Option<Rc<dyn Fn() -> [f32; 2]>>,
    gap: [f32; 2],
    overscan: [usize; 2],
    scroll_ahead: bool,
    use_parent_scroll: bool,
    capacity: usize,
    height: f32,
    data_key_fn: Option<Rc<dyn Fn() -> String>>,
    axis_keys: [Option<Rc<dyn Fn(usize) -> String>>; 2],
    maintain_position: bool,
    render_cell: Rc<dyn Fn(usize, usize) -> Element>,
    on_viewport_change: Option<ViewportCallback>,
    scroll_behavior: String,
    scrollbar_visibility: String,
    chrome: SlotChrome,
    wrap: Option<WrapConfig>,
}

// Axis indices cost O(rows + columns), never O(rows * columns). Scroll and
// idle frames reuse them; only count/data revisions scan business keys.
struct GridAxisKeys {
    keys: Vec<String>,
    indices: HashMap<String, usize>,
}

impl GridAxisKeys {
    fn new(count: usize, key: &dyn Fn(usize) -> String) -> Result<Self, VirtualError> {
        let keys: Vec<_> = (0..count).map(key).collect();
        let indices: HashMap<_, _> = keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, k)| (k, i))
            .collect();
        if indices.len() != keys.len() {
            return Err(VirtualError::DuplicateKey);
        }
        Ok(Self { keys, indices })
    }
}

fn grid_cell_key(axes: &[Option<Rc<GridAxisKeys>>; 2], row: usize, column: usize) -> String {
    let key = |axis: usize, index: usize| {
        axes[axis]
            .as_ref()
            .map_or_else(|| index.to_string(), |keys| keys.keys[index].clone())
    };
    let row = key(1, row);
    let column = key(0, column);
    // Byte-length prefix separates arbitrary Unicode/empty/delimiter keys.
    format!("grid:{}:{row}{column}", row.len())
}

struct WrapConfig {
    count: Rc<dyn Fn() -> usize>,
    minimum_width: f32,
    render_item: Rc<dyn Fn(usize) -> Element>,
    item_key: Option<Rc<dyn Fn(usize) -> String>>,
    maintain_position: bool,
    viewability: ListViewability,
    minimum_view_time: Duration,
    on_viewable_items_change: Option<Rc<dyn Fn(ViewableItems)>>,
    on_load_more: Option<Rc<dyn Fn()>>,
    on_load_more_at_start: Option<Rc<dyn Fn()>>,
    load_more_threshold: f64,
    load_more_at_start_threshold: Option<f64>,
    initial_scroll_at_end: bool,
    follow_end: bool,
    follow_end_threshold: f64,
    content_align: ListContentAlign,
    slots: WrapSlots,
}

// Three authored nodes at most; none participates in cell recycling or budgets.
// Parking also preserves state across empty/nonempty, hidden and error states.
#[derive(Default)]
struct WrapSlots {
    extents: [f64; 3],
    parked: RefCell<[Option<Element>; 3]>,
}

impl WrapSlots {
    fn set(&mut self, index: usize, extent: f32, child: Element) {
        assert!(
            extent.is_finite() && extent >= 0.0,
            "invalid wrap slot extent"
        );
        self.extents[index] = f64::from(extent);
        let mut wrapper = Element::new().child(child);
        wrapper.attrs.insert(SLOT_ATTR.into(), index.to_string());
        self.parked.get_mut()[index] = Some(wrapper);
    }

    fn insets(&self, count: usize) -> [f64; 2] {
        [
            self.extents[0] + if count == 0 { self.extents[2] } else { 0.0 },
            self.extents[1],
        ]
    }

    fn park(&self, child: Element) -> Option<Element> {
        if let Some(index) = child
            .attrs
            .get(SLOT_ATTR)
            .and_then(|i| i.parse::<usize>().ok())
        {
            self.parked.borrow_mut()[index] = Some(child);
            None
        } else {
            Some(child)
        }
    }

    fn mount(&self, root: &mut Element, wrap: UniformWrap) {
        let leading = wrap.content().insets()[0];
        let start = leading - self.insets(wrap.len())[0];
        let origins = [
            start,
            leading + wrap.grid().total()[1],
            start + self.extents[0],
        ];
        let width = wrap.total()[0] as f32;
        let mut parked = self.parked.borrow_mut();
        for (i, slot) in parked.iter_mut().enumerate() {
            if i == 2 && !wrap.is_empty() {
                continue;
            }
            if let Some(child) = slot.take() {
                let height = self.extents[i] as f32;
                root.children.push(
                    child
                        .absolute()
                        .left(0.0)
                        .top(origins[i] as f32)
                        .w(width)
                        .min_w(width)
                        .max_w(width)
                        .h(height)
                        .min_h(height)
                        .max_h(height)
                        .flex_shrink(0.0)
                        .overflow_x("hidden")
                        .overflow_y("hidden"),
                );
            }
        }
    }
}

/// Responsive flat-item grid, sharing the VirtualGrid renderer and handle.
/// Item indices retain mounted identity across column changes. Optional business
/// keys also retain wrapper identity and reading position across data changes.
pub struct VirtualWrapBuilder(VirtualGridBuilder);

pub fn virtual_wrap(id: &str) -> VirtualWrapBuilder {
    let mut grid = virtual_grid(id);
    grid.overscan = [0, 1];
    grid.wrap = Some(WrapConfig {
        count: Rc::new(|| 0),
        minimum_width: 80.0,
        render_item: Rc::new(|i| Element::new().text(format!("item {i}"))),
        item_key: None,
        maintain_position: true,
        viewability: ListViewability::default(),
        minimum_view_time: Duration::ZERO,
        on_viewable_items_change: None,
        on_load_more: None,
        on_load_more_at_start: None,
        load_more_threshold: 1.0,
        load_more_at_start_threshold: None,
        initial_scroll_at_end: false,
        follow_end: false,
        follow_end_threshold: 0.0,
        content_align: ListContentAlign::Start,
        slots: WrapSlots::default(),
    });
    VirtualWrapBuilder(grid)
}

impl VirtualWrapBuilder {
    pub fn item_count(mut self, count: usize) -> Self {
        self.0.wrap.as_mut().unwrap().count = Rc::new(move || count);
        self
    }
    /// Polled during frame synchronization, without subscribing the outer view.
    pub fn item_count_fn(mut self, count: impl Fn() -> usize + 'static) -> Self {
        self.0.wrap.as_mut().unwrap().count = Rc::new(count);
        self
    }
    /// Items share the available width. A viewport narrower than this minimum
    /// gets one narrower column; there is no horizontal scrolling.
    pub fn minimum_item_width(mut self, width: f32) -> Self {
        self.0.wrap.as_mut().unwrap().minimum_width = width;
        self
    }
    pub fn item_height(mut self, height: f32) -> Self {
        self.0.size[1] = height;
        self
    }
    pub fn item_height_fn(mut self, height: impl Fn() -> f32 + 'static) -> Self {
        self.0.size_fn = Some(Rc::new(move || [0.0, height()]));
        self
    }
    pub fn item_gap(mut self, horizontal: f32, vertical: f32) -> Self {
        self.0.gap = [horizontal, vertical];
        self
    }
    pub fn render_item(mut self, render: impl Fn(usize) -> Element + 'static) -> Self {
        self.0.wrap.as_mut().unwrap().render_item = Rc::new(render);
        self
    }
    /// Full-width scrolling header before the rows. Extent includes any desired
    /// spacing; item_gap applies only between data rows and columns.
    pub fn header(mut self, extent: f32, content: impl Into<Element>) -> Self {
        self.0
            .wrap
            .as_mut()
            .unwrap()
            .slots
            .set(0, extent, content.into());
        self
    }
    /// Full-width scrolling footer after the rows, with an explicit height.
    pub fn footer(mut self, extent: f32, content: impl Into<Element>) -> Self {
        self.0
            .wrap
            .as_mut()
            .unwrap()
            .slots
            .set(1, extent, content.into());
        self
    }
    /// Content between header and footer for an empty dataset. Authored slot
    /// nodes retain identity across data changes, hidden viewports and errors.
    /// Slots are excluded from data indices, viewability and mounted-cell limits.
    pub fn empty(mut self, extent: f32, content: impl Into<Element>) -> Self {
        self.0
            .wrap
            .as_mut()
            .unwrap()
            .slots
            .set(2, extent, content.into());
        self
    }
    pub fn handle(mut self, handle: VirtualGridHandle) -> Self {
        self.0.handle = handle;
        self
    }
    pub fn overscan(mut self, rows: usize) -> Self {
        self.0.overscan = [0, rows];
        self
    }
    /// Pre-render in the scroll direction, within the existing mount budget.
    /// Disabled by default; reflow, data changes and navigation reset prediction.
    pub fn scroll_ahead(mut self, enabled: bool) -> Self {
        self.0.scroll_ahead = enabled;
        self
    }
    /// Borrow the nearest vertical ancestor viewport. Width follows this
    /// component's container; height is natural. Local anchor/end policies are
    /// disabled so they cannot move the surrounding parent's content.
    pub fn use_parent_scroll(mut self) -> Self {
        self.0.use_parent_scroll = true;
        self
    }
    /// Includes offscreen focus, selection and portal owners. The requested
    /// window has priority; owners use only the remaining capacity.
    pub fn max_mounted_cells(mut self, capacity: usize) -> Self {
        self.0.capacity = capacity;
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.0 = self.0.height(height);
        self
    }
    pub fn data_key_fn(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.0.data_key_fn = Some(Rc::new(f));
        self
    }
    /// Stable, unique business keys, scanned only when count or data_key changes.
    /// Retains mounted wrappers across edits and reordering. Moved items and
    /// changed data revisions re-render their contents with the current index.
    /// Duplicate keys clear the window with data-virtual-grid-error=DuplicateKey;
    /// fix the keys and change data_key to recover. Keys require O(item_count)
    /// storage; the default index-based mode keeps constant-space geometry.
    pub fn item_key_fn(mut self, f: impl Fn(usize) -> String + 'static) -> Self {
        self.0.wrap.as_mut().unwrap().item_key = Some(Rc::new(f));
        self
    }
    /// With business keys, preserve the first visible item and pixel depth after
    /// edits. Deleted anchors choose the next surviving old item, then the prior
    /// one at its start. Enabled by default; explicit navigation takes priority.
    /// Width reflow preserves its anchor independently of this data-edit policy.
    pub fn maintain_visible_content_position(mut self, enabled: bool) -> Self {
        self.0.wrap.as_mut().unwrap().maintain_position = enabled;
        self
    }
    /// Snap to the end at the first successful nonempty measured viewport.
    /// Empty data, missing viewport and geometry errors keep this request pending.
    /// Explicit handle navigation wins and settles the initial request.
    pub fn initial_scroll_at_end(mut self) -> Self {
        self.0.wrap.as_mut().unwrap().initial_scroll_at_end = true;
        self
    }
    /// Follow appends and geometry changes while at the previous visual end.
    /// Defaults to false. Moving away suspends following; returning resumes it.
    /// Explicit navigation wins over following and business-key restoration.
    pub fn follow_end(mut self, enabled: bool) -> Self {
        self.0.wrap.as_mut().unwrap().follow_end = enabled;
        self
    }
    /// Align complete content (slots and rows) shorter than the viewport. Center
    /// and End fill its height; Start keeps the natural content extent. This does
    /// not enable initial end navigation or following; configure those separately.
    pub fn content_align(mut self, align: ListContentAlign) -> Self {
        self.0.wrap.as_mut().unwrap().content_align = align;
        self
    }
    /// Enable following within this many logical units of the previous end.
    /// Invalid/negative values retain the setting; follow_end(false) disables it.
    pub fn follow_end_threshold(mut self, distance: f32) -> Self {
        if distance.is_finite() && distance >= 0.0 {
            let wrap = self.0.wrap.as_mut().unwrap();
            wrap.follow_end = true;
            wrap.follow_end_threshold = f64::from(distance);
        }
        self
    }
    /// Required vertical overlap for partial rows; complete items always qualify.
    /// Invalid fractions retain the current policy. Excludes overscan/prediction.
    pub fn viewability(mut self, policy: ListViewability) -> Self {
        if policy.validate().is_ok() {
            self.0.wrap.as_mut().unwrap().viewability = policy;
        }
        self
    }
    /// Continuous qualifying time per data index (default zero). Dataset/count
    /// changes and hidden/offscreen suspension restart dwell. Reflow retains
    /// dwell for indices that still qualify in the new geometry.
    pub fn minimum_view_time(mut self, duration: Duration) -> Self {
        self.0.wrap.as_mut().unwrap().minimum_view_time = duration;
        self
    }
    /// Report sorted current/previous flat indices after layout when membership
    /// changes. Uses node-owned deadlines, with no idle wake/background timer.
    /// Callbacks run outside geometry borrows and may read the handle/update data.
    pub fn on_viewable_items_change(mut self, callback: impl Fn(ViewableItems) + 'static) -> Self {
        self.0.wrap.as_mut().unwrap().on_viewable_items_change = Some(Rc::new(callback));
        self
    }
    /// Request a page near the end, including an initially empty dataset. Fires
    /// once per approach, rearmed by count/data-key changes or moving away.
    /// Uses actual viewport geometry, excluding overscan and scroll prediction.
    /// The caller owns loading/exhaustion guards and may read the handle here.
    pub fn on_load_more(mut self, callback: impl Fn() + 'static) -> Self {
        self.0.wrap.as_mut().unwrap().on_load_more = Some(Rc::new(callback));
        self
    }
    /// Request preceding items near the start of nonempty data. Initial proximity
    /// can trigger; use a caller guard when history is exhausted.
    pub fn on_load_more_at_start(mut self, callback: impl Fn() + 'static) -> Self {
        self.0.wrap.as_mut().unwrap().on_load_more_at_start = Some(Rc::new(callback));
        self
    }
    /// Prefetch distance for both edges in viewport heights (default 1). Zero
    /// waits for the edge; invalid/negative values retain the previous setting.
    pub fn load_more_threshold(mut self, viewports: f64) -> Self {
        if viewports.is_finite() && viewports >= 0.0 {
            self.0.wrap.as_mut().unwrap().load_more_threshold = viewports;
        }
        self
    }
    /// Override the start prefetch distance independently of the end threshold.
    pub fn load_more_at_start_threshold(mut self, viewports: f64) -> Self {
        if viewports.is_finite() && viewports >= 0.0 {
            self.0.wrap.as_mut().unwrap().load_more_at_start_threshold = Some(viewports);
        }
        self
    }
    pub fn on_viewport_change(mut self, f: impl Fn(Option<GridViewport>) + 'static) -> Self {
        self.0.on_viewport_change = Some(Rc::new(f));
        self
    }
    pub fn scroll_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.0.scroll_behavior = behavior.into();
        self
    }
    pub fn scrollbar_visibility(mut self, visibility: impl Into<String>) -> Self {
        self.0.scrollbar_visibility = visibility.into();
        self
    }
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.0 = self.0.class(class);
        self
    }
}

impl From<VirtualWrapBuilder> for Element {
    fn from(builder: VirtualWrapBuilder) -> Self {
        builder.0.into()
    }
}

pub fn virtual_grid(id: &str) -> VirtualGridBuilder {
    VirtualGridBuilder {
        id: id.into(),
        handle: VirtualGridHandle::default(),
        rows: 0,
        columns: 0,
        row_count_fn: None,
        column_count_fn: None,
        size: [80.0, 32.0],
        size_fn: None,
        gap: [0.0; 2],
        overscan: [1; 2],
        scroll_ahead: false,
        use_parent_scroll: false,
        capacity: 16_384,
        height: 240.0,
        data_key_fn: None,
        axis_keys: [None, None],
        maintain_position: true,
        render_cell: Rc::new(|_, _| Element::new()),
        on_viewport_change: None,
        scroll_behavior: crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.into(),
        scrollbar_visibility: "auto".into(),
        chrome: SlotChrome::default(),
        wrap: None,
    }
}

impl VirtualGridBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn handle(mut self, handle: VirtualGridHandle) -> Self {
        self.handle = handle;
        self
    }
    pub fn row_count(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }
    pub fn column_count(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }
    pub fn row_count_fn(mut self, f: impl Fn() -> usize + 'static) -> Self {
        self.row_count_fn = Some(Rc::new(f));
        self
    }
    pub fn column_count_fn(mut self, f: impl Fn() -> usize + 'static) -> Self {
        self.column_count_fn = Some(Rc::new(f));
        self
    }
    pub fn cell_size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }
    pub fn cell_size_fn(mut self, f: impl Fn() -> [f32; 2] + 'static) -> Self {
        self.size_fn = Some(Rc::new(f));
        self
    }
    pub fn cell_gap(mut self, horizontal: f32, vertical: f32) -> Self {
        self.gap = [horizontal, vertical];
        self
    }
    pub fn overscan(mut self, columns: usize, rows: usize) -> Self {
        self.overscan = [columns, rows];
        self
    }
    /// Pre-render up to one viewport ahead on each axis. Prediction exceeding
    /// max_mounted_cells falls back to the normal window. Disabled by default.
    pub fn scroll_ahead(mut self, enabled: bool) -> Self {
        self.scroll_ahead = enabled;
        self
    }
    /// Borrow the nearest registered ancestor on each physical axis. Both
    /// axes must bind before mounting or navigation; no root fallback exists.
    /// The grid contributes its full natural extent instead of a local viewport.
    pub fn use_parent_scroll(mut self) -> Self {
        self.use_parent_scroll = true;
        self
    }
    /// Fail closed with an empty body and data-virtual-grid-error=Capacity when
    /// the requested window exceeds this budget. The viewport stays scrollable.
    /// Includes offscreen focus, selection and portal owners. The requested
    /// window has priority; owners use only the remaining capacity.
    pub fn max_mounted_cells(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        if height.is_finite() && height >= 0.0 {
            self.height = height;
        }
        self
    }
    /// Polled without subscribing the outer view. Change this key after edits or
    /// reorder to rebuild cell contents. Configured row/column keys retain wrappers
    /// across edits; axes without keys retain coordinate identity.
    pub fn data_key_fn(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.data_key_fn = Some(Rc::new(f));
        self
    }
    /// Stable unique row identities. Keys are scanned on row-count/data-key
    /// changes only. Duplicate keys clear the window until corrected and revised.
    /// Together with column_key_fn this costs O(rows + columns) storage; an
    /// unkeyed axis keeps coordinate identity. Render callbacks get current indices.
    pub fn row_key_fn(mut self, f: impl Fn(usize) -> String + 'static) -> Self {
        self.axis_keys[1] = Some(Rc::new(f));
        self
    }
    /// Stable unique column identities, with the same contract as row_key_fn.
    pub fn column_key_fn(mut self, f: impl Fn(usize) -> String + 'static) -> Self {
        self.axis_keys[0] = Some(Rc::new(f));
        self
    }
    /// Preserve each visible keyed axis's item and pixel depth across edits.
    /// A removed anchor uses its next old survivor, then its predecessor.
    /// Enabled by default; hidden and parent-scrolling grids keep owner offsets.
    /// Explicit handle navigation takes precedence over automatic restoration.
    pub fn maintain_visible_content_position(mut self, enabled: bool) -> Self {
        self.maintain_position = enabled;
        self
    }
    pub fn render_cell(mut self, f: impl Fn(usize, usize) -> Element + 'static) -> Self {
        self.render_cell = Rc::new(f);
        self
    }
    pub fn on_viewport_change(mut self, f: impl Fn(Option<GridViewport>) + 'static) -> Self {
        self.on_viewport_change = Some(Rc::new(f));
        self
    }
    pub fn scroll_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.scroll_behavior = behavior.into();
        self
    }
    pub fn scrollbar_visibility(mut self, visibility: impl Into<String>) -> Self {
        self.scrollbar_visibility = visibility.into();
        self
    }

    fn geometry(
        &self,
        width: f64,
        height: f64,
    ) -> Result<(UniformGrid, [f32; 2], Option<UniformWrap>), VirtualError> {
        let size = self.size_fn.as_ref().map_or(self.size, |f| f());
        let wrap = self
            .wrap
            .as_ref()
            .map(|items| {
                UniformWrap::new(
                    (items.count)(),
                    if width > 0.0 {
                        width
                    } else {
                        f64::from(items.minimum_width)
                    },
                    f64::from(items.minimum_width),
                    f64::from(size[1]),
                    self.gap.map(f64::from),
                )
                .and_then(|wrap| {
                    wrap.with_content(
                        items.slots.insets(wrap.len()),
                        if self.use_parent_scroll { 0.0 } else { height },
                        if self.use_parent_scroll {
                            ListContentAlign::Start
                        } else {
                            items.content_align
                        },
                    )
                })
            })
            .transpose()?;
        let grid = if let Some(wrap) = wrap {
            wrap.grid()
        } else {
            UniformGrid::new(
                self.row_count_fn.as_ref().map_or(self.rows, |f| f()),
                self.column_count_fn.as_ref().map_or(self.columns, |f| f()),
                size.map(f64::from),
                self.gap.map(f64::from),
            )?
        };
        if wrap
            .map_or(grid.total(), |w| w.total())
            .iter()
            .any(|v| *v > f64::from(f32::MAX))
        {
            return Err(VirtualError::InvalidExtent);
        }
        Ok((
            grid,
            wrap.map_or(size, |w| w.cell_size().map(|v| v as f32)),
            wrap,
        ))
    }
}

fn size_body(root: &mut Element, size: [f64; 2]) {
    *root = std::mem::take(root)
        .relative()
        .w(size[0] as f32)
        .min_w(size[0] as f32)
        .max_w(size[0] as f32)
        .h(size[1] as f32)
        .min_h(size[1] as f32)
        .max_h(size[1] as f32)
        .flex_shrink(0.0);
}

impl From<VirtualGridBuilder> for Element {
    fn from(builder: VirtualGridBuilder) -> Self {
        let id = builder.id.clone();
        let handle = builder.handle.clone();
        let owner = Rc::new(());
        *handle.0.binding.borrow_mut() = Some((
            crate::prelude::node(id.clone()),
            Rc::downgrade(&owner),
            Signal::new(()),
        ));
        handle.0.request.set(None);
        *handle.0.viewport.borrow_mut() = None;
        handle.0.grid.set(None);
        handle.0.wrap.set(None);
        let keys = ScrollViewportSignalKeys::new(&id);
        let x = use_signal(&keys.offset_x, || 0.0_f32);
        let y = use_signal(&keys.offset_y, || 0.0_f32);
        let tx = use_signal(&keys.target_x, || 0.0_f32);
        let ty = use_signal(&keys.target_y, || 0.0_f32);
        let width = use_signal(&keys.viewport_w, || 0.0_f32);
        let height = use_signal(&keys.viewport_h, || 0.0_f32);
        let mut body = Element::new().template_id(format!("{id}__grid_body"));
        size_body(
            &mut body,
            builder
                .geometry(0.0, 0.0)
                .map_or([0.0; 2], |(grid, _, wrap)| {
                    wrap.map_or(grid.total(), |w| w.total())
                }),
        );
        let previous = RefCell::new(None);
        let previous_data = RefCell::new(String::new());
        let previous_retained = RefCell::new(Vec::<(usize, usize)>::new());
        let previous_wrap = Cell::new(None::<UniformWrap>);
        let previous_load_edges = Cell::new(ListLoadEdges::default());
        let previous_end_geometry = Cell::new(None::<(f64, f64)>);
        let initial_end_settled = Cell::new(false);
        let previous_keys = RefCell::new(None::<Rc<[String]>>);
        let previous_axis_keys = RefCell::new([None::<Rc<GridAxisKeys>>, None]);
        let wrap_anchor = Cell::new(None::<ListScrollPosition>);
        let previous_offset = Cell::new(None::<f64>);
        let previous_geometry = Cell::new(None);
        let previous_parent = RefCell::new(None);
        let scroll_ahead = Cell::new([ListScrollAhead::default(); 2]);
        let chrome = builder.chrome.clone();
        let viewport_height = builder.height;
        let behavior = builder.scroll_behavior.clone();
        let visibility = builder.scrollbar_visibility.clone();
        let is_wrap = builder.wrap.is_some();
        let use_parent_scroll = builder.use_parent_scroll;
        let viewable_callback = builder
            .wrap
            .as_ref()
            .and_then(|wrap| wrap.on_viewable_items_change.clone());
        let viewable_state = viewable_callback.as_ref().map(|_| {
            Rc::new(RefCell::new((
                ListViewabilityTracker::new(builder.wrap.as_ref().unwrap().minimum_view_time),
                0..0,
            )))
        });
        let viewable_state_sync = viewable_state.clone();
        let body = super::virtual_list::stable_window_sync(body, move |root, layout, retry| {
            let _keep_owner = &owner;
            let read = |signal: Signal<f32>| {
                f64::from(
                    signal
                        .try_peek()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0)
                        .max(0.0),
                )
            };
            let owners: [Option<(String, f64)>; 2] = std::array::from_fn(|axis| {
                use_parent_scroll
                    .then(|| {
                        root.layout_node_id()
                            .and_then(|node| layout.ancestor_scroll_viewport(node, axis == 0))
                            .map(|(id, origin)| (id.to_owned(), origin))
                    })
                    .flatten()
            });
            let mut viewport = [read(width), read(height)];
            let mut parent = [ParentViewport::new(0.0, 0.0, 0.0).unwrap(); 2];
            if use_parent_scroll {
                for axis in 0..2 {
                    if let Some((id, origin)) = &owners[axis] {
                        let keys = ScrollViewportSignalKeys::new(id);
                        let offset_key = if axis == 0 {
                            keys.offset_x
                        } else {
                            keys.offset_y
                        };
                        let extent_key = if axis == 0 {
                            keys.viewport_w
                        } else {
                            keys.viewport_h
                        };
                        viewport[axis] = read(use_signal(&extent_key, || 0.0_f32));
                        parent[axis] = ParentViewport::new(
                            read(use_signal(&offset_key, || 0.0_f32)),
                            viewport[axis],
                            *origin,
                        )
                        .unwrap();
                    } else {
                        viewport[axis] = 0.0;
                    }
                }
                if is_wrap {
                    // The outer wrapper carries the authored width constraint;
                    // the body itself carries the computed natural grid width.
                    viewport[0] = root
                        .layout_node_id()
                        .filter(|_| owners[1].is_some())
                        .and_then(|node| layout.taffy.parent(node))
                        .and_then(|node| layout.taffy.layout(node).ok())
                        .map_or(0.0, |layout| {
                            f64::from(
                                (layout.size.width
                                    - layout.padding.left
                                    - layout.padding.right
                                    - layout.border.left
                                    - layout.border.right)
                                    .max(0.0),
                            )
                        });
                    parent[0] = ParentViewport::new(0.0, viewport[0], 0.0).unwrap();
                }
            }
            let mut offset = if use_parent_scroll {
                parent.map(|p| p.offset())
            } else {
                [read(x), read(y)]
            };
            let observed_y = offset[1];
            let geometry = builder.geometry(viewport[0], viewport[1]);
            let data = builder
                .data_key_fn
                .as_ref()
                .map_or_else(String::new, |f| f());
            let wrap_data_updated = *previous_data.borrow() != data
                || previous_wrap.get().map(|wrap| wrap.len())
                    != geometry
                        .as_ref()
                        .ok()
                        .and_then(|(_, _, wrap)| wrap.map(|w| w.len()));
            let mut navigation_changed = false;
            let mut navigation_requested = false;
            let extent = geometry.as_ref().map_or([0.0; 2], |(grid, _, wrap)| {
                wrap.map_or(grid.total(), |w| w.total())
            });
            let result = geometry.and_then(|(grid, size, wrap)| {
                let data_changed = *previous_data.borrow() != data;
                let old_axis_keys = previous_axis_keys.borrow().clone();
                let mut axis_keys = old_axis_keys.clone();
                let mut axes_changed = [false; 2];
                if wrap.is_none() {
                    for axis in 0..2 {
                        if let Some(key) = &builder.axis_keys[axis] {
                            let count = grid.axes()[axis].len();
                            if data_changed
                                || axis_keys[axis].as_ref().map(|k| k.keys.len()) != Some(count)
                            {
                                let next = Rc::new(GridAxisKeys::new(count, key.as_ref())?);
                                axes_changed[axis] = old_axis_keys[axis]
                                    .as_ref()
                                    .is_none_or(|old| old.keys != next.keys);
                                axis_keys[axis] = Some(next);
                            }
                        }
                    }
                    if builder.maintain_position
                        && !use_parent_scroll
                        && viewport.iter().all(|v| *v > 0.0)
                    {
                        if let Some(old) = handle.0.grid.get() {
                            for axis in 0..2 {
                                if !axes_changed[axis] {
                                    continue;
                                }
                                if let (Some(previous), Some(next)) =
                                    (&old_axis_keys[axis], &axis_keys[axis])
                                {
                                    let anchor = old.axes()[axis]
                                        .anchor(offset[axis])?
                                        .map(|p| {
                                            p.remap_keys(&previous.keys, |key| {
                                                next.indices.get(key).copied()
                                            })
                                        })
                                        .transpose()?
                                        .flatten();
                                    if let Some(anchor) = anchor {
                                        if let Ok(restored) = grid.axes()[axis]
                                            .restore_position(anchor, viewport[axis])
                                        {
                                            navigation_changed |= restored != offset[axis];
                                            offset[axis] = restored;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let grid_keyed = axis_keys.iter().any(Option::is_some);
                let old_keys = previous_keys.borrow().clone();
                let keys = builder.wrap.as_ref().and_then(|items| {
                    items.item_key.as_ref().map(|key| {
                        if data_changed
                            || old_keys.as_ref().map(|k| k.len()) != wrap.map(|w| w.len())
                        {
                            Rc::<[String]>::from(
                                (0..wrap.unwrap().len()).map(|i| key(i)).collect::<Vec<_>>(),
                            )
                        } else {
                            old_keys.clone().unwrap()
                        }
                    })
                });
                let keys_changed = match (&keys, &old_keys) {
                    (Some(next), Some(old)) => !Rc::ptr_eq(next, old) && next != old,
                    _ => keys.is_some() != old_keys.is_some(),
                };
                let indices: HashMap<_, _> = if keys_changed {
                    keys.as_deref()
                        .unwrap_or_default()
                        .iter()
                        .enumerate()
                        .map(|(i, key)| (key, i))
                        .collect()
                } else {
                    HashMap::new()
                };
                if keys_changed && indices.len() != keys.as_ref().map_or(0, |k| k.len()) {
                    return Err(VirtualError::DuplicateKey);
                }
                if let (Some(old), Some(next)) =
                    (previous_wrap.get().filter(|_| !use_parent_scroll), wrap)
                {
                    if previous_offset.get() != Some(offset[1]) || (data_changed && keys.is_none())
                    {
                        wrap_anchor.set(old.anchor(offset[1], viewport[1]).ok().flatten());
                    }
                    let maintain = builder.wrap.as_ref().unwrap().maintain_position;
                    let reflow =
                        old.columns() != next.columns() || old.cell_size() != next.cell_size();
                    if keys_changed {
                        wrap_anchor.set(if maintain {
                            wrap_anchor.get().and_then(|anchor| {
                                anchor
                                    .remap_keys(old_keys.as_deref().unwrap_or_default(), |key| {
                                        indices.get(key).copied()
                                    })
                                    .ok()
                                    .flatten()
                            })
                        } else if reflow {
                            old.anchor(offset[1], viewport[1])?
                        } else {
                            None
                        });
                    }
                    if reflow || (keys_changed && maintain) {
                        if let Some(anchor) = wrap_anchor.get() {
                            if let Ok(restored) = next.restore_anchor(anchor, viewport[1]) {
                                navigation_changed |= restored != offset[1];
                                offset[1] = restored;
                            }
                        }
                    }
                }
                let total = wrap.map_or(grid.total(), |w| w.total());
                // Compare the user's current offset with the last successful
                // visible geometry, before reflow/key restoration adjusts it.
                let end_follow = builder.wrap.as_ref().is_some_and(|items| {
                    !use_parent_scroll
                        && viewport.iter().all(|v| *v > 0.0)
                        && ((items.initial_scroll_at_end
                            && !initial_end_settled.get()
                            && wrap.is_some_and(|w| !w.is_empty()))
                            || (items.follow_end
                                && previous_end_geometry.get().is_some_and(
                                    |(old_total, old_viewport)| {
                                        ((old_total, old_viewport) != (total[1], viewport[1])
                                            || wrap_data_updated)
                                            && observed_y
                                                >= (f64::from(
                                                    (old_total - old_viewport).max(0.0) as f32
                                                ) - items.follow_end_threshold)
                                                    .max(0.0)
                                    },
                                )))
                });
                let clamped = if use_parent_scroll {
                    offset
                } else {
                    [
                        offset[0].min((total[0] - viewport[0]).max(0.0)),
                        offset[1].min((total[1] - viewport[1]).max(0.0)),
                    ]
                };
                navigation_changed |= clamped != offset;
                offset = clamped;
                if end_follow {
                    offset[1] = (total[1] - viewport[1]).max(0.0);
                    navigation_changed = true;
                    wrap_anchor.set(None);
                }
                if viewport.iter().all(|v| *v > 0.0) {
                    if let Some(request) = handle.0.request.take() {
                        navigation_requested = true;
                        let next = if use_parent_scroll {
                            let old_total = previous_geometry
                                .get()
                                .map_or(total, |(total, _, _, _)| total);
                            let parent_total = std::array::from_fn(|axis| {
                                let origin =
                                    owners[axis].as_ref().map_or(0.0, |(_, origin)| *origin);
                                let recorded =
                                    owners[axis].as_ref().map_or(total[axis], |(id, _)| {
                                        let key = if axis == 0 {
                                            scroll_content_w_signal_key(id)
                                        } else {
                                            scroll_content_h_signal_key(id)
                                        };
                                        read(use_signal(&key, || 0.0_f32))
                                    });
                                (recorded + total[axis] - old_total[axis])
                                    .max(origin + total[axis])
                                    .max(viewport[axis])
                            });
                            let item_target = |index, align, extra| {
                                wrap.ok_or(VirtualError::UnknownKey)
                                    .and_then(|wrap| {
                                        wrap.scroll_to_item_in_parent(
                                            index,
                                            parent[1],
                                            parent_total[1],
                                            align,
                                            extra,
                                        )
                                    })
                                    .map(|y| [0.0, y])
                            };
                            match request {
                                Navigation::End => {
                                    let target = |axis: usize| {
                                        parent[axis].scroll_to(
                                            total[axis],
                                            0.0,
                                            parent_total[axis],
                                            ListScrollAlign::End,
                                            0.0,
                                        )
                                    };
                                    if wrap.is_some() {
                                        target(1).map(|y| [0.0, y])
                                    } else {
                                        target(0).and_then(|x| target(1).map(|y| [x, y]))
                                    }
                                }
                                Navigation::Cell {
                                    row,
                                    column,
                                    align,
                                    extra,
                                } => {
                                    if let Some(wrap) = wrap {
                                        wrap.index(row, column)
                                            .ok_or(VirtualError::UnknownKey)
                                            .and_then(|index| {
                                                item_target(index, align[1], extra[1])
                                            })
                                    } else {
                                        grid.scroll_to_cell_in_parent(
                                            row,
                                            column,
                                            parent,
                                            parent_total,
                                            align,
                                            extra,
                                        )
                                    }
                                }
                                Navigation::Item {
                                    index,
                                    align,
                                    extra,
                                } => item_target(index, align, extra),
                            }
                        } else {
                            match request {
                                Navigation::End => Ok([
                                    (total[0] - viewport[0]).max(0.0),
                                    (total[1] - viewport[1]).max(0.0),
                                ]),
                                Navigation::Cell {
                                    row,
                                    column,
                                    align,
                                    extra,
                                } => {
                                    if let Some(wrap) = wrap {
                                        wrap.index(row, column)
                                            .ok_or(VirtualError::UnknownKey)
                                            .and_then(|index| {
                                                wrap.scroll_to_item(
                                                    index,
                                                    clamped[1],
                                                    viewport[1],
                                                    align[1],
                                                    extra[1],
                                                )
                                            })
                                            .map(|y| [0.0, y])
                                    } else {
                                        grid.scroll_to_cell(
                                            row, column, clamped, viewport, align, extra,
                                        )
                                    }
                                }
                                Navigation::Item {
                                    index,
                                    align,
                                    extra,
                                } => wrap
                                    .ok_or(VirtualError::UnknownKey)
                                    .and_then(|w| {
                                        w.scroll_to_item(
                                            index,
                                            clamped[1],
                                            viewport[1],
                                            align,
                                            extra,
                                        )
                                    })
                                    .map(|y| [0.0, y]),
                            }
                        };
                        if let Ok(next) = next {
                            if use_parent_scroll {
                                for axis in 0..2 {
                                    let origin =
                                        owners[axis].as_ref().map_or(0.0, |(_, origin)| *origin);
                                    parent[axis] = ParentViewport::new(
                                        f64::from(next[axis] as f32),
                                        viewport[axis],
                                        origin,
                                    )
                                    .unwrap();
                                }
                                offset = parent.map(|p| p.offset());
                            } else {
                                offset = next;
                            }
                            navigation_changed = true;
                            wrap_anchor.set(None);
                        }
                    }
                }
                // Scroll signals use f32; keep the same coordinates when reading
                // them on the next frame, including fractional anchor depths.
                if !use_parent_scroll {
                    offset = offset.map(|value| f64::from(value as f32));
                }
                if wrap.is_some() {
                    offset[0] = 0.0;
                }
                if let Some(wrap) = wrap.filter(|_| !use_parent_scroll) {
                    let current = wrap.anchor(offset[1], viewport[1])?;
                    let keep = wrap_anchor.get().filter(|anchor| {
                        current.is_some_and(|p| {
                            anchor.index / wrap.columns() == p.index / wrap.columns()
                                && (anchor.offset.min(size[1] as f64) - p.offset).abs()
                                    <= f64::from(f32::EPSILON) * offset[1].abs().max(1.0)
                        })
                    });
                    wrap_anchor.set(keep.or(current));
                }
                previous_wrap.set(wrap);
                *previous_keys.borrow_mut() = keys.clone();
                previous_offset.set(Some(offset[1]));
                let geometry_key = (total, size, wrap.map(|w| (w.len(), w.columns())), viewport);
                let mut prediction = scroll_ahead.get();
                let parent_changed =
                    previous_parent.replace(Some(owners.clone())) != Some(owners.clone());
                if previous_geometry.replace(Some(geometry_key)) != Some(geometry_key)
                    || parent_changed
                    || *previous_data.borrow() != data
                    || navigation_changed
                    || navigation_requested
                    || viewport.contains(&0.0)
                {
                    prediction = [ListScrollAhead::default(); 2];
                }
                let ahead = if builder.scroll_ahead {
                    let observed = if use_parent_scroll {
                        parent.map(|p| p.parent_offset())
                    } else {
                        offset
                    };
                    [
                        prediction[0].update(observed[0], viewport[0])?,
                        prediction[1].update(observed[1], viewport[1])?,
                    ]
                } else {
                    [0.0; 2]
                };
                scroll_ahead.set(prediction);
                let window = if use_parent_scroll {
                    if let Some(wrap) = wrap {
                        wrap.window_in_parent_with_scroll_ahead(
                            if viewport[0] > 0.0 {
                                parent[1]
                            } else {
                                ParentViewport::new(
                                    parent[1].parent_offset(),
                                    0.0,
                                    owners[1].as_ref().map_or(0.0, |(_, origin)| *origin),
                                )
                                .unwrap()
                            },
                            builder.overscan[1],
                            builder.capacity,
                            ahead[1],
                        )?
                    } else {
                        grid.window_in_parent_with_scroll_ahead(
                            parent,
                            builder.overscan,
                            builder.capacity,
                            ahead,
                        )?
                    }
                } else if let Some(wrap) = wrap {
                    wrap.window_with_scroll_ahead(
                        offset[1],
                        if viewport[0] > 0.0 { viewport[1] } else { 0.0 },
                        builder.overscan[1],
                        builder.capacity,
                        ahead[1],
                    )?
                } else {
                    grid.window_with_scroll_ahead(
                        offset,
                        viewport,
                        builder.overscan,
                        builder.capacity,
                        ahead,
                    )?
                };
                // Inspect only mounted cell subtrees; retaining an owner never
                // reveals it or changes the viewport report. Business keys can
                // move across a reflow; coordinate datasets cannot cross revisions.
                let focused = use_signal("__jadix_button_focus_id", String::new).peek();
                let mut retained = Vec::new();
                if viewport.iter().all(|v| *v > 0.0)
                    && (!data_changed || keys.is_some() || grid_keyed)
                {
                    for cell in &root.children {
                        let Some(key) = cell.attrs.get(CELL_ATTR) else {
                            continue;
                        };
                        if !subtree_needs_virtual_retention(cell, &focused) {
                            continue;
                        }
                        let position = if let Some(wrap) = wrap {
                            let item = if let Some(keys) = &keys {
                                key.strip_prefix("key:").and_then(|key| {
                                    cell.attrs
                                        .get(INDEX_ATTR)
                                        .and_then(|i| i.parse::<usize>().ok())
                                        .filter(|i| {
                                            keys.get(*i).is_some_and(|current| current == key)
                                        })
                                        .or_else(|| indices.get(&key.to_string()).copied())
                                })
                            } else {
                                key.strip_prefix("item:")
                                    .and_then(|i| i.parse::<usize>().ok())
                            };
                            item.filter(|i| *i < wrap.len())
                                .map(|i| (i / wrap.columns(), i % wrap.columns()))
                        } else if grid_keyed {
                            cell.attrs
                                .get(INDEX_ATTR)
                                .and_then(|i| i.split_once(':'))
                                .and_then(|(r, c)| {
                                    Some([c.parse::<usize>().ok()?, r.parse::<usize>().ok()?])
                                })
                                .and_then(|old| {
                                    let mut next = old;
                                    for axis in 0..2 {
                                        if let Some(keys) = &axis_keys[axis] {
                                            let key = old_axis_keys[axis]
                                                .as_ref()?
                                                .keys
                                                .get(old[axis])?;
                                            next[axis] = *keys.indices.get(key)?;
                                        }
                                    }
                                    grid.cell_origin(next[1], next[0])
                                        .map(|_| (next[1], next[0]))
                                })
                        } else {
                            key.split_once(':')
                                .and_then(|(r, c)| Some((r.parse().ok()?, c.parse().ok()?)))
                                .filter(|(r, c)| grid.cell_origin(*r, *c).is_some())
                        };
                        if let Some((row, column)) = position {
                            if !window.rows.contains(&row) || !window.columns.contains(&column) {
                                retained.push((row, column));
                            }
                        }
                    }
                }
                retained.sort_unstable();
                retained.dedup();
                let window_cells = window
                    .rows
                    .clone()
                    .map(|row| {
                        window
                            .columns
                            .clone()
                            .filter(|column| wrap.is_none_or(|w| w.index(row, *column).is_some()))
                            .count()
                    })
                    .sum::<usize>();
                // Visible/prefetched cells have priority over offscreen owners.
                retained.truncate(builder.capacity.saturating_sub(window_cells));
                Ok((grid, size, wrap, window, keys, axis_keys, retained))
            });
            if navigation_changed && result.is_ok() {
                if use_parent_scroll {
                    for axis in 0..2 {
                        if is_wrap && axis == 0 {
                            continue;
                        }
                        if let Some((id, _)) = &owners[axis] {
                            let keys = ScrollViewportSignalKeys::new(id);
                            let (offset_key, target_key) = if axis == 0 {
                                cancel_scroll_end_x(id);
                                (keys.offset_x, keys.target_x)
                            } else {
                                cancel_scroll_end_y(id);
                                (keys.offset_y, keys.target_y)
                            };
                            use_signal(&offset_key, || 0.0_f32)
                                .set_silent(parent[axis].parent_offset() as f32);
                            use_signal(&target_key, || 0.0_f32)
                                .set_silent(parent[axis].parent_offset() as f32);
                        }
                    }
                } else {
                    x.set_silent(offset[0] as f32);
                    y.set_silent(offset[1] as f32);
                    tx.set_silent(offset[0] as f32);
                    ty.set_silent(offset[1] as f32);
                    cancel_scroll_end_x(&builder.id);
                    cancel_scroll_end_y(&builder.id);
                }
            }
            let mut status = LayoutStatus::Clean;
            let current_viewport = match result {
                Ok((grid, size, wrap, window, keys, axis_keys, retained)) => {
                    let snapshot_is_current = || {
                        builder
                            .data_key_fn
                            .as_ref()
                            .map_or_else(String::new, |f| f())
                            == data
                            && builder.geometry(viewport[0], viewport[1]).ok().is_some_and(
                                |(live_grid, live_size, live_wrap)| {
                                    live_size == size
                                        && live_grid.axes().map(|axis| axis.len())
                                            == grid.axes().map(|axis| axis.len())
                                        && live_wrap.map(|w| (w.len(), w.total()))
                                            == wrap.map(|w| (w.len(), w.total()))
                                },
                            )
                    };
                    let total = wrap.map_or(grid.total(), |w| w.total());
                    if wrap.is_some() && viewport.iter().all(|v| *v > 0.0) {
                        previous_end_geometry.set(Some((total[1], viewport[1])));
                        if wrap.is_some_and(|w| !w.is_empty()) {
                            initial_end_settled.set(true);
                        }
                    }
                    handle.0.grid.set(Some(grid));
                    handle.0.wrap.set(wrap);
                    let key = (
                        window.columns.clone(),
                        window.rows.clone(),
                        total,
                        size,
                        wrap.map(|w| (w.len(), w.columns())),
                        viewport.iter().all(|v| *v > 0.0),
                    );
                    let data_changed = *previous_data.borrow() != data;
                    let grid_keyed = axis_keys.iter().any(Option::is_some);
                    if previous.borrow().as_ref() != Some(&key)
                        || data_changed
                        || *previous_retained.borrow() != retained
                    {
                        let old = std::mem::take(&mut root.children);
                        let mut cells: HashMap<String, Element> = old
                            .into_iter()
                            .filter_map(|cell| match &builder.wrap {
                                Some(wrap) => wrap.slots.park(cell),
                                None => Some(cell),
                            })
                            .filter_map(|cell| {
                                if data_changed && keys.is_none() && !grid_keyed {
                                    None
                                } else {
                                    cell.attrs.get(CELL_ATTR).cloned().map(|key| (key, cell))
                                }
                            })
                            .collect();
                        if viewport.iter().all(|v| *v > 0.0) {
                            if let (Some(items), Some(wrap)) = (&builder.wrap, wrap) {
                                items.slots.mount(root, wrap);
                            }
                        }
                        let positions = window
                            .rows
                            .clone()
                            .flat_map(|row| window.columns.clone().map(move |column| (row, column)))
                            .chain(retained.iter().copied());
                        let mut snapshot_valid = snapshot_is_current();
                        for (row, column) in positions {
                            if !snapshot_valid {
                                break;
                            }
                            let item = wrap.and_then(|w| w.index(row, column));
                            if wrap.is_some() && item.is_none() {
                                continue;
                            }
                            let stable_key = item.and_then(|i| keys.as_ref().map(|keys| &keys[i]));
                            let key = stable_key.map_or_else(
                                || {
                                    item.map_or_else(
                                        || format!("{row}:{column}"),
                                        |i| format!("item:{i}"),
                                    )
                                },
                                |key| format!("key:{key}"),
                            );
                            let key = if grid_keyed {
                                grid_cell_key(&axis_keys, row, column)
                            } else {
                                key
                            };
                            let index = if grid_keyed {
                                Some(format!("{row}:{column}"))
                            } else {
                                item.map(|i| i.to_string())
                            };
                            let mut cell = cells.remove(&key).unwrap_or_else(|| {
                                Element::new().template_id(if grid_keyed {
                                    format!(
                                        "{}__grid_key_{}",
                                        builder.id,
                                        key.as_bytes()
                                            .iter()
                                            .map(|b| format!("{b:02x}"))
                                            .collect::<String>()
                                    )
                                } else {
                                    stable_key.map_or_else(
                                        || {
                                            item.map_or_else(
                                                || format!("{}__cell_{row}_{column}", builder.id),
                                                |i| format!("{}__item_{i}", builder.id),
                                            )
                                        },
                                        |key| {
                                            format!(
                                                "{}__key_{}",
                                                builder.id,
                                                key.as_bytes()
                                                    .iter()
                                                    .map(|b| format!("{b:02x}"))
                                                    .collect::<String>()
                                            )
                                        },
                                    )
                                })
                            });
                            if cell.children.is_empty()
                                || ((keys.is_some() || grid_keyed)
                                    && (data_changed
                                        || cell.attrs.get(INDEX_ATTR) != index.as_ref()))
                            {
                                let child = match (&builder.wrap, item) {
                                    (Some(items), Some(i)) => (items.render_item)(i),
                                    _ => (builder.render_cell)(row, column),
                                };
                                snapshot_valid = snapshot_is_current();
                                if !snapshot_valid {
                                    break;
                                }
                                cell.children = vec![child];
                            }
                            if let Some(index) = index {
                                cell.attrs.insert(INDEX_ATTR.into(), index);
                            }
                            let origin = wrap
                                .map_or_else(
                                    || grid.cell_origin(row, column),
                                    |wrap| wrap.item_origin(item.unwrap()),
                                )
                                .unwrap();
                            cell = cell
                                .absolute()
                                .left(origin[0] as f32)
                                .top(origin[1] as f32)
                                .w(size[0])
                                .min_w(size[0])
                                .max_w(size[0])
                                .h(size[1])
                                .min_h(size[1])
                                .max_h(size[1])
                                .flex_shrink(0.0)
                                .overflow_x("hidden")
                                .overflow_y("hidden");
                            cell.attrs.insert(CELL_ATTR.into(), key);
                            root.children.push(cell);
                        }
                        *previous_axis_keys.borrow_mut() = axis_keys;
                        *previous_retained.borrow_mut() = retained;
                        size_body(root, total);
                        *previous.borrow_mut() = Some(key);
                        *previous_data.borrow_mut() = data.clone();
                        status = LayoutStatus::StructureChanged;
                    }
                    if root.attrs.remove(ERROR_ATTR).is_some() {
                        status = status.merge(LayoutStatus::StyleChanged);
                    }
                    if !snapshot_is_current() {
                        *previous.borrow_mut() = None;
                        retry.set(true);
                        return status;
                    }
                    Some(window.viewport)
                }
                Err(error) => {
                    previous_geometry.set(None);
                    previous_retained.borrow_mut().clear();
                    scroll_ahead.set([ListScrollAhead::default(); 2]);
                    handle.0.grid.set(None);
                    handle.0.wrap.set(None);
                    let message = format!("{error:?}");
                    let old_size = root.style.size;
                    size_body(root, extent);
                    if root.style.size != old_size {
                        status = LayoutStatus::LayoutChanged;
                    }
                    if root.attrs.get(ERROR_ATTR) != Some(&message) || !root.children.is_empty() {
                        root.attrs.insert(ERROR_ATTR.into(), message);
                        for child in std::mem::take(&mut root.children) {
                            if let Some(wrap) = &builder.wrap {
                                wrap.slots.park(child);
                            }
                        }
                        // Capacity failure retains valid scroll extents so users
                        // can narrow the visible window and recover.
                        // The geometry was read once above, including dynamic callbacks.
                        *previous.borrow_mut() = None;
                        status = LayoutStatus::StructureChanged;
                    }
                    None
                }
            };
            if navigation_changed {
                status = status.merge(LayoutStatus::LayoutChanged);
            }
            let changed = *handle.0.viewport.borrow() != current_viewport;
            *handle.0.viewport.borrow_mut() = current_viewport.clone();
            if let Some(state) = &viewable_state_sync {
                let mut state = state.borrow_mut();
                if wrap_data_updated || current_viewport.is_none() {
                    state.0.restart();
                }
                state.1 = handle
                    .0
                    .wrap
                    .get()
                    .filter(|_| viewport.iter().all(|v| *v > 0.0))
                    .and_then(|wrap| {
                        let policy = builder.wrap.as_ref().unwrap().viewability;
                        if use_parent_scroll {
                            wrap.viewable_items_in_parent(parent[1], policy)
                        } else {
                            wrap.viewable_items(
                                offset[1],
                                viewport[1],
                                builder.wrap.as_ref().unwrap().viewability,
                            )
                        }
                        .ok()
                    })
                    .unwrap_or(0..0);
            }
            let near = builder
                .wrap
                .as_ref()
                .and_then(|items| {
                    handle
                        .0
                        .wrap
                        .get()
                        .filter(|_| current_viewport.is_some() && viewport.iter().all(|v| *v > 0.0))
                        .and_then(|wrap| {
                            let thresholds = [
                                items
                                    .load_more_at_start_threshold
                                    .unwrap_or(items.load_more_threshold),
                                items.load_more_threshold,
                            ];
                            if use_parent_scroll {
                                parent[1].load_edges(
                                    wrap.total()[1],
                                    thresholds,
                                    !wrap.is_empty(),
                                    ListOrder::Forward,
                                )
                            } else {
                                ListLoadEdges::new(
                                    offset[1],
                                    viewport[1],
                                    wrap.total()[1],
                                    [
                                        items
                                            .load_more_at_start_threshold
                                            .unwrap_or(items.load_more_threshold),
                                        items.load_more_threshold,
                                    ],
                                    !wrap.is_empty(),
                                    ListOrder::Forward,
                                )
                            }
                            .ok()
                        })
                })
                .unwrap_or_default();
            let load = near.entered(previous_load_edges.replace(near), wrap_data_updated);
            if changed {
                if let Some(callback) = &builder.on_viewport_change {
                    callback(current_viewport);
                }
            }
            if let Some(items) = &builder.wrap {
                if load.start {
                    if let Some(callback) = &items.on_load_more_at_start {
                        callback();
                    }
                }
                if load.end {
                    if let Some(callback) = &items.on_load_more {
                        callback();
                    }
                }
            }
            status
        });
        let mut element = chrome.apply(if use_parent_scroll {
            Element::new()
                .template_id(&id)
                .w("100%")
                .flex_shrink(0.0)
                .child(body)
        } else {
            scroll_area(&id)
                .overflow_x(if is_wrap { "hidden" } else { "auto" })
                .overflow_y("auto")
                .scroll_behavior(behavior)
                .scrollbar_visibility(visibility)
                .w("100%")
                .h(viewport_height)
                .child(body)
                .into()
        });
        if let (Some(state), Some(callback)) = (viewable_state, viewable_callback) {
            let origin = web_time::Instant::now();
            crate::authoring::deadline::attach(
                &mut element,
                crate::authoring::deadline::Visibility::Visible,
                move |context| {
                    let (change, next_deadline) = {
                        let mut state = state.borrow_mut();
                        if context.resumed {
                            state.0.restart();
                        }
                        let candidates = state.1.clone();
                        let change = state
                            .0
                            .update(candidates, context.now.saturating_duration_since(origin));
                        (change, state.0.next_deadline())
                    };
                    if let Some(change) = change {
                        callback(change);
                    }
                    crate::authoring::deadline::Poll {
                        redraw: false,
                        next_wake: next_deadline.and_then(|deadline| origin.checked_add(deadline)),
                    }
                },
            );
        }
        element
    }
}
