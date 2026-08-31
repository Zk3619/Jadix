//! # Table
//!
//! 对齐 **solid-ui / shadcn** 复合表壳 + **TanStack Table** 的 columns / data 心智。
//!
//! ```text
//! Table                         // overflow 外壳 + w-full text-sm
//! ├── TableCaption              // 可选；muted-foreground
//! ├── TableHeader               // 表头区；行带 border-b
//! │   └── TableRow → TableHead
//! ├── TableBody                 // 行区
//! │   └── TableRow → TableCell  // hover muted/50；selected → muted
//! └── TableFooter               // solid-ui：bg-primary / primary-foreground
//! ```
//!
//! ## 数据表预设（TanStack 风格）
//!
//! | 入口 | 说明 |
//! |------|------|
//! | [`table`] / [`data_table`] | 默认密度；VirtualList 行体；排序 / 单行选中 / 可选分页与全局筛选 |
//! | [`table_compact`] | 紧凑行高（`py` 更矮） |
//! | [`table_comfortable`] | 舒适行高 |
//!
//! 预设内部用复合零件的视觉契约（边框、表头 muted-foreground、选中 muted），
//! 不承诺分组 / 列 pinning / 多列排序（见 `docs/component_inventory.md`）。
//!
//! ## Usage
//! ```ignore
//! // 复合 RSX（静态小表）
//! ui! {
//!     <Table>
//!         <TableCaption>"Recent invoices"</TableCaption>
//!         <TableHeader>
//!             <TableRow>
//!                 <TableHead w="100">"Invoice"</TableHead>
//!                 <TableHead>"Status"</TableHead>
//!                 <TableHead align="end">"Amount"</TableHead>
//!             </TableRow>
//!         </TableHeader>
//!         <TableBody>
//!             <TableRow>
//!                 <TableCell>"INV001"</TableCell>
//!                 <TableCell>"Paid"</TableCell>
//!                 <TableCell align="end">"$250.00"</TableCell>
//!             </TableRow>
//!         </TableBody>
//!     </Table>
//! }
//!
//! // 数据表预设
//! Element::from(
//!     data_table("people")
//!         .columns([
//!             TableColumn::new("name", "Name").width(120.0),
//!             TableColumn::new("role", "Role").width(100.0),
//!         ])
//!         .row_count(40)
//!         .cell(|r, c| format!("{r}:{c}"))
//!         .selected_signal(sel),
//! )
//! ```

use std::rc::Rc;

use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{ButtonHoverPaint, Element, Handler};
use crate::primitives::interactive::mark_hover_paint_dirty;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::take_packed;
use rui_macros::component;

const ATTR_TABLE_SLOT: &str = "rui_table_slot";
const SLOT_CAPTION: &str = "caption";
const SLOT_HEADER: &str = "header";
const SLOT_BODY: &str = "body";
const SLOT_FOOTER: &str = "footer";

const TEXT_SM: f32 = 14.0;
const HEAD_H_DEFAULT: f32 = 40.0; // h-10
const HEAD_H_COMPACT: f32 = 32.0;
const HEAD_H_COMFORTABLE: f32 = 48.0;
const ROW_H_DEFAULT: f32 = 40.0;
const ROW_H_COMPACT: f32 = 32.0;
const ROW_H_COMFORTABLE: f32 = 48.0;
fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_TABLE_SLOT).map(|s| s.as_str())
}

fn mark_slot(mut el: Element, slot: &str) -> Element {
    el.set_attr(ATTR_TABLE_SLOT, slot);
    el
}

fn apply_cell_align(mut el: Element, align: TableAlign) -> Element {
    el = match align {
        TableAlign::Start => el.justify_start().text_left(),
        TableAlign::Center => el.justify_center().text_center(),
        TableAlign::End => el.justify_end().text_right(),
    };
    el
}

fn style_textish(el: &mut Element, size: f32, weight: f32, color: [u8; 4]) {
    if el.text.is_some() {
        el.text_style.size = size;
        el.text_style.weight = weight as u16;
        el.text_style.color = color;
    }
    for c in &mut el.children {
        style_textish(c, size, weight, color);
    }
}

fn parse_align(raw: &str) -> TableAlign {
    TableAlign::parse(raw)
}

