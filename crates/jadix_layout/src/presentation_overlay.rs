//! Shared overlay dependency order and viewport placement.
use std::collections::{BTreeMap, BTreeSet};

use jadix_core::presentation::{NodeContent, NodeId, PopoverPlacement, Rect};

pub fn overlay_order(
    nodes: &BTreeMap<NodeId, (Option<NodeId>, &NodeContent)>,
) -> Result<Vec<NodeId>, NodeId> {
    overlay_order_with_priority(nodes, &BTreeMap::new())
}

pub fn overlay_order_with_priority(
    nodes: &BTreeMap<NodeId, (Option<NodeId>, &NodeContent)>,
    priority: &BTreeMap<NodeId, usize>,
) -> Result<Vec<NodeId>, NodeId> {
    let mut dependencies = BTreeMap::<NodeId, BTreeSet<NodeId>>::new();
    for (id, (parent, content)) in nodes {
        if !matches!(
            content,
            NodeContent::Dialog { .. } | NodeContent::Popover { .. }
        ) {
            continue;
        }
        let mut required = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut current = *parent;
        let mut visible = true;
        while let Some(ancestor) = current {
            if !visited.insert(ancestor) || visited.len() > 256 {
                return Err(*id);
            }
            let (parent, content) = nodes.get(&ancestor).ok_or(*id)?;
            if let NodeContent::Dialog { open, .. } | NodeContent::Popover { open, .. } = content {
                required.insert(ancestor);
                visible &= *open;
            }
            current = *parent;
        }
        if let NodeContent::Popover { anchor, open, .. } = content {
            if anchor.root != id.root || !nodes.contains_key(anchor) {
                return Err(*id);
            }
            let mut current = Some(*anchor);
            visited.clear();
            while let Some(ancestor) = current {
                if !visited.insert(ancestor) || visited.len() > 256 {
                    return Err(*id);
                }
                if ancestor == *id {
                    return Err(*id);
                }
                let (parent, content) = nodes.get(&ancestor).ok_or(*id)?;
                if let NodeContent::Dialog {
                    open: anchor_open, ..
                }
                | NodeContent::Popover {
                    open: anchor_open, ..
                } = content
                {
                    if *open && visible && !anchor_open {
                        return Err(*id);
                    }
                    required.insert(ancestor);
                }
                current = *parent;
            }
        }
        dependencies.insert(*id, required);
    }
    let mut dependents = BTreeMap::<NodeId, Vec<NodeId>>::new();
    let mut ready = BTreeSet::new();
    for (id, required) in &dependencies {
        if required.is_empty() {
            ready.insert((priority.get(id).copied().unwrap_or_default(), *id));
        }
        for dependency in required {
            dependents.entry(*dependency).or_default().push(*id);
        }
    }
    let mut ordered = Vec::new();
    while let Some((_, id)) = ready.pop_first() {
        ordered.push(id);
        if let Some(children) = dependents.get(&id) {
            for child in children {
                let required = dependencies.get_mut(child).ok_or(*child)?;
                required.remove(&id);
                if required.is_empty() {
                    ready.insert((priority.get(child).copied().unwrap_or_default(), *child));
                }
            }
        }
    }
    if ordered.len() != dependencies.len() {
        return Err(*dependencies
            .iter()
            .find(|(_, required)| !required.is_empty())
            .unwrap()
            .0);
    }
    Ok(ordered)
}

pub fn validate_anchors(
    nodes: &BTreeMap<NodeId, (Option<NodeId>, &NodeContent)>,
) -> Result<(), NodeId> {
    overlay_order(nodes).map(|_| ())
}

pub fn position(
    anchor: Rect,
    size: [f64; 2],
    viewport: [f32; 2],
    placement: PopoverPlacement,
    gap: u16,
) -> [f64; 2] {
    use PopoverPlacement::*;
    let (horizontal, leading, alignment) = match placement {
        TopStart => (false, true, 0),
        Top => (false, true, 1),
        TopEnd => (false, true, 2),
        BottomStart => (false, false, 0),
        Bottom => (false, false, 1),
        BottomEnd => (false, false, 2),
        LeftStart => (true, true, 0),
        Left => (true, true, 1),
        LeftEnd => (true, true, 2),
        RightStart => (true, false, 0),
        Right => (true, false, 1),
        RightEnd => (true, false, 2),
    };
    let axis = usize::from(!horizontal);
    let cross = 1 - axis;
    let origin = [anchor.x, anchor.y];
    let extent = [anchor.width, anchor.height];
    let viewport = viewport.map(f64::from);
    let margin = viewport.map(|value| 8.0_f64.min(value / 2.0));
    let before = origin[axis] - f64::from(gap) - margin[axis];
    let after = viewport[axis] - margin[axis] - origin[axis] - extent[axis] - f64::from(gap);
    let (preferred, opposite) = if leading {
        (before, after)
    } else {
        (after, before)
    };
    let leading = if preferred < size[axis] && opposite > preferred {
        !leading
    } else {
        leading
    };
    let mut result = [0.0; 2];
    result[axis] = if leading {
        origin[axis] - f64::from(gap) - size[axis]
    } else {
        origin[axis] + extent[axis] + f64::from(gap)
    };
    result[cross] = origin[cross]
        + match alignment {
            0 => 0.0,
            1 => (extent[cross] - size[cross]) / 2.0,
            _ => extent[cross] - size[cross],
        };
    for index in 0..2 {
        result[index] = result[index].clamp(
            margin[index],
            (viewport[index] - margin[index] - size[index]).max(margin[index]),
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadix_core::presentation::RootId;
    #[test]
    fn malformed_parent_cycles_fail_instead_of_hanging_shared_overlay_validation() {
        let root = RootId::new();
        let a = NodeId::new(root);
        let b = NodeId::new(root);
        let dialog = NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 0,
        };
        let nodes = BTreeMap::from([(a, (Some(b), &dialog)), (b, (Some(a), &dialog))]);
        assert!(overlay_order(&nodes).is_err());
    }
}
