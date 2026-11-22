//! Native materialization only: layout, keyed owners and edit state stay in the model.
//! Hosts pin stateful/interactive nodes and disable culling for assistive exploration.

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScrollViewport {
    pub offset: [f64; 2],
    pub size: [f64; 2],
    /// Document points/second at the last scroll sample.
    pub velocity: [f64; 2],
    /// Seconds since that sample. The forward lead decays even when scrolling stops.
    pub elapsed: f64,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CullNode {
    /// Parent index must precede the child; roots have no parent.
    pub parent: Option<usize>,
    /// Absolute layout coordinates, before ancestor scroll offsets.
    pub frame: [f64; 4],
    pub viewport: Option<ScrollViewport>,
    /// Preserve native state/focus and its ancestor chain, without pinning siblings.
    pub pinned: bool,
    /// Layout bounds cannot describe transformed/animated descendants. Keep this
    /// entire subtree materialized until the host can supply visual bounds.
    #[serde(default)]
    pub preserve_subtree: bool,
}

#[derive(Clone, Copy)]
struct Rect([f64; 4]);
impl Rect {
    fn intersects(self, other: Self) -> bool {
        self.0[0] <= other.0[2]
            && self.0[2] >= other.0[0]
            && self.0[1] <= other.0[3]
            && self.0[3] >= other.0[1]
    }
    fn intersection(self, other: Self) -> Self {
        Self([
            self.0[0].max(other.0[0]),
            self.0[1].max(other.0[1]),
            self.0[2].min(other.0[2]),
            self.0[3].min(other.0[3]),
        ])
    }
    fn union(self, other: Self) -> Self {
        Self([
            self.0[0].min(other.0[0]),
            self.0[1].min(other.0[1]),
            self.0[2].max(other.0[2]),
            self.0[3].max(other.0[3]),
        ])
    }
    fn empty(self) -> bool {
        self.0[0] > self.0[2] || self.0[1] > self.0[3]
    }
}

/// Return one materialization bit per node. Bounds include overflowing descendants;
/// nested scroll offsets/clips compose in visual coordinates. Invalid topology or
/// geometry is rejected so adapters can conservatively keep the entire tree.
pub fn materialization_plan(nodes: &[CullNode]) -> Result<Vec<bool>, &'static str> {
    if nodes.len() > 100_000 {
        return Err("viewport node budget exceeded");
    }
    let mut offsets = vec![[0.0; 2]; nodes.len()];
    let mut depths = vec![0usize; nodes.len()];
    let mut clips: Vec<Option<Rect>> = vec![None; nodes.len()];
    let mut bounds = Vec::with_capacity(nodes.len());
    let mut preserved = vec![false; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        preserved[index] = node.preserve_subtree;
        if node.frame.iter().any(|v| !v.is_finite() || v.abs() > 1e12)
            || node.frame[2] < 0.0
            || node.frame[3] < 0.0
        {
            return Err("invalid viewport frame");
        }
        if let Some(parent) = node.parent {
            if parent >= index {
                return Err("invalid viewport parent");
            }
            depths[index] = depths[parent] + 1;
            if depths[index] > 128 {
                return Err("viewport depth budget exceeded");
            }
            offsets[index] = offsets[parent];
            preserved[index] |= preserved[parent];
            clips[index] = clips[parent];
            if let Some(viewport) = nodes[parent].viewport {
                let x = nodes[parent].frame[0] - offsets[parent][0];
                let y = nodes[parent].frame[1] - offsets[parent][1];
                let mut region = [x, y, x + viewport.size[0], y + viewport.size[1]];
                for axis in 0..2 {
                    let margin = (viewport.size[axis] * 0.5).max(64.0);
                    let lead = (viewport.velocity[axis] * 0.12 * (-viewport.elapsed / 0.15).exp())
                        .clamp(-viewport.size[axis] * 2.0, viewport.size[axis] * 2.0);
                    region[axis] -= margin + (-lead).max(0.0);
                    region[axis + 2] += margin + lead.max(0.0);
                    offsets[index][axis] += viewport.offset[axis];
                }
                clips[index] =
                    Some(clips[index].map_or(Rect(region), |clip| clip.intersection(Rect(region))));
            }
        }
        if let Some(viewport) = node.viewport {
            if viewport
                .offset
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > 1e12)
                || viewport
                    .size
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0.0 || *v > 1e12)
                || viewport.velocity.iter().any(|v| !v.is_finite())
                || !viewport.elapsed.is_finite()
                || viewport.elapsed < 0.0
            {
                return Err("invalid scroll viewport");
            }
        }
        let [x, y, width, height] = node.frame;
        bounds.push(Rect([
            x - offsets[index][0],
            y - offsets[index][1],
            x + width - offsets[index][0],
            y + height - offsets[index][1],
        ]));
    }
    for index in (0..nodes.len()).rev() {
        if let Some(parent) = nodes[index].parent {
            bounds[parent] = bounds[parent].union(bounds[index]);
        }
    }
    let mut retained: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            node.pinned || preserved[index]
                || clips[index].is_none_or(|clip| !clip.empty() && clip.intersects(bounds[index]))
        })
        .collect();
    for index in (0..nodes.len()).rev() {
        if retained[index] {
            if let Some(parent) = nodes[index].parent {
                retained[parent] = true;
            }
        }
    }
    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(parent: Option<usize>, y: f64) -> CullNode {
        CullNode {
            parent,
            frame: [0.0, y, 100.0, 40.0],
            viewport: None,
            pinned: false,
            preserve_subtree: false,
        }
    }
    fn scroll(parent: Option<usize>, y: f64, offset: f64) -> CullNode {
        CullNode {
            viewport: Some(ScrollViewport {
                offset: [0.0, offset],
                size: [100.0, 100.0],
                velocity: [0.0; 2],
                elapsed: 0.0,
            }),
            ..node(parent, y)
        }
    }
    #[test]
    fn transformed_subtrees_keep_descendants_without_pinning_siblings() {
        let mut nodes = [scroll(None, 0.0, 500.0), node(Some(0), 2000.0),
            node(Some(1), 2100.0), node(Some(2), 2200.0), node(Some(0), 3000.0)];
        nodes[1].preserve_subtree = true;
        assert_eq!(materialization_plan(&nodes).unwrap(), [true, true, true, true, false]);
        nodes[1].preserve_subtree = false;
        assert_eq!(materialization_plan(&nodes).unwrap(), [true, false, false, false, false]);
    }
    #[test]
    fn scroll_keeps_layout_owners_and_overflowing_children() {
        let nodes = [
            scroll(None, 0.0, 500.0),
            node(Some(0), 0.0),
            node(Some(1), 520.0),
            node(Some(0), 1000.0),
        ];
        assert_eq!(
            materialization_plan(&nodes).unwrap(),
            [true, true, true, false]
        );
    }
    #[test]
    fn nested_offsets_and_clips_compose_and_pins_keep_ancestors() {
        let mut nodes = [
            scroll(None, 0.0, 500.0),
            scroll(Some(0), 500.0, 200.0),
            node(Some(1), 720.0),
            node(Some(1), 1400.0),
            node(Some(0), 1000.0),
        ];
        assert_eq!(
            materialization_plan(&nodes).unwrap(),
            [true, true, true, false, false]
        );
        nodes[3].pinned = true;
        assert_eq!(
            materialization_plan(&nodes).unwrap(),
            [true, true, true, true, false]
        );
    }
    #[test]
    fn urgent_lead_decays_and_reverses_without_retaining_far_rows() {
        let mut nodes = [
            scroll(None, 0.0, 500.0),
            node(Some(0), 780.0),
            node(Some(0), 240.0),
            node(Some(0), 10_000.0),
        ];
        nodes[0].viewport.as_mut().unwrap().velocity[1] = 10_000.0;
        assert_eq!(
            materialization_plan(&nodes).unwrap(),
            [true, true, false, false]
        );
        nodes[0].viewport.as_mut().unwrap().elapsed = 2.0;
        assert_eq!(
            materialization_plan(&nodes).unwrap(),
            [true, false, false, false]
        );
        nodes[0].viewport.as_mut().unwrap().elapsed = 0.0;
        nodes[0].viewport.as_mut().unwrap().velocity[1] = -10_000.0;
        assert_eq!(
            materialization_plan(&nodes).unwrap(),
            [true, false, true, false]
        );
    }
    #[test]
    fn malformed_geometry_or_topology_is_rejected() {
        assert!(materialization_plan(&[node(Some(0), 0.0)]).is_err());
        let mut nodes = [scroll(None, 0.0, 0.0)];
        nodes[0].viewport.as_mut().unwrap().velocity[0] = f64::NAN;
        assert!(materialization_plan(&nodes).is_err());
    }
}
