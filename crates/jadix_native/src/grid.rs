use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::ops::Range;

use crate::presentation::{AccessibleRole, NodeContent, NodeId};
use crate::{BackendError, HeadlessRoot};

struct GridSpan {
    identifier: NodeId,
    rows: Range<u32>,
    columns: Range<u32>,
}

impl HeadlessRoot {
    pub(crate) fn validate_grid(&self) -> Result<(), BackendError> {
        let mut cells: BTreeMap<NodeId, Vec<GridSpan>> = BTreeMap::new();
        let mut headers: BTreeMap<NodeId, Vec<GridSpan>> = BTreeMap::new();
        for (identifier, node) in &self.nodes {
            let Some(role) = node.descriptor.accessible_role else {
                continue;
            };
            match role {
                AccessibleRole::Grid { rows, columns }
                    if rows > i32::MAX as u32
                        || columns > i32::MAX as u32
                        || !matches!(node.descriptor.content, NodeContent::Container) =>
                {
                    return Err(BackendError::InvalidGrid(*identifier));
                }
                AccessibleRole::GridCell { .. } | AccessibleRole::GridColumnHeader { .. } => {
                    if !(matches!(node.descriptor.content, NodeContent::Container)
                        || matches!(role, AccessibleRole::GridColumnHeader { .. })
                            && matches!(node.descriptor.content, NodeContent::Text(_)))
                    {
                        return Err(BackendError::InvalidGrid(*identifier));
                    }
                    let mut ancestor = node.parent;
                    let mut owner = None;
                    while let Some(parent) = ancestor {
                        let node = &self.nodes[&parent];
                        if let Some(AccessibleRole::Grid { rows, columns }) =
                            node.descriptor.accessible_role
                        {
                            owner = Some((parent, rows, columns));
                            break;
                        }
                        ancestor = node.parent;
                    }
                    let Some((parent, rows, columns)) = owner else {
                        return Err(BackendError::InvalidGrid(*identifier));
                    };
                    match role {
                        AccessibleRole::GridCell {
                            row,
                            column,
                            row_span,
                            column_span,
                        } => {
                            let Some(row_end) = row.checked_add(row_span) else {
                                return Err(BackendError::InvalidGrid(*identifier));
                            };
                            let Some(column_end) = column.checked_add(column_span) else {
                                return Err(BackendError::InvalidGrid(*identifier));
                            };
                            if row_span == 0
                                || column_span == 0
                                || row_end > rows
                                || column_end > columns
                            {
                                return Err(BackendError::InvalidGrid(*identifier));
                            }
                            cells.entry(parent).or_default().push(GridSpan {
                                identifier: *identifier,
                                rows: row..row_end,
                                columns: column..column_end,
                            });
                        }
                        AccessibleRole::GridColumnHeader {
                            column,
                            column_span,
                        } => {
                            let Some(column_end) = column.checked_add(column_span) else {
                                return Err(BackendError::InvalidGrid(*identifier));
                            };
                            if column_span == 0 || column_end > columns {
                                return Err(BackendError::InvalidGrid(*identifier));
                            }
                            headers.entry(parent).or_default().push(GridSpan {
                                identifier: *identifier,
                                rows: 0..1,
                                columns: column..column_end,
                            });
                        }
                        _ => unreachable!(),
                    }
                }
                _ => {}
            }
        }
        for spans in cells.values_mut().chain(headers.values_mut()) {
            verify_disjoint_spans(spans)?;
        }
        Ok(())
    }
}

fn verify_disjoint_spans(spans: &mut [GridSpan]) -> Result<(), BackendError> {
    spans.sort_unstable_by_key(|span| (span.rows.start, span.identifier));
    let mut active_columns = BTreeMap::new();
    let mut expiring_rows = BinaryHeap::new();
    for span in spans {
        while let Some(&Reverse((end_row, column))) = expiring_rows.peek() {
            if end_row > span.rows.start {
                break;
            }
            expiring_rows.pop();
            active_columns.remove(&column);
        }
        // Active spans share this row and have disjoint column ranges. Of the
        // spans that start before this one's end, only the last can overlap.
        if active_columns
            .range(..span.columns.end)
            .next_back()
            .is_some_and(|(_, &end)| end > span.columns.start)
        {
            return Err(BackendError::InvalidGrid(span.identifier));
        }
        active_columns.insert(span.columns.start, span.columns.end);
        expiring_rows.push(Reverse((span.rows.end, span.columns.start)));
    }
    Ok(())
}
