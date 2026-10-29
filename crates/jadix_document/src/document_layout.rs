//! Variable-height document indexing, independent of mounted UI nodes.

use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DocumentScrollAnchor {
    pub block: usize,
    pub offset: f64,
}

#[derive(Clone, Debug, Default)]
pub struct DocumentHeightIndex {
    heights: Vec<f64>,
    tree: Vec<f64>,
}

impl DocumentHeightIndex {
    pub fn new(heights: impl IntoIterator<Item = f64>) -> Self {
        let heights: Vec<_> = heights.into_iter().map(valid_height).collect();
        let mut index = Self {
            tree: vec![0.0; heights.len() + 1],
            heights,
        };
        for position in 1..index.tree.len() {
            index.tree[position] += index.heights[position - 1];
            let parent = position + (position & position.wrapping_neg());
            if parent < index.tree.len() {
                index.tree[parent] += index.tree[position];
            }
        }
        index
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }
    pub fn height(&self, block: usize) -> Option<f64> {
        self.heights.get(block).copied()
    }
    pub fn total(&self) -> f64 {
        self.top(self.len())
    }

    pub fn top(&self, block: usize) -> f64 {
        let mut position = block.min(self.len());
        let mut height = 0.0;
        while position > 0 {
            height += self.tree[position];
            position &= position - 1;
        }
        height
    }

    pub fn measure(&mut self, block: usize, height: f64) -> bool {
        let Some(previous) = self.heights.get_mut(block) else {
            return false;
        };
        let height = valid_height(height);
        let delta = height - *previous;
        if delta.abs() < 0.25 {
            return false;
        }
        *previous = height;
        let mut position = block + 1;
        while position < self.tree.len() {
            self.tree[position] += delta;
            position += position & position.wrapping_neg();
        }
        true
    }

    pub fn block_at(&self, offset: f64) -> usize {
        if self.is_empty() {
            return 0;
        }
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let mut position = 0;
        let mut sum = 0.0;
        let mut step = self.len().next_power_of_two();
        while step > 0 {
            let next = position + step;
            if next < self.tree.len() && sum + self.tree[next] <= offset {
                position = next;
                sum += self.tree[next];
            }
            step >>= 1;
        }
        position.min(self.len() - 1)
    }

    pub fn visible(&self, offset: f64, viewport: f64, overscan: usize) -> Range<usize> {
        if self.is_empty() {
            return 0..0;
        }
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let viewport = if viewport.is_finite() {
            viewport.max(0.0)
        } else {
            0.0
        };
        let start = self.block_at(offset).saturating_sub(overscan);
        let end = self
            .block_at(offset + viewport)
            .saturating_add(1)
            .saturating_add(overscan)
            .min(self.len());
        start..end.max(start)
    }

    pub fn anchor(&self, offset: f64) -> DocumentScrollAnchor {
        let block = self.block_at(offset);
        DocumentScrollAnchor {
            block,
            offset: (offset - self.top(block)).max(0.0),
        }
    }

    pub fn restore(&self, anchor: DocumentScrollAnchor) -> f64 {
        let block = anchor.block.min(self.len().saturating_sub(1));
        self.top(block) + anchor.offset.min(self.height(block).unwrap_or(0.0))
    }
}

fn valid_height(height: f64) -> f64 {
    if height.is_finite() {
        height.max(1.0)
    } else {
        1.0
    }
}

#[cfg(test)]
#[path = "tests/document_layout.rs"]
mod tests;
