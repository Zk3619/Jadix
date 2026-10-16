//! # Table
//!
//! ShadCN **base** 复合表壳 + **TanStack Table**（`reference/table-main`）columns / data 心智。
//! 样式对位 `Reference/ShadCN-UI/.../bases/base/ui/table.tsx`（`style-nova`：`cn-table-*`）。
//!
//! ```text
//! Table                         // table-container · relative · overflow-x-auto
//! └── table (inner)             // w-full · caption-bottom · text-sm
//!     ├── TableCaption          // mt-4 · text-sm · muted-foreground
//!     ├── TableHeader           // [&_tr]:border-b
//!     │   └── TableRow → TableHead
//!     ├── TableBody             // [&_tr:last-child]:border-0
//!     │   └── TableRow → TableCell  // hover muted/50 · selected muted
//!     └── TableFooter           // bg-muted/50 · border-t · font-medium
//! ```
//!
//! ## 数据表预设（TanStack 风格）
//!
//! | 入口 | 说明 |
//! |------|------|
//! | [`table`] / [`data_table`] | 默认密度；VirtualList 行体；排序 / 单行选中 / 可选分页与全局筛选 |
//! | [`table_compact`] | 紧凑行高 |
//! | [`table_comfortable`] | 舒适行高 |
//!
//! 预设复用复合壳视觉契约；不承诺分组 / 列 pinning / 多列排序（见 `docs/components/component_inventory.md`）。
//!
//! `ui!` 中 `<Table>` / `<TableRow>` / `<TableHead>` / `<TableCell>` 走通用 `*Props`；
//! 宏对 `selected` / `align` / `density` 做 `bind_attr`（与 Button `variant` 同模式）。
//! 列轨是领域 prop `col_w`（不是根布局 `w`）。RSX 写 `col_w`；Builder `.w()` 是别名。
//! 见 `docs/COMPONENT_LIBRARY.md` §1.1。
//!
//! 表体走 [`crate::app::reactive::reactive_view`] **结构岛**（排序 / 分页整表 rebuild），
//! 不是项级 `bind_*`。不要按组件把这里改成叶子绑（closeout Wave 55 停做）。
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags（ShadCN invoices 示例）
//! ui! {
//!     <Table>
//!         <TableCaption>"A list of your recent invoices."</TableCaption>
//!         <TableHeader>
//!             <TableRow id="inv-h">
//!                 <TableHead col_w="100">"Invoice"</TableHead>
//!                 <TableHead>"Status"</TableHead>
//!                 <TableHead align="end">"Amount"</TableHead>
//!             </TableRow>
//!         </TableHeader>
//!         <TableBody>
//!             <TableRow id="inv-1" selected={sel.get() == Some(0)}>
//!                 <TableCell>"INV001"</TableCell>
//!                 <TableCell>"Paid"</TableCell>
//!                 <TableCell align="end">"$250.00"</TableCell>
//!             </TableRow>
//!         </TableBody>
//!         <TableFooter>
//!             <TableRow id="inv-f">
//!                 <TableCell>"Total"</TableCell>
//!                 <TableCell align="end">"$2,500.00"</TableCell>
//!             </TableRow>
//!         </TableFooter>
//!     </Table>
//! }
//!
//! // B · Builder（TanStack columns + data）
//! Element::from(
//!     data_table("people")
//!         .columns([
//!             TableColumn::new("name", "Name").width(120.0),
//!             TableColumn::new("role", "Role").width(100.0).align("end"),
//!         ])
//!         .row_count(40)
//!         .sort_signal(sort)
//!         .global_filter_signal(filter)
//!         .pagination_signal(page)
//!         .selected_signal(sel)
//!         .cell(|r, c| format!("{r}:{c}")),
//! )
//! ```

use std::rc::Rc;

use crate::app::reactive::reactive_view;
use crate::components::base::{packed_children, SlotChrome, SlotStyleFn};
use crate::components::button::button;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{ButtonHoverPaint, Element, Handler, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::mark_hover_paint_dirty;
use crate::primitives::r#box::{gstack, hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::parse_color;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::take_packed;
use jadix_macros::component;

const ATTR_TABLE_SLOT: &str = "jadix_table_slot";
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
fn table_footer_bg() -> [f32; 4] {
    parse_color("muted/50")
}

fn strip_last_row_border(mut rows: Vec<Element>) -> Vec<Element> {
    if let Some(last) = rows.last_mut() {
        last.style.set_border_side_width(2, 0.0);
    }
    rows
}

fn apply_table_row_state(mut row: Element, selected: bool) -> Element {
    if selected {
        row = row.bg(tokens::muted());
        row.set_attr("data-state", "selected");
        row.button_hover_paint = None;
    } else {
        row = row.bg([0.0, 0.0, 0.0, 0.0]);
        row.attrs.remove("data-state");
    }
    row
}

fn wire_table_row_hover(mut row: Element, row_id: &str, selected: bool) -> Element {
    row.focus_id = Some(row_id.to_string());
    row.template_id = Some(row_id.to_string());
    if !selected {
        let hover_key = format!("{row_id}_hover");
        let hovered = use_signal(hover_key.as_str(), || false);
        let _ = hovered.peek();
        // Paint from `{id}_hover` (exclusive `on_hover` bubble). Class `hover:bg-*`
        // is the cell hit leaf, so `:hover` never lands on the row — first enter
        // stayed rest, and switching dest-reset both rows (leave flashed then died).
        row.button_hover_paint = Some(ButtonHoverPaint {
            variant: "table-row".into(),
            disabled: false,
            hover_easing: "cubic".into(),
            text_color: None,
        });
        row = row.on_hover(move |h| {
            hovered.set_silent(h);
            mark_hover_paint_dirty();
        });
    }
    row
}

fn head_cell_height(density: TableDensity) -> f32 {
    density.header_h()
}

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_TABLE_SLOT).map(|s| s.as_str())
}

