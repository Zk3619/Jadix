//! Literal Unicode search over the presented document, excluding passwords.
use super::*;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellSearchMatch {
    pub node: NodeId,
    /// UTF-8 byte range in the painted label or editable document.
    pub bytes: Range<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellSearchSummary {
    pub matches: usize,
    pub current: Option<usize>,
    /// More than 4096 matches exist; only this bounded prefix is retained.
    pub truncated: bool,
}
#[derive(Default)]
pub(super) struct Search {
    query: String,
    pub(super) dirty: bool,
    changed: BTreeSet<NodeId>,
    // Inverse accepted paint order. Only complete, untruncated searches patch.
    order: Vec<usize>,
    pub matches: Vec<CellSearchMatch>,
    pub active: Option<usize>,
    truncated: bool,
    ranges: BTreeMap<NodeId, Range<usize>>,
}
impl Search {
    pub(super) fn invalidate(&mut self) {
        self.dirty = true;
        self.changed.clear();
    }
    pub(super) fn touch(&mut self, node: NodeId) {
        if self.query.is_empty() || self.dirty {
            return;
        }
        if self.truncated || self.changed.len() == 4096 {
            self.invalidate();
        } else {
            self.changed.insert(node);
        }
    }

    pub(super) fn highlights(&self, node: NodeId, byte: usize) -> (bool, bool) {
        let Some(range) = self.ranges.get(&node) else {
            return (false, false);
        };
        let matches = &self.matches[range.clone()];
        let at = matches.partition_point(|found| found.bytes.end <= byte);
        let hit = matches
            .get(at)
            .is_some_and(|found| found.bytes.contains(&byte));
        (hit, hit && self.active == Some(range.start + at))
    }
}
impl CellSurface {
    /// Case-sensitive non-overlapping literal matches. Search includes mounted
    /// offscreen content, but never hidden/out-of-scope content or passwords.
    /// It does not query unmounted virtual rows or change editor selection.
    pub fn find(&mut self, query: &str) -> io::Result<CellSearchSummary> {
        self.live()?;
        if query.len() > 512 || query.chars().any(char::is_control) {
            return Err(invalid("search query exceeds budget or contains controls"));
        }
        if self.area.is_none() {
            return Err(invalid("layout before search"));
        }
        if self.search.query != query {
            self.search = Search {
                query: query.into(),
                dirty: true,
                ..Default::default()
            };
        }
        self.refresh_search();
        self.reveal_search();
        self.dirty = true;
        Ok(self.search_summary())
    }
    pub fn find_next(&mut self, backwards: bool) -> io::Result<CellSearchSummary> {
        self.live()?;
        self.refresh_search();
        let count = self.search.matches.len();
        if count > 0 {
            let at = self
                .search
                .active
                .unwrap_or(if backwards { 0 } else { count - 1 });
            self.search.active = Some(if backwards {
                (at + count - 1) % count
            } else {
                (at + 1) % count
            });
            self.reveal_search();
            self.dirty = true;
        }
        Ok(self.search_summary())
    }
    pub fn clear_search(&mut self) {
        if !self.search.query.is_empty() {
            self.search = Search::default();
            self.dirty = true;
        }
    }
    pub fn search_summary(&self) -> CellSearchSummary {
        CellSearchSummary {
            matches: self.search.matches.len(),
            current: self.search.active,
            truncated: self.search.truncated,
        }
    }
    pub fn search_matches(&self) -> &[CellSearchMatch] {
        &self.search.matches
    }
    pub(super) fn refresh_search(&mut self) {
        if self.search.query.is_empty() || (!self.search.dirty && self.search.changed.is_empty()) {
            return;
        }
        if !self.search.dirty {
            let changed = std::mem::take(&mut self.search.changed);
            let removed: usize = changed
                .iter()
                .filter_map(|id| self.search.ranges.get(id))
                .map(|r| r.len())
                .sum();
            let mut remaining = 4096 - (self.search.matches.len() - removed);
            let mut replacements = Vec::new();
            let mut same_counts = true;
            let mut same = true;
            let mut complete = true;
            for id in &changed {
                let Some(&index) = self.node_indices.get(id) else {
                    complete = false;
                    break;
                };
                let mut found = Vec::new();
                self.node_search_matches(index, &mut found, remaining + 1);
                if found.len() > remaining
                    || (!found.is_empty() && self.search.order[index] == usize::MAX)
                {
                    complete = false;
                    break;
                }
                let range = self.search.ranges.get(id).cloned().unwrap_or(0..0);
                let old = &self.search.matches[range.clone()];
                same_counts &= range.len() == found.len();
                same &= old == found.as_slice();
                remaining -= found.len();
                replacements.push((range, self.search.order[index], found));
            }
            if complete {
                if same {
                    return;
                }
                // Equal per-node cardinalities keep all group offsets stable.
                // Only replace those slots; selection follows its original match
                // identity once, rather than a fallback selected during this loop.
                if same_counts {
                    let active = self.search.active;
                    let selected = active.and_then(|at| self.search.matches.get(at)).cloned();
                    let mut next_active = active;
                    for (range, _, found) in replacements {
                        if active.is_some_and(|at| range.contains(&at)) {
                            next_active = selected
                                .as_ref()
                                .and_then(|old| {
                                    found
                                        .iter()
                                        .position(|m| m == old)
                                        .map(|at| range.start + at)
                                })
                                .or(Some(0));
                        }
                        #[cfg(test)]
                        search_delta_tests::match_visits(found.len());
                        self.search.matches[range].clone_from_slice(&found);
                    }
                    self.search.active = next_active;
                    return;
                }
                let mut additions: Vec<_> = replacements
                    .into_iter()
                    .flat_map(|(_, rank, found)| found.into_iter().map(move |m| (rank, m)))
                    .collect();
                // The published flat match list is bounded to 4096. Merge its
                // unchanged order with changed groups; never visit source text.
                additions.sort_by_key(|(rank, m)| (*rank, m.bytes.start));
                let mut old = self
                    .search
                    .matches
                    .iter()
                    .filter(|m| {
                        #[cfg(test)]
                        search_delta_tests::match_visits(1);
                        !changed.contains(&m.node)
                    })
                    .peekable();
                let mut new = additions.into_iter().peekable();
                let mut matches = Vec::new();
                while old.peek().is_some() || new.peek().is_some() {
                    let take_new = match (old.peek(), new.peek()) {
                        (Some(a), Some((rank, _))) => {
                            *rank < self.search.order[self.node_indices[&a.node]]
                        }
                        (None, Some(_)) => true,
                        _ => false,
                    };
                    if take_new {
                        matches.push(new.next().unwrap().1);
                    } else {
                        matches.push(old.next().unwrap().clone());
                    }
                }
                self.accept_search(matches, false);
                return;
            }
        }
        let mut matches = Vec::new();
        let mut truncated = false;
        let mut order = vec![usize::MAX; self.nodes.len()];
        for (rank, &index) in self.paint_order.iter().enumerate() {
            order[index] = rank;
            self.node_search_matches(index, &mut matches, 4097);
            if matches.len() > 4096 {
                matches.truncate(4096);
                truncated = true;
                break;
            }
        }
        self.search.order = order;
        self.search.changed.clear();
        self.accept_search(matches, truncated);
    }
    fn node_search_matches(&self, index: usize, matches: &mut Vec<CellSearchMatch>, limit: usize) {
        #[cfg(test)]
        interaction_tests::SEARCH_VISITS.with(|n| n.set(n.get() + 1));
        let node = &self.nodes[index];
        if !self.geometry[index].live
            || !self.in_scope(index)
            || matches!(
                node.descriptor.content,
                NodeContent::Input { password: true, .. }
            )
        {
            return;
        }
        let label = label(&node.descriptor.content);
        let value = self
            .fields
            .get(&node.id)
            .map_or(label.as_ref(), |f| f.editor.document().text());
        let mut boundaries = value
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(value.len()))
            .peekable();
        for (start, _) in value.match_indices(&self.search.query) {
            let end = start + self.search.query.len();
            while boundaries.peek().is_some_and(|i| *i < start) {
                boundaries.next();
            }
            if boundaries.peek() != Some(&start) {
                continue;
            }
            while boundaries.peek().is_some_and(|i| *i < end) {
                boundaries.next();
            }
            if boundaries.peek() != Some(&end) {
                continue;
            }
            matches.push(CellSearchMatch {
                node: node.id,
                bytes: start..end,
            });
            if matches.len() == limit {
                break;
            }
        }
    }
    fn accept_search(&mut self, matches: Vec<CellSearchMatch>, truncated: bool) {
        let selected = self
            .search
            .active
            .and_then(|at| self.search.matches.get(at))
            .cloned();
        let mut ranges: BTreeMap<NodeId, Range<usize>> = BTreeMap::new();
        for (at, found) in matches.iter().enumerate() {
            #[cfg(test)]
            search_delta_tests::match_visits(1);
            ranges
                .entry(found.node)
                .and_modify(|r| r.end = at + 1)
                .or_insert(at..at + 1);
        }
        self.search.active = selected
            .and_then(|old| {
                matches.iter().position(|found| {
                    #[cfg(test)]
                    search_delta_tests::match_visits(1);
                    *found == old
                })
            })
            .or_else(|| (!matches.is_empty()).then_some(0));
        self.search.matches = matches;
        self.search.truncated = truncated;
        self.search.ranges = ranges;
        self.search.dirty = false;
    }
    fn reveal_search(&mut self) {
        let Some(found) = self
            .search
            .active
            .and_then(|at| self.search.matches.get(at))
            .cloned()
        else {
            return;
        };
        let Some(index) = self.node_indices.get(&found.node).copied() else {
            return;
        };
        let label = label(&self.nodes[index].descriptor.content);
        let value = self
            .fields
            .get(&found.node)
            .map_or(label.as_ref(), |field| field.editor.document().text());
        let frame = self.geometry[index].frame;
        let mut position = (0, 0, 1);
        grid(
            value,
            false,
            (frame[2] - frame[0]).max(1) as usize,
            usize::MAX,
            |byte, _, col, row, width| {
                if byte == found.bytes.start {
                    position = (col, row, width.max(1));
                }
            },
        );
        let mut row = position.1;
        if let Some(field) = self.fields.get_mut(&found.node) {
            let height = (frame[3] - frame[1]).max(1) as usize;
            if row < field.top_row {
                field.top_row = row;
            }
            if row >= field.top_row + height {
                field.top_row = row + 1 - height;
            }
            row -= field.top_row;
        }
        self.reveal_region(
            index,
            [
                position.0 as i64,
                row as i64,
                (position.0 + position.2) as i64,
                row as i64 + 1,
            ],
        );
    }
}