// ── Align / Density ──────────────────────────────────────────────────────────

/// 单元格水平对齐（对齐 Tailwind `text-left` / `text-center` / `text-right`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TableAlign {
    #[default]
    Start,
    Center,
    End,
}

impl TableAlign {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "center" | "middle" => Self::Center,
            "end" | "right" => Self::End,
            _ => Self::Start,
        }
    }
}

/// 数据表行高密度预设。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TableDensity {
    /// solid-ui 默认：`h-10` / `p-2` 量级。
    #[default]
    Default,
    /// 更矮的行，适合密集数据。
    Compact,
    /// 更松的行，适合触控 / 演示。
    Comfortable,
}

impl TableDensity {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "compact" | "sm" => Self::Compact,
            "comfortable" | "lg" => Self::Comfortable,
            _ => Self::Default,
        }
    }

    fn header_h(self) -> f32 {
        match self {
            Self::Default => HEAD_H_DEFAULT,
            Self::Compact => HEAD_H_COMPACT,
            Self::Comfortable => HEAD_H_COMFORTABLE,
        }
    }

    fn row_h(self) -> f32 {
        match self {
            Self::Default => ROW_H_DEFAULT,
            Self::Compact => ROW_H_COMPACT,
            Self::Comfortable => ROW_H_COMFORTABLE,
        }
    }
}

// ── Composite（solid-ui / shadcn）────────────────────────────────────────────

/// 表根：`relative w-full overflow-auto` 外壳 + 内层 `w-full text-sm`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Table(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut caption = Vec::new();
    let mut header = Vec::new();
    let mut body = Vec::new();
    let mut footer = Vec::new();
    let mut rest = Vec::new();
    for k in kids {
        match slot_of(&k) {
            Some(SLOT_CAPTION) => caption.push(k),
            Some(SLOT_HEADER) => header.push(k),
            Some(SLOT_BODY) => body.push(k),
            Some(SLOT_FOOTER) => footer.push(k),
            _ => rest.push(k),
        }
    }

    // HTML caption 可在上/下；solid-ui demo 放顶部。其余按 Header → Body → Footer → 未标记。
    let mut ordered = Vec::new();
    ordered.append(&mut caption);
    ordered.append(&mut header);
    ordered.append(&mut body);
    ordered.append(&mut footer);
    ordered.append(&mut rest);

    let inner: Element = vstack().w("100%").min_w(0.0).children(ordered).into();
    let mut wrap: Element = r#box()
        .w("100%")
        .min_w(0.0)
        .overflow("auto")
        .child(inner)
        .into();
    wrap.set_attr("rui_table", "1");
    wrap.set_attr("data-slot", "table");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        wrap.set_attr("class", c);
    }
    wrap
}

/// `thead`：子行带底边（由 [`TableRow`] 绘制）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    mark_slot(
        vstack().w("100%").min_w(0.0).children(kids).into(),
        SLOT_HEADER,
    )
}

/// `tbody`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableBody(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    mark_slot(
        vstack().w("100%").min_w(0.0).children(kids).into(),
        SLOT_BODY,
    )
}

/// `tfoot`：solid-ui `bg-primary font-medium text-primary-foreground`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let fg = tokens::primary_foreground_text();
    let mut styled = kids;
    for k in &mut styled {
        style_textish(k, TEXT_SM, 500.0, fg);
    }
    mark_slot(
        hstack()
            .w("100%")
            .min_w(0.0)
            .items_center()
            .bg(tokens::primary())
            .children(styled)
            .into(),
        SLOT_FOOTER,
    )
}

