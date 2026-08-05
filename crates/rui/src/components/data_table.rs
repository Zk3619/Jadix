//! # DataTable
//!
//! 简易数据表：表头 + VirtualList 行；单选行。

use crate::app::reactive::reactive_view;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::tokens;
use std::rc::Rc;

#[derive(Clone)]
pub struct DataColumn {
    pub header: String,
    pub width: f32,
}

pub struct DataTableBuilder {
    id: String,
    columns: Vec<DataColumn>,
    row_count: usize,
    row_height: f32,
    height: f32,
    selected: Option<Signal<Option<usize>>>,
    render_cell: Rc<dyn Fn(usize, usize) -> String>,
}

pub fn data_table(id: impl Into<String>) -> DataTableBuilder {
    DataTableBuilder {
        id: id.into(),
        columns: Vec::new(),
        row_count: 0,
        row_height: 28.0,
        height: 240.0,
        selected: None,
        render_cell: Rc::new(|r, c| format!("{r}:{c}")),
    }
}

impl DataTableBuilder {
    pub fn columns(mut self, cols: impl IntoIterator<Item = DataColumn>) -> Self {
        self.columns = cols.into_iter().collect();
        self
    }

    pub fn row_count(mut self, n: usize) -> Self {
        self.row_count = n;
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected = Some(s);
        self
    }

    pub fn render_cell(mut self, f: impl Fn(usize, usize) -> String + 'static) -> Self {
        self.render_cell = Rc::new(f);
        self
    }
}

impl From<DataTableBuilder> for Element {
    fn from(builder: DataTableBuilder) -> Self {
        let DataTableBuilder {
            id,
            columns,
            row_count,
            row_height,
            height,
            selected,
            render_cell,
        } = builder;
        let sel_key = format!("{id}__dt_sel");
        let selected = selected.unwrap_or_else(|| use_signal(sel_key.as_str(), || None::<usize>));
        let view_id = format!("{id}__dt_view");

        reactive_view(view_id, move || {
            let _ = selected.get();
            let mut header = hstack().w("100%").bg(tokens::muted());
            for col in &columns {
                let cell: Element = r#box()
                    .w(col.width)
                    .h(row_height)
                    .px("0.5rem")
                    .child(text(col.header.as_str()).size(12.0))
                    .into();
                header = header.child(cell);
            }

            let cols = columns.clone();
            let sel = selected;
            let render_cell = render_cell.clone();
            let list: Element = virtual_list(format!("{id}__rows"))
                .item_count(row_count)
                .item_height(row_height)
                .height(height)
                .render_item(move |row| {
                    let mut row_el: Element = hstack().w("100%").into();
                    let is_sel = sel.peek() == Some(row);
                    if is_sel {
                        row_el = row_el.bg(tokens::muted());
                    }
                    for (ci, col) in cols.iter().enumerate() {
                        let label = render_cell(row, ci);
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

            vstack().w("100%").child(header).child(list).into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_table_builds() {
        let _el: Element = data_table("dt1")
            .columns([
                DataColumn {
                    header: "A".into(),
                    width: 80.0,
                },
                DataColumn {
                    header: "B".into(),
                    width: 120.0,
                },
            ])
            .row_count(50)
            .into();
    }
}
