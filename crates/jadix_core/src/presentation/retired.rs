//! Exact retired generation membership shared by presentation hosts.
//! Node generations are process-global; callers must validate the root first.
use std::collections::BTreeMap;

use crate::presentation::NodeId;

#[derive(Clone, Default)]
pub struct RetiredIds {
    intervals: BTreeMap<u64, u64>,
    count: usize,
}

impl RetiredIds {
    pub fn contains(&self, node: &NodeId) -> bool {
        let generation = node.generation();
        self.intervals
            .range(..=generation)
            .next_back()
            .is_some_and(|(_, end)| *end >= generation)
    }

    pub fn insert(&mut self, node: NodeId) {
        if self.contains(&node) {
            return;
        }
        let generation = node.generation();
        let previous = self
            .intervals
            .range(..generation)
            .next_back()
            .map(|(start, end)| (*start, *end));
        let following = self
            .intervals
            .range(generation..)
            .next()
            .map(|(start, end)| (*start, *end));
        let start = previous
            .filter(|(_, end)| end.checked_add(1) == Some(generation))
            .map_or(generation, |(start, _)| start);
        let end = match following {
            Some((next, end)) if generation.checked_add(1) == Some(next) => {
                self.intervals.remove(&next);
                end
            }
            _ => generation,
        };
        self.intervals.insert(start, end);
        self.count += 1;
    }

    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn ranges(&self) -> usize {
        self.intervals.len()
    }

    pub fn clear(&mut self) {
        self.intervals.clear();
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::RootId;

    #[test]
    fn exact_membership_survives_out_of_order_removal_and_range_merging() {
        let root = RootId::new();
        let nodes: Vec<_> = (0..10).map(|_| NodeId::new(root)).collect();
        let mut retired = RetiredIds::default();
        for index in [0, 2, 4, 6, 8] {
            retired.insert(nodes[index]);
        }
        for (index, node) in nodes.iter().enumerate() {
            assert_eq!(retired.contains(node), index % 2 == 0);
        }
        for index in [7, 3, 5, 1, 9, 9] {
            retired.insert(nodes[index]);
        }
        assert_eq!(retired.len(), 10);
        assert!(retired.ranges() <= 10);
        assert!(nodes.iter().all(|node| retired.contains(node)));
        retired.clear();
        assert_eq!(retired.len(), 0);
        assert_eq!(retired.ranges(), 0);
    }
}
