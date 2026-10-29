//! Table 排序 / 筛选序（WP-SI.8）。

use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TableSortDir {
    #[default]
    None,
    Asc,
    Desc,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableSortState {
    pub column_id: Option<String>,
    pub dir: TableSortDir,
}

pub fn cycle_sort(current: &TableSortState, column_id: &str) -> TableSortState {
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

pub fn filtered_row_order(
    row_count: usize,
    col_count: usize,
    cell: &Rc<dyn Fn(usize, usize) -> String>,
    filter: &str,
) -> Vec<usize> {
    let q = filter.trim().to_ascii_lowercase();
    if q.is_empty() {
        return (0..row_count).collect();
    }
    (0..row_count)
        .filter(|&r| (0..col_count).any(|ci| cell(r, ci).to_ascii_lowercase().contains(&q)))
        .collect()
}

pub fn sorted_row_order(
    mut order: Vec<usize>,
    col_index: Option<usize>,
    cell: &Rc<dyn Fn(usize, usize) -> String>,
    dir: TableSortDir,
) -> Vec<usize> {
    let Some(ci) = col_index else {
        return order;
    };
    if dir == TableSortDir::None {
        return order;
    }
    let desc = dir == TableSortDir::Desc;
    order.sort_by(|&a, &b| {
        let ca = cell(a, ci);
        let cb = cell(b, ci);
        let ord = ca.cmp(&cb);
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
    order
}

#[cfg(test)]
#[path = "tests/table_sort.rs"]
mod tests;