/// `tr`：`border-b`；选中 `bg-muted`；可选 hover（ghost 近似 `muted/50`）。
///
/// `id` 可选；空则自动分配，供 hover 信号键使用。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableRow(
    id: String,
    selected: bool,
    mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .border_style("solid")
        .border_bottom(tokens::border(), 1.0)
        .children(kids)
        .into();

    if selected {
        row = row.bg(tokens::muted());
        row.set_attr("data-state", "selected");
    } else {
        row = row.bg([0.0, 0.0, 0.0, 0.0]);
    }

    let row_id = if id.trim().is_empty() {
        format!("rui-tr-{}", row_uid())
    } else {
        id
    };

    // hover：复用 Button ghost 管线（accent ≈ muted 主题下接近 muted/50）。
    let hover_key = format!("{row_id}_hover");
    let hovered = use_signal(hover_key.as_str(), || false);
    let _ = hovered.peek();
    row.focus_id = Some(row_id.clone());
    row.template_id = Some(row_id);
    if !selected {
        row.button_hover_paint = Some(ButtonHoverPaint {
            variant: "ghost".into(),
            disabled: false,
            hover_easing: "spring".into(),
            text_color: None,
        });
        row = row.on_hover(move |h| {
            hovered.set_silent(h);
            mark_hover_paint_dirty();
        });
    }

    row = row.on_click(move || on_click.call(()));
    row.set_attr("data-slot", "table-row");
    row
}

fn row_uid() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// `th`：`h-10 px-2`、`font-medium text-muted-foreground`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableHead(w: Option<String>, align: Option<String>, children: Option<Element>) -> Element {
    let align = align
        .as_deref()
        .map(parse_align)
        .unwrap_or(TableAlign::Start);
    let muted = parse_text_color("muted-foreground");
    let mut kids = children.map(take_packed).unwrap_or_default();
    if kids.is_empty() {
        kids.push(Element::from(
            text("").size(TEXT_SM).weight(500.0).color(muted),
        ));
    } else {
        for k in &mut kids {
            style_textish(k, TEXT_SM, 500.0, muted);
        }
    }

    let mut cell: Element = hstack()
        .h(HEAD_H_DEFAULT)
        .min_h(HEAD_H_DEFAULT)
        .px(tokens::SPACE_2)
        .items_center()
        .min_w(0.0)
        .children(kids)
        .into();
    if let Some(width) = w.filter(|s| !s.trim().is_empty()) {
        cell = cell.w(width.as_str());
    } else {
        cell = cell.flex_grow(1.0);
    }
    cell = apply_cell_align(cell, align);
    cell.set_attr("data-slot", "table-head");
    cell
}

/// `td`：`p-2 align-middle`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableCell(w: Option<String>, align: Option<String>, children: Option<Element>) -> Element {
    let align = align
        .as_deref()
        .map(parse_align)
        .unwrap_or(TableAlign::Start);
    let fg = parse_text_color("foreground");
    let mut kids = children.map(take_packed).unwrap_or_default();
    if kids.is_empty() {
        kids.push(Element::from(
            text("").size(TEXT_SM).weight(400.0).color(fg),
        ));
    } else {
        for k in &mut kids {
            style_textish(k, TEXT_SM, 400.0, fg);
        }
    }

    let mut cell: Element = hstack()
        .p(tokens::SPACE_2)
        .items_center()
        .min_w(0.0)
        .children(kids)
        .into();
    if let Some(width) = w.filter(|s| !s.trim().is_empty()) {
        cell = cell.w(width.as_str());
    } else {
        cell = cell.flex_grow(1.0);
    }
    cell = apply_cell_align(cell, align);
    cell.set_attr("data-slot", "table-cell");
    cell
}

/// `caption`：`mt-4 text-sm text-muted-foreground`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableCaption(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    let mut kids = children.map(take_packed).unwrap_or_default();
    for k in &mut kids {
        style_textish(k, TEXT_SM, 400.0, muted);
    }
    if kids.is_empty() {
        kids.push(Element::from(
            text("").size(TEXT_SM).weight(400.0).color(muted),
        ));
    }
    mark_slot(
        r#box().w("100%").mt(tokens::SPACE_4).children(kids).into(),
        SLOT_CAPTION,
    )
}

// ── Column / state（TanStack 对齐）───────────────────────────────────────────

#[derive(Clone)]
pub struct TableColumn {
    pub id: String,
    pub header: String,
    pub width: f32,
    pub align: TableAlign,
    pub sortable: bool,
}

impl TableColumn {
    pub fn new(id: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            header: header.into(),
            width: 100.0,
            align: TableAlign::Start,
            sortable: true,
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(1.0);
        self
    }

    pub fn align(mut self, a: impl AsRef<str>) -> Self {
        self.align = TableAlign::parse(a.as_ref());
        self
    }

    pub fn sortable(mut self, yes: bool) -> Self {
        self.sortable = yes;
        self
    }
}