fn mark_slot(mut el: Element, slot: &str) -> Element {
    el.set_attr(ATTR_TABLE_SLOT, slot);
    el
}

fn apply_cell_align(mut el: Element, align: TableAlign) -> Element {
    apply_cell_align_in_place(&mut el, align);
    el
}

/// 只写 `text-align`，**不**把文本叶撑成 `width: 100%`。
/// nowrap 叶在测宽时会忽略 available width；撑满后 `justify-content` 失效，
/// 而绘制宽度又回到 intrinsic，看起来永远左对齐。
fn patch_table_text_align_recursive(el: &mut Element, align: TableAlign) -> bool {
    use crate::style::{TextAlign, TextAlignDeclaration};

    let keyword = match align {
        TableAlign::Start => "start",
        TableAlign::Center => "center",
        TableAlign::End => "end",
    };
    let mut changed = false;
    crate::core::tree::walk_elements_mut_pruned(el, |node| {
        if node.text.is_some() {
            if let TextAlignDeclaration::Value(want) = TextAlign::parse_declaration(keyword) {
                if node.text_style.align != Some(want) {
                    node.text_style.align = Some(want);
                    changed = true;
                }
            }
            return false;
        }
        true
    });
    changed
}

fn apply_cell_align_in_place(el: &mut Element, align: TableAlign) -> bool {
    use taffy::prelude::JustifyContent;

    let want_justify = match align {
        TableAlign::Start => JustifyContent::FLEX_START,
        TableAlign::Center => JustifyContent::CENTER,
        TableAlign::End => JustifyContent::FLEX_END,
    };
    let mut changed = false;
    if el.style.justify_content != Some(want_justify) {
        el.style.justify_content = Some(want_justify);
        changed = true;
    }
    if patch_table_text_align_recursive(el, align) {
        changed = true;
    }
    changed
}

fn parse_table_col_width(raw: &str) -> crate::layout::dimension::CssDim {
    use crate::layout::dimension::{parse_css_dim, CssDim};
    let s = raw.trim();
    if s.is_empty() {
        return CssDim::Auto;
    }
    // ShadCN `w-[100px]` / Tailwind 表头列宽：无单位数字 = px（非 rem）。
    if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
        if let Ok(px) = s.parse::<f32>() {
            return CssDim::Px(px);
        }
    }
    parse_css_dim(s).unwrap_or(CssDim::Auto)
}

fn apply_table_cell_width(mut cell: Element, w: Option<&str>) -> Element {
    cell = cell.min_w(0.0).w("100%").justify_self_kw("stretch");
    if let Some(width) = w.filter(|s| !s.trim().is_empty()) {
        cell.set_attr("data-table-col-w", width.trim());
    }
    cell
}

fn finish_table_row_grid(mut row: Element, tracks: Vec<crate::layout::GridTrack>) -> Element {
    row.style.display = taffy::prelude::Display::Grid;
    row.style.grid_template_columns = tracks;
    row.style.justify_items = Some(taffy::prelude::AlignItems::STRETCH);
    row
}

fn is_table_row_slot(el: &Element) -> bool {
    el.attrs.get("data-slot").map(|s| s.as_str()) == Some("table-row")
}

fn find_first_table_row(el: &Element) -> Option<&Element> {
    crate::core::tree::find_in_tree(el, is_table_row_slot)
}

fn stamp_shared_column_tracks(el: &mut Element, tracks: &[crate::layout::GridTrack]) {
    crate::core::tree::walk_elements_mut(el, |node| {
        if is_table_row_slot(node) {
            node.style.display = taffy::prelude::Display::Grid;
            node.style.grid_template_columns = tracks.to_vec();
            node.style.justify_items = Some(taffy::prelude::AlignItems::STRETCH);
        }
    });
}

/// 用表头（否则第一行）的 `data-table-col-w` 作为整表列轨，避免各行独立 `1fr` 对不齐。
fn infer_shared_column_tracks(sections: &[Element]) -> Option<Vec<crate::layout::GridTrack>> {
    let header_row = sections
        .iter()
        .find(|el| slot_of(el) == Some(SLOT_HEADER))
        .and_then(find_first_table_row);
    let row = header_row.or_else(|| sections.iter().find_map(find_first_table_row))?;
    if row.children.is_empty() {
        return None;
    }
    Some(table_row_grid_template_from_cells(&row.children))
}

