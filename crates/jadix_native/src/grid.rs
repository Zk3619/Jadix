use std::collections::BTreeSet;

use crate::presentation::{AccessibleRole, NodeContent};
use crate::{BackendError, HeadlessRoot};

impl HeadlessRoot {
    pub(crate) fn validate_grid(&self) -> Result<(), BackendError> {
        let mut cells = BTreeSet::new();
        let mut headers = BTreeSet::new();
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
                    let valid = match role {
                        AccessibleRole::GridCell { row, column } => {
                            row < rows && column < columns && cells.insert((parent, row, column))
                        }
                        AccessibleRole::GridColumnHeader { column } => {
                            column < columns && headers.insert((parent, column))
                        }
                        _ => unreachable!(),
                    };
                    if !valid {
                        return Err(BackendError::InvalidGrid(*identifier));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