pub use crate::utils::table_sort::{TableSortDir, TableSortState};

/// 客户端分页状态（对齐 TanStack `PaginationState`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TablePaginationState {
    pub page_index: usize,
    pub page_size: usize,
}

impl Default for TablePaginationState {
    fn default() -> Self {
        Self {
            page_index: 0,
            page_size: 10,
        }
    }
}

impl TablePaginationState {
    pub fn new(page_index: usize, page_size: usize) -> Self {
        Self {
            page_index,
            page_size: page_size.max(1),
        }
    }

    pub fn page_count(self, row_count: usize) -> usize {
        if row_count == 0 {
            return 1;
        }
        row_count.div_ceil(self.page_size.max(1))
    }
}

// ── Data table builder + presets ─────────────────────────────────────────────

pub struct TableBuilder {
    id: String,
    columns: Vec<TableColumn>,
    row_count: usize,
    row_height: Option<f32>,
    height: f32,
    density: TableDensity,
    selected: Option<Signal<Option<usize>>>,
    sort: Option<Signal<TableSortState>>,
    sortable: bool,
    global_filter: Option<Signal<String>>,
    pagination: Option<Signal<TablePaginationState>>,
    caption: Option<String>,
    cell: Rc<dyn Fn(usize, usize) -> String>,
}

/// 默认密度数据表预设（shadcn 外观 + TanStack columns/data）。
pub fn table(id: impl Into<String>) -> TableBuilder {
    TableBuilder {
        id: id.into(),
        columns: Vec::new(),
        row_count: 0,
        row_height: None,
        height: 240.0,
        density: TableDensity::Default,
        selected: None,
        sort: None,
        sortable: true,
        global_filter: None,
        pagination: None,
        caption: None,
        cell: Rc::new(|r, c| format!("{r}:{c}")),
    }
}

/// [`table`] 的别名（强调「数据表」角色，对齐 solid-ui `data-table-demo`）。
pub fn data_table(id: impl Into<String>) -> TableBuilder {
    table(id)
}

/// 紧凑密度预设。
pub fn table_compact(id: impl Into<String>) -> TableBuilder {
    table(id).density(TableDensity::Compact)
}

/// 舒适密度预设。
pub fn table_comfortable(id: impl Into<String>) -> TableBuilder {
    table(id).density(TableDensity::Comfortable)
}

impl TableBuilder {
    pub fn columns(mut self, cols: impl IntoIterator<Item = TableColumn>) -> Self {
        self.columns = cols.into_iter().collect();
        self
    }

    pub fn row_count(mut self, n: usize) -> Self {
        self.row_count = n;
        self
    }

    /// Convenience: materialize `rows` as string grid (`cell` = `rows[r][c]`).
    pub fn rows(mut self, rows: impl IntoIterator<Item = Vec<String>>) -> Self {
        let data: Vec<Vec<String>> = rows.into_iter().collect();
        self.row_count = data.len();
        self.cell = Rc::new(move |r, c| {
            data.get(r)
                .and_then(|row| row.get(c))
                .cloned()
                .unwrap_or_default()
        });
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = Some(h.max(1.0));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h.max(1.0);
        self
    }

    pub fn density(mut self, d: impl Into<TableDensity>) -> Self {
        self.density = d.into();
        self
    }

    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected = Some(s);
        self
    }

    pub fn sort_signal(mut self, s: Signal<TableSortState>) -> Self {
        self.sort = Some(s);
        self
    }

    pub fn sortable(mut self, yes: bool) -> Self {
        self.sortable = yes;
        self
    }

    /// 全局字符串筛选（对齐 TanStack `globalFilter`）：任一单元格包含即保留。
    pub fn global_filter_signal(mut self, s: Signal<String>) -> Self {
        self.global_filter = Some(s);
        self
    }

    /// 客户端分页；未设则渲染筛选/排序后的全部行（虚拟列表）。
    pub fn pagination_signal(mut self, s: Signal<TablePaginationState>) -> Self {
        self.pagination = Some(s);
        self
    }

    pub fn caption(mut self, c: impl Into<String>) -> Self {
        self.caption = Some(c.into());
        self
    }

    /// TanStack-style cell accessor: `(row_index, column_index) -> display string`.
    pub fn cell(mut self, f: impl Fn(usize, usize) -> String + 'static) -> Self {
        self.cell = Rc::new(f);
        self
    }
}