/// 从行内单元格的 `data-table-col-w` 生成共享列轨（各行同模板 → 列纵向对齐）。
fn table_row_grid_template_from_cells(cells: &[Element]) -> Vec<crate::layout::GridTrack> {
    use crate::layout::track_fr_1;
    use taffy::prelude::{length, GridTemplateComponent};

    if cells.is_empty() {
        return crate::layout::cols_equal_fr(1);
    }
    cells
        .iter()
        .map(|cell| {
            if let Some(raw) = cell.attrs.get("data-table-col-w") {
                let dim = parse_table_col_width(raw);
                if let crate::layout::dimension::CssDim::Px(px) = dim {
                    return GridTemplateComponent::Single(length(px));
                }
            }
            track_fr_1()
        })
        .collect()
}

fn table_row_grid_template_from_columns(columns: &[TableColumn]) -> Vec<crate::layout::GridTrack> {
    use crate::layout::{track_fr_1, GridTrack};
    use taffy::prelude::{fr, length, minmax};

    if columns.is_empty() {
        return crate::layout::cols_equal_fr(1);
    }
    columns
        .iter()
        .map(|col| {
            if col.width.is_finite() && col.width > 0.0 {
                // 指定宽度是下限；`minmax(w, 1fr)` 让列铺满表格，`align=end` 才看得到。
                minmax::<GridTrack>(length(col.width), fr(1.0))
            } else {
                track_fr_1()
            }
        })
        .collect()
}

fn style_textish(el: &mut Element, size: f32, weight: f32, color: [u8; 4]) {
    if el.text.is_some() {
        el.text_style.size = Some(size);
        el.text_style.weight = Some(weight as u16);
        el.text_style.color = Some(color);
        el.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        el.attrs
            .insert("white_space".to_string(), "nowrap".to_string());
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

impl std::str::FromStr for TableAlign {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

impl From<&str> for TableAlign {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TableAlign {
    fn from(s: String) -> Self {
        Self::parse(&s)
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

impl std::str::FromStr for TableDensity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

// ── Composite（solid-ui / shadcn）────────────────────────────────────────────

/// 表根：`relative w-full overflow-x-auto` 外壳 + 内层 `w-full caption-bottom text-sm`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Table(children: Option<Element>) -> Element {
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

    let mut ordered = Vec::new();
    ordered.append(&mut caption);
    ordered.append(&mut header);
    ordered.append(&mut body);
    ordered.append(&mut footer);
    ordered.append(&mut rest);

    if let Some(tracks) = infer_shared_column_tracks(&ordered) {
        for section in &mut ordered {
            stamp_shared_column_tracks(section, &tracks);
        }
    }

    let mut inner: Element = vstack().w("100%").min_w(0.0).children(ordered).into();
    inner.set_attr("data-slot", "table");
    table_container_wrap(inner)
}

/// ShadCN `relative w-full overflow-x-auto`. Y stays visible so the wrap is
/// content-height (not a nested Y scrollport that flex-shrinks to a clip and
/// dest-resets cell glyphs on the next chrome / hover frame).
fn table_container_wrap(inner: Element) -> Element {
    let mut wrap: Element = r#box()
        .relative(true)
        .w("100%")
        .min_w(0.0)
        .flex_shrink(0.0)
        .overflow_x("auto")
        .child(inner)
        .into();
    wrap.set_attr("jadix_table", "1");
    wrap.set_attr("data-slot", "table-container");
    wrap
}

/// `thead`：子行带底边（[`TableRow`] `border-b`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack().w("100%").min_w(0.0).children(kids).into();
    el.set_attr("data-slot", "table-header");
    mark_slot(el, SLOT_HEADER)
}

/// `tbody`：末行去底边（`[&_tr:last-child]:border-0`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableBody(children: Option<Element>) -> Element {
    let kids = strip_last_row_border(children.map(take_packed).unwrap_or_default());
    let mut el: Element = vstack().w("100%").min_w(0.0).children(kids).into();
    el.set_attr("data-slot", "table-body");
    mark_slot(el, SLOT_BODY)
}

/// Footer wrapper already paints `muted/50`. Child rows keep hover wiring, but
/// rest fill matches the wrapper so hover/leave is a no-op and must not dest-reset
/// (CLEAR of the same color punches the wrapper `border-t` / row `border-b`).
fn stamp_footer_row_rest_fill(el: &mut Element) {
    if is_table_row_slot(el) {
        el.set_attr("data-table-footer-row", "1");
        if el.attrs.get("data-state").map(String::as_str) != Some("selected") {
            el.style.bg_color = table_footer_bg();
        }
    }
    for child in &mut el.children {
        stamp_footer_row_rest_fill(child);
    }
}

/// `tfoot`：`bg-muted/50 border-t font-medium`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let fg = parse_text_color("foreground");
    let mut styled = kids;
    for k in &mut styled {
        style_textish(k, TEXT_SM, 500.0, fg);
        stamp_footer_row_rest_fill(k);
    }
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .border_style("solid")
        .border_top(tokens::border(), 1.0)
        .bg(table_footer_bg())
        .children(styled)
        .into();
    el.set_attr("data-slot", "table-footer");
    mark_slot(el, SLOT_FOOTER)
}

/// `tr`：`border-b transition-colors`；`hover:bg-muted/50`；`data-state=selected` → `bg-muted`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableRow(
    id: String,
    selected: bool,
    mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let col_tracks = table_row_grid_template_from_cells(&kids);
    let mut row: Element = gstack()
        .w("100%")
        .min_w(0.0)
        .grid_template_columns(col_tracks.clone())
        .items_center()
        .border_style("solid")
        .border_bottom(tokens::border(), 1.0)
        .children(kids)
        .into();
    row = finish_table_row_grid(row, col_tracks);

    row = apply_table_row_state(row, selected);

    let row_id = if id.trim().is_empty() {
        format!("jadix-tr-{}", row_uid())
    } else {
        id
    };

    row = wire_table_row_hover(row, &row_id, selected);
    row = row.on_click(move || on_click.call(()));
    row.set_attr("data-slot", "table-row");
    row
}

fn row_uid() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// `th`：`h-10 px-2 text-left align-middle font-medium text-foreground whitespace-nowrap`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableHead(
    col_w: Option<String>,
    align: Option<String>,
    children: Option<Element>,
) -> Element {
    let align = align
        .as_deref()
        .map(parse_align)
        .unwrap_or(TableAlign::Start);
    let fg = parse_text_color("foreground");
    let head_h = head_cell_height(TableDensity::Default);
    let mut kids = children.map(take_packed).unwrap_or_default();
    if kids.is_empty() {
        kids.push(Element::from(
            text("").size(TEXT_SM).weight(500.0).color(fg),
        ));
    } else {
        for k in &mut kids {
            style_textish(k, TEXT_SM, 500.0, fg);
        }
    }

    let mut cell: Element = hstack()
        .h(head_h)
        .min_h(head_h)
        .px(tokens::SPACE_2)
        .items_center()
        .min_w(0.0)
        .children(kids)
        .into();
    cell = apply_table_cell_width(cell, col_w.as_deref());
    cell = apply_cell_align(cell, align);
    cell.set_attr("data-slot", "table-head");
    cell
}

