//! # Table
//!
//! 产品层数据表（对齐 TanStack Table 的 **columns + data** 模型 + solid-ui/ShadCN 表外观）。
//! 表头 + VirtualList 行；可选行选中与列排序。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!   table("people")
//!     .columns([
//!       TableColumn::new("name", "Name").width(120.0),
//!       TableColumn::new("role", "Role").width(100.0),
//!     ])
//!     .row_count(40)
//!     .cell(|r, c| format!("{r}:{c}"))
//!     .selected_signal(sel),
//! )
//! ```

use std::rc::Rc;

use crate::app::reactive::reactive_view;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::tokens;

#[derive(Clone)]
pub struct TableColumn {
    pub id: String,
    pub header: String,
    pub width: f32,
}

impl TableColumn {
    pub fn new(id: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            header: header.into(),
            width: 100.0,
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TableSortDir {
    #[default]
    None,
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TableSortState {
    pub column_id: Option<String>,
    pub dir: TableSortDir,
}

pub struct TableBuilder {
    id: String,
    columns: Vec<TableColumn>,
    row_count: usize,
    row_height: f32,
    height: f32,
    selected: Option<Signal<Option<usize>>>,
    sort: Option<Signal<TableSortState>>,
    sortable: bool,
    cell: Rc<dyn Fn(usize, usize) -> String>,
}

pub fn table(id: impl Into<String>) -> TableBuilder {
    TableBuilder {
        id: id.into(),
        columns: Vec::new(),
        row_count: 0,
        row_height: 28.0,
        height: 240.0,
        selected: None,
        sort: None,
        sortable: true,
        cell: Rc::new(|r, c| format!("{r}:{c}")),
    }
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
        self.row_height = h.max(1.0);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h.max(1.0);
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

    /// TanStack-style cell accessor: `(row_index, column_index) -> display string`.
    pub fn cell(mut self, f: impl Fn(usize, usize) -> String + 'static) -> Self {
        self.cell = Rc::new(f);
        self
    }
}

fn cycle_sort(current: &TableSortState, column_id: &str) -> TableSortState {
    if current.column_id.as_deref() != Some(column_id) {
        return TableSortState {
            column_id: Some(column_id.into()),
            dir: TableSortDir::Asc,
        };
    }
    match current.dir {
        TableSortDir::None | TableSortDir::Desc => TableSortState {
            column_id: Some(column_id.into()),
            dir: TableSortDir::Asc,
        },
        TableSortDir::Asc => TableSortState {
            column_id: Some(column_id.into()),
            dir: TableSortDir::Desc,
        },
    }
}

fn sorted_row_order(
    row_count: usize,
    columns: &[TableColumn],
    cell: &Rc<dyn Fn(usize, usize) -> String>,
    sort: &TableSortState,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..row_count).collect();
    let Some(col_id) = sort.column_id.as_deref() else {
        return order;
    };
    if sort.dir == TableSortDir::None {
        return order;
    }
    let Some(ci) = columns.iter().position(|c| c.id == col_id) else {
        return order;
    };
    let desc = sort.dir == TableSortDir::Desc;
    order.sort_by(|&a, &b| {
        let cmp = cell(a, ci).cmp(&cell(b, ci));
        if desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
    order
}

impl From<TableBuilder> for Element {
    fn from(builder: TableBuilder) -> Self {
        let TableBuilder {
            id,
            columns,
            row_count,
            row_height,
            height,
            selected,
            sort,
            sortable,
            cell,
        } = builder;
        let selected =
            selected.unwrap_or_else(|| use_signal(format!("{id}__table_sel").as_str(), || None));
        let sort = sort.unwrap_or_else(|| {
            use_signal(
                format!("{id}__table_sort").as_str(),
                TableSortState::default,
            )
        });
        let view_id = format!("{id}__table_view");

        reactive_view(view_id, move || {
            let _ = selected.get();
            let sort_state = sort.get();
            let order = sorted_row_order(row_count, &columns, &cell, &sort_state);

            let mut header = hstack().w("100%").bg(tokens::muted());
            for col in &columns {
                let label = if sortable && sort_state.column_id.as_deref() == Some(col.id.as_str())
                {
                    match sort_state.dir {
                        TableSortDir::Asc => format!("{} ↑", col.header),
                        TableSortDir::Desc => format!("{} ↓", col.header),
                        TableSortDir::None => col.header.clone(),
                    }
                } else {
                    col.header.clone()
                };
                let mut cell_el: Element = r#box()
                    .w(col.width)
                    .h(row_height)
                    .px("0.5rem")
                    .child(text(label.as_str()).size(12.0).weight(500.0))
                    .into();
                if sortable {
                    let col_id = col.id.clone();
                    let sort_sig = sort;
                    cell_el = interactive(cell_el)
                        .focus_id(format!("{id}:th:{}", col.id))
                        .focusable(true)
                        .on_click(move || {
                            let next = cycle_sort(&sort_sig.peek(), &col_id);
                            sort_sig.set(next);
                        })
                        .into();
                }
                header = header.child(cell_el);
            }

            let cols = columns.clone();
            let sel = selected;
            let cell_fn = cell.clone();
            let list: Element = virtual_list(format!("{id}__rows"))
                .item_count(order.len())
                .item_height(row_height)
                .height(height)
                .render_item(move |vis_i| {
                    let row = order.get(vis_i).copied().unwrap_or(vis_i);
                    let mut row_el: Element = hstack().w("100%").into();
                    let is_sel = sel.peek() == Some(row);
                    if is_sel {
                        row_el = row_el.bg(tokens::muted());
                    }
                    for (ci, col) in cols.iter().enumerate() {
                        let label = cell_fn(row, ci);
                        let cell: Element = r#box()
                            .w(col.width)
                            .h(row_height)
                            .px("0.5rem")
                            .child({
                                let t: Element = text(label).size(12.0).into();
                                t
                            })
                            .into();
                        row_el = row_el.child(cell);
                    }
                    let s = sel;
                    row_el.on_click(move || s.set(Some(row)))
                })
                .into();

            let mut root: Element = vstack().w("100%").child(header).child(list).into();
            root.set_attr("rui_table", "1");
            root.template_id = Some(id.clone());
            root
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_builds_and_sort_cycles() {
        let _el: Element = table("t1")
            .columns([
                TableColumn::new("a", "A").width(80.0),
                TableColumn::new("b", "B").width(120.0),
            ])
            .row_count(50)
            .into();
        let s = TableSortState::default();
        let s1 = cycle_sort(&s, "a");
        assert_eq!(s1.dir, TableSortDir::Asc);
        let s2 = cycle_sort(&s1, "a");
        assert_eq!(s2.dir, TableSortDir::Desc);
    }
}