impl From<&str> for TableDensity {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TableDensity {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

fn cycle_sort(current: &TableSortState, column_id: &str) -> TableSortState {
    crate::utils::table_sort::cycle_sort(current, column_id)
}

fn filtered_row_order(
    row_count: usize,
    columns: &[TableColumn],
    cell: &Rc<dyn Fn(usize, usize) -> String>,
    filter: &str,
) -> Vec<usize> {
    crate::utils::table_sort::filtered_row_order(row_count, columns.len(), cell, filter)
}

fn sorted_row_order(
    order: Vec<usize>,
    columns: &[TableColumn],
    cell: &Rc<dyn Fn(usize, usize) -> String>,
    sort: &TableSortState,
) -> Vec<usize> {
    let col_index = sort
        .column_id
        .as_deref()
        .and_then(|id| columns.iter().position(|c| c.id == id));
    crate::utils::table_sort::sorted_row_order(order, col_index, cell, sort.dir)
}

fn paginate_order(order: Vec<usize>, page: TablePaginationState) -> Vec<usize> {
    let size = page.page_size.max(1);
    let start = page.page_index.saturating_mul(size);
    if start >= order.len() {
        return Vec::new();
    }
    let end = (start + size).min(order.len());
    order[start..end].to_vec()
}

fn sort_marker(sort: &TableSortState, col_id: &str) -> &'static str {
    if sort.column_id.as_deref() != Some(col_id) {
        return "";
    }
    match sort.dir {
        TableSortDir::Asc => " ↑",
        TableSortDir::Desc => " ↓",
        TableSortDir::None => "",
    }
}

impl From<TableBuilder> for Element {
    fn from(builder: TableBuilder) -> Self {
        let TableBuilder {
            id,
            columns,
            row_count,
            row_height,
            height,
            density,
            selected,
            sort,
            sortable,
            global_filter,
            pagination,
            caption,
            cell,
        } = builder;

        let header_h = density.header_h();
        let row_h = row_height.unwrap_or_else(|| density.row_h());

        let selected =
            selected.unwrap_or_else(|| use_signal(format!("{id}__table_sel").as_str(), || None));
        let sort = sort.unwrap_or_else(|| {
            use_signal(
                format!("{id}__table_sort").as_str(),
                TableSortState::default,
            )
        });
        let filter_sig = global_filter;
        let page_sig = pagination;
        let view_id = format!("{id}__table_view");
        let caption = caption;

        reactive_view(view_id, move || {
            let _ = selected.get();
            let sort_state = sort.get();
            let filter_text = filter_sig.as_ref().map(|s| s.get()).unwrap_or_default();
            let page_state = page_sig.as_ref().map(|s| s.get());

            let filtered = filtered_row_order(row_count, &columns, &cell, &filter_text);
            let sorted = sorted_row_order(filtered, &columns, &cell, &sort_state);
            let total_after_filter = sorted.len();
            let order = if let Some(p) = page_state {
                paginate_order(sorted, p)
            } else {
                sorted
            };

            let muted_fg = parse_text_color("muted-foreground");
            let fg = parse_text_color("foreground");

            // Header row — TableHead 视觉
            let mut header_row: Element = hstack()
                .w("100%")
                .min_w(0.0)
                .items_center()
                .border_style("solid")
                .border_bottom(tokens::border(), 1.0)
                .into();

            for col in &columns {
                let can_sort = sortable && col.sortable;
                let marker = if can_sort {
                    sort_marker(&sort_state, &col.id)
                } else {
                    ""
                };
                let label = format!("{}{marker}", col.header);
                let mut head: Element = hstack()
                    .w(col.width)
                    .h(header_h)
                    .min_h(header_h)
                    .px(tokens::SPACE_2)
                    .items_center()
                    .child(
                        text(label.as_str())
                            .size(TEXT_SM)
                            .weight(500.0)
                            .color(muted_fg),
                    )
                    .into();
                head = apply_cell_align(head, col.align);
                if can_sort {
                    let col_id = col.id.clone();
                    let sort_sig = sort;
                    let bid = format!("{id}:th:{}", col.id);
                    // ghost Button：可点排序，外观贴近 shadcn data-table header。
                    head = button(bid)
                        .variant("ghost")
                        .size("sm")
                        .w(col.width)
                        .h(header_h)
                        .on_click(move || {
                            let next = cycle_sort(&sort_sig.peek(), &col_id);
                            sort_sig.set(next);
                        })
                        .child(
                            text(label.as_str())
                                .size(TEXT_SM)
                                .weight(500.0)
                                .color(muted_fg),
                        )
                        .into();
                    head = apply_cell_align(head, col.align);
                }
                header_row = header_row.child(head);
            }

            let cols = columns.clone();
            let sel = selected;
            let cell_fn = cell.clone();
            let table_id = id.clone();
            let list: Element = virtual_list(format!("{id}__rows"))
                .item_count(order.len())
                .item_height(row_h)
                .height(height)
                .render_item(move |vis_i| {
                    let row = order.get(vis_i).copied().unwrap_or(vis_i);
                    let is_sel = sel.peek() == Some(row);
                    let mut row_el: Element = hstack()
                        .w("100%")
                        .min_w(0.0)
                        .h(row_h)
                        .items_center()
                        .border_style("solid")
                        .border_bottom(tokens::border(), 1.0)
                        .into();
                    if is_sel {
                        row_el = row_el.bg(tokens::muted());
                        row_el.set_attr("data-state", "selected");
                    } else {
                        row_el = row_el.bg([0.0, 0.0, 0.0, 0.0]);
                    }

                    for (ci, col) in cols.iter().enumerate() {
                        let label = cell_fn(row, ci);
                        let mut cell_el: Element = hstack()
                            .w(col.width)
                            .h(row_h)
                            .px(tokens::SPACE_2)
                            .items_center()
                            .child(text(label).size(TEXT_SM).weight(400.0).color(fg))
                            .into();
                        cell_el = apply_cell_align(cell_el, col.align);
                        row_el = row_el.child(cell_el);
                    }

                    let row_focus = format!("{table_id}:tr:{row}");
                    let hover_key = format!("{row_focus}_hover");
                    let hovered = use_signal(hover_key.as_str(), || false);
                    let _ = hovered.peek();
                    row_el.focus_id = Some(row_focus.clone());
                    row_el.template_id = Some(row_focus);
                    if !is_sel {
                        row_el.button_hover_paint = Some(ButtonHoverPaint {
                            variant: "ghost".into(),
                            disabled: false,
                            hover_easing: "spring".into(),
                            text_color: None,
                        });
                        row_el = row_el.on_hover(move |h| {
                            hovered.set_silent(h);
                            mark_hover_paint_dirty();
                        });
                    }

                    let s = sel;
                    row_el.on_click(move || s.set(Some(row)))
                })
                .into();

            let header_wrap: Element =
                mark_slot(vstack().w("100%").child(header_row).into(), SLOT_HEADER);
            let body_wrap: Element = mark_slot(list, SLOT_BODY);

            let mut stack_kids: Vec<Element> = Vec::new();
            if let Some(cap) = caption.clone() {
                stack_kids.push(mark_slot(
                    r#box()
                        .w("100%")
                        .mt(tokens::SPACE_4)
                        .child(text(cap).size(TEXT_SM).weight(400.0).color(muted_fg))
                        .into(),
                    SLOT_CAPTION,
                ));
            }
            stack_kids.push(header_wrap);
            stack_kids.push(body_wrap);

            if let Some(p) = page_state {
                let pages = TablePaginationState::page_count(p, total_after_filter);
                let page_sig = page_sig.expect("page_state implies signal");
                let info = format!(
                    "Page {} / {} · {} rows",
                    p.page_index + 1,
                    pages,
                    total_after_filter
                );
                let prev_id = format!("{id}:page-prev");
                let next_id = format!("{id}:page-next");
                let prev = button(prev_id)
                    .variant("outline")
                    .size("sm")
                    .disabled(p.page_index == 0)
                    .on_click(move || {
                        let mut cur = page_sig.peek();
                        cur.page_index = cur.page_index.saturating_sub(1);
                        page_sig.set(cur);
                    })
                    .child(text("Previous").size(TEXT_SM));
                let next = button(next_id)
                    .variant("outline")
                    .size("sm")
                    .disabled(p.page_index + 1 >= pages)
                    .on_click(move || {
                        let mut cur = page_sig.peek();
                        if cur.page_index + 1 < pages {
                            cur.page_index += 1;
                            page_sig.set(cur);
                        }
                    })
                    .child(text("Next").size(TEXT_SM));
                let footer_row: Element = hstack()
                    .w("100%")
                    .items_center()
                    .justify("between")
                    .px(tokens::SPACE_2)
                    .py(tokens::SPACE_2)
                    .bg(tokens::primary())
                    .child(
                        text(info)
                            .size(TEXT_SM)
                            .weight(500.0)
                            .color(tokens::primary_foreground_text()),
                    )
                    .child(
                        hstack()
                            .gap(tokens::SPACE_2)
                            .items_center()
                            .child(prev)
                            .child(next),
                    )
                    .into();
                stack_kids.push(mark_slot(footer_row, SLOT_FOOTER));
            }

            let inner: Element = vstack().w("100%").min_w(0.0).children(stack_kids).into();
            let mut root: Element = r#box()
                .w("100%")
                .min_w(0.0)
                .overflow("auto")
                .child(inner)
                .into();
            root.set_attr("rui_table", "1");
            root.set_attr("data-slot", "table");
            root.template_id = Some(id.clone());
            root
        })
    }
}

/// RSX：`<DataTable id columns=… row_count=…>` 轻量入口（单元格默认 `r:c`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DataTable(
    id: String,
    row_count: usize,
    height: Option<f32>,
    density: Option<String>,
    sortable: bool,
    caption: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = data_table(id).row_count(row_count).sortable(sortable);
    if let Some(h) = height {
        b = b.height(h);
    }
    if let Some(d) = density {
        b = b.density(d.as_str());
    }
    if let Some(c) = caption {
        b = b.caption(c);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_builds_and_sort_cycles() {
        let _el: Element = table("t1")
            .columns([
                TableColumn::new("a", "A").width(80.0),
                TableColumn::new("b", "B").width(120.0).align("end"),
            ])
            .row_count(50)
            .into();
        let s = TableSortState::default();
        let s1 = cycle_sort(&s, "a");
        assert_eq!(s1.dir, TableSortDir::Asc);
        let s2 = cycle_sort(&s1, "a");
        assert_eq!(s2.dir, TableSortDir::Desc);
    }

    #[test]
    fn density_presets_and_filter_paginate() {
        let _c: Element = table_compact("tc").row_count(3).into();
        let _f: Element = table_comfortable("tf").row_count(3).into();
        let cell: Rc<dyn Fn(usize, usize) -> String> = Rc::new(|r, c| format!("{}-{}", r, c));
        let cols = [TableColumn::new("a", "A")];
        let order = filtered_row_order(5, &cols, &cell, "2-");
        assert_eq!(order, vec![2]);
        let page = TablePaginationState::new(1, 2);
        let paged = paginate_order(vec![0, 1, 2, 3, 4], page);
        assert_eq!(paged, vec![2, 3]);
    }

    #[test]
    fn composite_table_builds() {
        let head = TableHead(TableHeadProps {
            w: Some("100".into()),
            align: None,
            children: Some(Element::from(text("Invoice"))),
            ..Default::default()
        });
        let header = TableHeader(TableHeaderProps {
            children: Some(TableRow(TableRowProps {
                id: "tr-h".into(),
                selected: Some(false),
                on_click: Some(Handler::default()),
                children: Some(head),
                ..Default::default()
            })),
            ..Default::default()
        });
        let body = TableBody(TableBodyProps {
            children: Some(TableRow(TableRowProps {
                id: "tr-b".into(),
                selected: Some(true),
                on_click: Some(Handler::default()),
                children: Some(TableCell(TableCellProps {
                    w: None,
                    align: None,
                    children: Some(Element::from(text("INV001"))),
                    ..Default::default()
                })),
                ..Default::default()
            })),
            ..Default::default()
        });
        let el = Table(TableProps {
            class: None,
            children: Some(vstack().child(header).child(body).into()),
            ..Default::default()
        });
        assert_eq!(el.attrs.get("rui_table").map(|s| s.as_str()), Some("1"));
    }
}