/// `td`：`p-2 align-middle whitespace-nowrap`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TableCell(
    col_w: Option<String>,
    align: Option<String>,
    children: Option<Element>,
) -> Element {
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
    cell = apply_table_cell_width(cell, col_w.as_deref());
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
    let mut cap: Element = r#box().w("100%").mt(tokens::SPACE_4).children(kids).into();
    cap.set_attr("data-slot", "table-caption");
    mark_slot(cap, SLOT_CAPTION)
}

impl_simple_slot_builder!(
    TableHeaderBuilder,
    table_header,
    TableHeader,
    TableHeaderProps
);
impl_simple_slot_builder!(TableBodyBuilder, table_body, TableBody, TableBodyProps);
impl_simple_slot_builder!(
    TableFooterBuilder,
    table_footer,
    TableFooter,
    TableFooterProps
);
impl_simple_slot_builder!(
    TableCaptionBuilder,
    table_caption,
    TableCaption,
    TableCaptionProps
);
impl_simple_slot_builder!(TableViewBuilder, ui_table, Table, TableProps);

pub struct TableRowBuilder {
    id: String,
    selected: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn table_row() -> TableRowBuilder {
    TableRowBuilder {
        id: String::new(),
        selected: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl TableRowBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn selected(mut self, v: bool) -> Self {
        self.selected = v;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
}

impl From<TableRowBuilder> for Element {
    fn from(builder: TableRowBuilder) -> Self {
        let el = TableRow(TableRowProps {
            id: builder.id,
            selected: Some(builder.selected),
            on_click: Default::default(),
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for TableRowBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct TableHeadBuilder {
    col_w: Option<String>,
    align: Option<String>,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn table_head() -> TableHeadBuilder {
    TableHeadBuilder {
        col_w: None,
        align: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl TableHeadBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn col_w(mut self, w: impl Into<String>) -> Self {
        self.col_w = Some(w.into());
        self
    }
    /// Alias of [`Self::col_w`] (column track, not layout `w`).
    pub fn w(self, w: impl Into<String>) -> Self {
        self.col_w(w)
    }

    pub fn align(mut self, a: impl Into<String>) -> Self {
        self.align = Some(a.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<TableHeadBuilder> for Element {
    fn from(builder: TableHeadBuilder) -> Self {
        let el = TableHead(TableHeadProps {
            col_w: builder.col_w,
            align: builder.align,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for TableHeadBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct TableCellBuilder {
    col_w: Option<String>,
    align: Option<String>,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn table_cell() -> TableCellBuilder {
    TableCellBuilder {
        col_w: None,
        align: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl TableCellBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn col_w(mut self, w: impl Into<String>) -> Self {
        self.col_w = Some(w.into());
        self
    }
    /// Alias of [`Self::col_w`] (column track, not layout `w`).
    pub fn w(self, w: impl Into<String>) -> Self {
        self.col_w(w)
    }

    pub fn align(mut self, a: impl Into<String>) -> Self {
        self.align = Some(a.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<TableCellBuilder> for Element {
    fn from(builder: TableCellBuilder) -> Self {
        let el = TableCell(TableCellProps {
            col_w: builder.col_w,
            align: builder.align,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for TableCellBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl TableViewBuilder {
    pub fn header_with(self, f: impl FnOnce(TableHeaderBuilder) -> TableHeaderBuilder) -> Self {
        self.child(f(table_header()))
    }
    pub fn body_with(self, f: impl FnOnce(TableBodyBuilder) -> TableBodyBuilder) -> Self {
        self.child(f(table_body()))
    }
    pub fn footer_with(self, f: impl FnOnce(TableFooterBuilder) -> TableFooterBuilder) -> Self {
        self.child(f(table_footer()))
    }
    pub fn caption(self, text: impl Into<String>) -> Self {
        self.child(table_caption().text(text))
    }
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

// ── Data table builder + presets (ADR-9 surface B) ───────────────────────────

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
    style: Option<SlotStyleFn>,
}

/// 默认密度数据表预设（ShadCN 外观 + TanStack columns/data）。
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
        style: None,
    }
}

/// [`table`] 的别名（强调数据表角色，对齐 ShadCN data-table demo）。
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

    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.style = Some(std::rc::Rc::new(f));
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
            style,
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
            let head_fg = parse_text_color("foreground");
            let fg = parse_text_color("foreground");

            let col_tracks = table_row_grid_template_from_columns(&columns);

            // Header row — TableHead 视觉
            let mut header_row: Element = gstack()
                .w("100%")
                .min_w(0.0)
                .grid_template_columns(col_tracks.clone())
                .items_center()
                .border_style("solid")
                .border_bottom(tokens::border(), 1.0)
                .into();
            header_row = finish_table_row_grid(header_row, col_tracks.clone());
            header_row.set_attr("data-slot", "table-row");

            for col in &columns {
                let can_sort = sortable && col.sortable;
                let marker = if can_sort {
                    sort_marker(&sort_state, &col.id)
                } else {
                    ""
                };
                let label = format!("{}{marker}", col.header);
                let mut head: Element = hstack()
                    .w("100%")
                    .min_w(0.0)
                    .h(header_h)
                    .min_h(header_h)
                    .px(tokens::SPACE_2)
                    .items_center()
                    .child(
                        text(label.as_str())
                            .size(TEXT_SM)
                            .weight(500.0)
                            .color(head_fg),
                    )
                    .into();
                head = head.whitespace_nowrap();
                head.set_attr("data-slot", "table-head");
                head = head.justify_self_kw("stretch");
                head = apply_cell_align(head, col.align);
                if can_sort {
                    let col_id = col.id.clone();
                    let align = col.align;
                    let sort_sig = sort;
                    let bid = format!("{id}:th:{}", col.id);
                    head = button(bid)
                        .variant("ghost")
                        .size("sm")
                        .w_full()
                        .h(header_h)
                        // Apply before Button snapshots its class baseline; later
                        // layout/hover resolution must retain the column alignment.
                        .style(move |mut b| {
                            apply_cell_align_in_place(&mut b.el, align);
                            b.px(tokens::SPACE_2)
                        })
                        .on_click(move || {
                            let next = cycle_sort(&sort_sig.peek(), &col_id);
                            sort_sig.set(next);
                        })
                        .child(
                            text(label.as_str())
                                .size(TEXT_SM)
                                .weight(500.0)
                                .color(head_fg),
                        )
                        .into();
                    head = head.min_w(0.0).justify_self_kw("stretch");
                    head.set_attr("data-slot", "table-head");
                }
                header_row = header_row.child(head);
            }

            let cols = columns.clone();
            let sel = selected;
            let cell_fn = cell.clone();
            let table_id = id.clone();
            let row_count_vis = order.len();
            let list: Element = virtual_list(format!("{id}__rows"))
                .item_count(row_count_vis)
                .item_height(row_h)
                .height(height)
                .render_item(move |vis_i| {
                    let row = order.get(vis_i).copied().unwrap_or(vis_i);
                    let is_sel = sel.peek() == Some(row);
                    let is_last = vis_i + 1 == row_count_vis;
                    let mut row_el: Element = gstack()
                        .w("100%")
                        .min_w(0.0)
                        .grid_template_columns(col_tracks.clone())
                        .h(row_h)
                        .items_center()
                        .border_style("solid")
                        .border_bottom(tokens::border(), 1.0)
                        .into();
                    row_el = finish_table_row_grid(row_el, col_tracks.clone());
                    if is_last {
                        row_el.style.set_border_side_width(2, 0.0);
                    }
                    row_el = apply_table_row_state(row_el, is_sel);

                    for (ci, col) in cols.iter().enumerate() {
                        let label = cell_fn(row, ci);
                        let mut cell_el: Element = hstack()
                            .w("100%")
                            .min_w(0.0)
                            .h(row_h)
                            .px(tokens::SPACE_2)
                            .items_center()
                            .child(text(label).size(TEXT_SM).weight(400.0).color(fg))
                            .into();
                        cell_el = cell_el.justify_self_kw("stretch");
                        cell_el = cell_el.whitespace_nowrap();
                        cell_el.set_attr("data-slot", "table-cell");
                        cell_el = apply_cell_align(cell_el, col.align);
                        row_el = row_el.child(cell_el);
                    }

                    let row_focus = format!("{table_id}:tr:{row}");
                    row_el = wire_table_row_hover(row_el, &row_focus, is_sel);
                    row_el.set_attr("data-slot", "table-row");

                    let s = sel;
                    row_el.on_click(move || s.set(Some(row)))
                })
                .into();

            let mut header_wrap: Element =
                mark_slot(vstack().w("100%").child(header_row).into(), SLOT_HEADER);
            header_wrap.set_attr("data-slot", "table-header");
            let mut body_wrap: Element = mark_slot(list, SLOT_BODY);
            body_wrap.set_attr("data-slot", "table-body");

            let mut stack_kids: Vec<Element> = Vec::new();
            if let Some(cap) = caption.clone() {
                let mut cap_el: Element = r#box()
                    .w("100%")
                    .mt(tokens::SPACE_4)
                    .child(text(cap).size(TEXT_SM).weight(400.0).color(muted_fg))
                    .into();
                cap_el.set_attr("data-slot", "table-caption");
                stack_kids.push(mark_slot(cap_el, SLOT_CAPTION));
            }
            stack_kids.push(header_wrap);
            stack_kids.push(body_wrap);

            if let Some(p) = page_state {
                let pages = TablePaginationState::page_count(p, total_after_filter);
                let page_sig = page_sig.expect("page_state implies signal");
                let info = crate::tr_with_default_zh(
                    "jadix.table.page_info",
                    "Page {page} / {pages} · {rows} rows",
                    "第 {page} / {pages} 页 · 共 {rows} 条",
                    &[
                        ("page", &(p.page_index + 1).to_string()),
                        ("pages", &pages.to_string()),
                        ("rows", &total_after_filter.to_string()),
                    ],
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
                    .child(
                        text(crate::tr_default_zh(
                            "jadix.table.previous",
                            "Previous",
                            "上一页",
                        ))
                        .size(TEXT_SM),
                    );
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
                    .child(
                        text(crate::tr_default_zh("jadix.table.next", "Next", "下一页"))
                            .size(TEXT_SM),
                    );
                let footer_row: Element = hstack()
                    .w("100%")
                    .items_center()
                    .justify("between")
                    .px(tokens::SPACE_2)
                    .py(tokens::SPACE_2)
                    .border_style("solid")
                    .border_top(tokens::border(), 1.0)
                    .bg(table_footer_bg())
                    .child(text(info).size(TEXT_SM).weight(500.0).color(fg))
                    .child(
                        hstack()
                            .gap(tokens::SPACE_2)
                            .items_center()
                            .child(prev)
                            .child(next),
                    )
                    .into();
                let mut footer_wrap = mark_slot(footer_row, SLOT_FOOTER);
                footer_wrap.set_attr("data-slot", "table-footer");
                stack_kids.push(footer_wrap);
            }

            let mut inner: Element = vstack().w("100%").min_w(0.0).children(stack_kids).into();
            inner.set_attr("data-slot", "table");
            let mut root = table_container_wrap(inner);
            root.template_id = Some(id.clone());
            if let Some(ref style) = style {
                root = style(crate::components::base::BaseComponentBuilder::new(root)).el;
            }
            root
        })
    }
}

/// RSX：`selected={sig.get()}` 原位刷新行底色（不整树重建）。
pub fn apply_bound_table_row_selected(el: &mut Element, selected: bool) -> LayoutStatus {
    let want_bg = if selected {
        tokens::muted()
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let mut changed = false;
    if el.style.bg_color != want_bg {
        el.style.bg_color = want_bg;
        changed = true;
    }
    if selected {
        if el.attrs.get("data-state").map(|s| s.as_str()) != Some("selected") {
            el.set_attr("data-state", "selected");
            changed = true;
        }
        if el.button_hover_paint.is_some() {
            el.button_hover_paint = None;
            changed = true;
        }
    } else {
        if el.attrs.contains_key("data-state") {
            el.attrs.remove("data-state");
            changed = true;
        }
    }
    if changed {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

/// RSX：`align={…}` / `align="end"` 原位刷新单元格对齐。
pub fn apply_bound_table_align(el: &mut Element, raw: impl AsRef<str>) -> LayoutStatus {
    if apply_cell_align_in_place(el, TableAlign::parse(raw.as_ref())) {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
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
    fn builder_ui_table_header_cell_style() {
        let el: Element = ui_table()
            .header_with(|h| {
                h.child(
                    table_row().child(table_head().style(|b| b.text_size(20.0)).text("Invoice")),
                )
            })
            .into();
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "table_head .style(|b| b.text_size(20)) must land on the header tree"
        );
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
    fn parse_table_col_width_bare_number_is_px() {
        use crate::layout::dimension::{dim_length_px, CssDim};
        let dim = parse_table_col_width("100");
        assert!(matches!(dim, CssDim::Px(v) if (v - 100.0).abs() < 1e-4));
        let px = parse_table_col_width("6.25rem");
        assert!(dim_length_px(px.as_immediate_dimension().unwrap()).unwrap() > 90.0);
    }

    #[test]
    fn table_row_unpacks_four_heads_from_fragment() {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            TableHead(TableHeadProps {
                col_w: Some("100".into()),
                align: None,
                children: Some(Element::from(text("Invoice"))),
                ..Default::default()
            }),
            TableHead(TableHeadProps {
                col_w: None,
                align: None,
                children: Some(Element::from(text("Status"))),
                ..Default::default()
            }),
            TableHead(TableHeadProps {
                col_w: None,
                align: None,
                children: Some(Element::from(text("Method"))),
                ..Default::default()
            }),
            TableHead(TableHeadProps {
                col_w: None,
                align: Some("end".into()),
                children: Some(Element::from(text("Amount"))),
                ..Default::default()
            }),
        ]);
        let row = TableRow(TableRowProps {
            id: "hdr".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(frag),
            ..Default::default()
        });
        assert_eq!(row.children.len(), 4, "each TableHead must be a row column");
        assert_eq!(
            row.children[0].attrs.get("data-slot").map(|s| s.as_str()),
            Some("table-head")
        );
    }

    #[test]
    fn table_row_grid_template_uses_col_w_attr() {
        use taffy::prelude::Display;

        let fixed = apply_table_cell_width(r#box().into(), Some("100px"));
        let flex = apply_table_cell_width(r#box().into(), None);
        let tracks = table_row_grid_template_from_cells(&[fixed, flex]);
        assert_eq!(tracks.len(), 2);

        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            apply_table_cell_width(r#box().into(), Some("100px")),
            apply_table_cell_width(r#box().into(), None),
        ]);
        let row = TableRow(TableRowProps {
            id: "grid-r".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(frag),
            ..Default::default()
        });
        assert_eq!(row.style.display, Display::Grid);
        assert_eq!(row.style.grid_template_columns.len(), 2);
    }

    #[test]
    fn apply_cell_align_end_fills_text_leaf() {
        use crate::style::TextAlign;
        use taffy::prelude::JustifyContent;

        let cell = apply_cell_align(
            hstack().child(text("$250.00").size(14.0)).into(),
            TableAlign::End,
        );
        assert_eq!(cell.style.justify_content, Some(JustifyContent::FLEX_END));
        assert_eq!(cell.children.len(), 1);
        let leaf = &cell.children[0];
        assert_eq!(leaf.text_style.align, Some(TextAlign::End));
    }

    #[test]
    fn table_head_amount_align_end() {
        use crate::style::TextAlign;
        use taffy::prelude::JustifyContent;

        let head = TableHead(TableHeadProps {
            col_w: None,
            align: Some("end".into()),
            children: Some(Element::from(text("Amount"))),
            ..Default::default()
        });
        assert_eq!(head.style.justify_content, Some(JustifyContent::FLEX_END));
        assert_eq!(head.children.len(), 1);
        let leaf = &head.children[0];
        assert_eq!(leaf.text_style.align, Some(TextAlign::End));
    }

    #[test]
    fn composite_rows_share_header_column_tracks() {
        use taffy::prelude::Display;

        let header = TableHeader(TableHeaderProps {
            children: Some(TableRow(TableRowProps {
                id: "h".into(),
                selected: Some(false),
                on_click: Some(Handler::default()),
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag = frag.children(vec![
                        TableHead(TableHeadProps {
                            col_w: Some("100px".into()),
                            align: None,
                            children: Some(Element::from(text("A"))),
                            ..Default::default()
                        }),
                        TableHead(TableHeadProps {
                            col_w: None,
                            align: None,
                            children: Some(Element::from(text("B"))),
                            ..Default::default()
                        }),
                        TableHead(TableHeadProps {
                            col_w: Some("96px".into()),
                            align: Some("end".into()),
                            children: Some(Element::from(text("C"))),
                            ..Default::default()
                        }),
                    ]);
                    frag
                }),
                ..Default::default()
            })),
            ..Default::default()
        });
        let body = TableBody(TableBodyProps {
            children: Some(TableRow(TableRowProps {
                id: "b".into(),
                selected: Some(false),
                on_click: Some(Handler::default()),
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag = frag.children(vec![
                        TableCell(TableCellProps {
                            col_w: None,
                            align: None,
                            children: Some(Element::from(text("aaaaaa"))),
                            ..Default::default()
                        }),
                        TableCell(TableCellProps {
                            col_w: None,
                            align: None,
                            children: Some(Element::from(text("b"))),
                            ..Default::default()
                        }),
                        TableCell(TableCellProps {
                            col_w: None,
                            align: Some("end".into()),
                            children: Some(Element::from(text("1"))),
                            ..Default::default()
                        }),
                    ]);
                    frag
                }),
                ..Default::default()
            })),
            ..Default::default()
        });
        let mut kids = Element::new();
        kids.set_attr("jadix_fragment", "1");
        kids = kids.children(vec![header, body]);
        let el = Table(TableProps {
            children: Some(kids),
            ..Default::default()
        });
        let inner = &el.children[0];
        let header_row = find_first_table_row(
            inner
                .children
                .iter()
                .find(|c| slot_of(c) == Some(SLOT_HEADER))
                .expect("header"),
        )
        .expect("header row");
        let body_row = find_first_table_row(
            inner
                .children
                .iter()
                .find(|c| slot_of(c) == Some(SLOT_BODY))
                .expect("body"),
        )
        .expect("body row");
        assert_eq!(header_row.style.display, Display::Grid);
        assert_eq!(body_row.style.display, Display::Grid);
        assert_eq!(
            header_row.style.grid_template_columns.len(),
            body_row.style.grid_template_columns.len()
        );
        assert_eq!(
            header_row.style.grid_template_columns, body_row.style.grid_template_columns,
            "body rows must reuse the header column tracks"
        );
    }

    #[test]
    fn composite_table_column_x_aligns_across_rows() {
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::tree::LayoutTree;

        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let header = TableHeader(TableHeaderProps {
                    children: Some(TableRow(TableRowProps {
                        id: "lh".into(),
                        selected: Some(false),
                        on_click: Some(Handler::default()),
                        children: Some({
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag = frag.children(vec![
                                TableHead(TableHeadProps {
                                    col_w: Some("100px".into()),
                                    align: None,
                                    children: Some(Element::from(text("Inv"))),
                                    ..Default::default()
                                }),
                                TableHead(TableHeadProps {
                                    col_w: None,
                                    align: Some("end".into()),
                                    children: Some(Element::from(text("Amt"))),
                                    ..Default::default()
                                }),
                            ]);
                            frag
                        }),
                        ..Default::default()
                    })),
                    ..Default::default()
                });
                let body = TableBody(TableBodyProps {
                    children: Some(TableRow(TableRowProps {
                        id: "lb".into(),
                        selected: Some(false),
                        on_click: Some(Handler::default()),
                        children: Some({
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag = frag.children(vec![
                                TableCell(TableCellProps {
                                    col_w: None,
                                    align: None,
                                    children: Some(Element::from(text("INV001"))),
                                    ..Default::default()
                                }),
                                TableCell(TableCellProps {
                                    col_w: None,
                                    align: Some("end".into()),
                                    children: Some(Element::from(text("$1"))),
                                    ..Default::default()
                                }),
                            ]);
                            frag
                        }),
                        ..Default::default()
                    })),
                    ..Default::default()
                });
                let mut kids = Element::new();
                kids.set_attr("jadix_fragment", "1");
                kids = kids.children(vec![header, body]);
                let table = Table(TableProps {
                    children: Some(kids),
                    ..Default::default()
                });
                let mut root = Element::new().w(400.0).h(200.0).child(table);
                let mut tree = LayoutTree::new();
                tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);

                let inner = &root.children[0].children[0];
                let header_row = find_first_table_row(
                    inner
                        .children
                        .iter()
                        .find(|c| slot_of(c) == Some(SLOT_HEADER))
                        .unwrap(),
                )
                .unwrap();
                let body_row = find_first_table_row(
                    inner
                        .children
                        .iter()
                        .find(|c| slot_of(c) == Some(SLOT_BODY))
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(header_row.children.len(), 2);
                assert_eq!(body_row.children.len(), 2);
                for i in 0..2 {
                    let hx = header_row.children[i].layout_bounds[0];
                    let bx = body_row.children[i].layout_bounds[0];
                    let hw = header_row.children[i].layout_bounds[2];
                    let bw = body_row.children[i].layout_bounds[2];
                    assert!((hx - bx).abs() < 1.0, "col {i} x: header={hx} body={bx}");
                    assert!((hw - bw).abs() < 1.0, "col {i} w: header={hw} body={bw}");
                }
                let amt = &body_row.children[1];
                assert!(
                    amt.layout_bounds[2] > 200.0,
                    "end column should grow, got {}",
                    amt.layout_bounds[2]
                );
                let leaf = &amt.children[0];
                let leaf_right = leaf.layout_bounds[0] + leaf.layout_bounds[2];
                let cell_right = amt.layout_bounds[0] + amt.layout_bounds[2];
                assert!(
                    (cell_right - leaf_right).abs() < 12.0,
                    "end-aligned text should sit on the cell's trailing edge; leaf_right={leaf_right} cell_right={cell_right}"
                );
            });
        });
    }

    #[test]
    fn composite_table_builds() {
        let head = TableHead(TableHeadProps {
            col_w: Some("100".into()),
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
                    col_w: None,
                    align: None,
                    children: Some(Element::from(text("INV001"))),
                    ..Default::default()
                })),
                ..Default::default()
            })),
            ..Default::default()
        });
        let el = Table(TableProps {
            children: Some(vstack().child(header).child(body).into()),
            ..Default::default()
        });
        assert_eq!(el.attrs.get("jadix_table").map(|s| s.as_str()), Some("1"));
    }

    #[test]
    fn table_cell_picks_up_stylesheet_slot_rule() {
        use crate::style::{clear_stylesheets, parse_stylesheet, register_stylesheet};
        clear_stylesheets();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=table-cell]"]
classes = "underline"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        let cell = TableCell(TableCellProps {
            col_w: None,
            align: None,
            children: Some(Element::from(text("INV001"))),
            ..Default::default()
        });
        let cs = cell.class_set.as_ref().expect("class_set");
        assert!(
            cs.base.iter().any(|c| c == "underline"),
            "TableCell must bind [data-slot=table-cell]; got {:?}",
            cs.base
        );
        clear_stylesheets();
    }
}
