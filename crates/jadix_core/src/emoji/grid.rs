use super::EmojiMatch;
use crate::presentation::NavigationKey;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum EmojiRow {
    Group(u8),
    Items(std::ops::Range<usize>),
}

/// Shared section layout and keyboard geometry, independent of mounted rows.
#[derive(Clone, Debug)]
pub struct EmojiGrid {
    pub items: Vec<EmojiMatch>,
    pub rows: Vec<EmojiRow>,
    item_rows: Vec<usize>,
    grouped: bool,
}
impl EmojiGrid {
    pub fn new(items: Vec<EmojiMatch>, columns: usize) -> Self {
        Self::for_query(items, columns, "")
    }
    /// A search is one continuous grid, independent of catalog sections.
    pub fn for_query(items: Vec<EmojiMatch>, columns: usize, query: &str) -> Self {
        Self::with_headers(items, columns, query, |_| true)
    }
    /// Section boundaries remain intact even when the application omits headings.
    pub fn with_headers(items: Vec<EmojiMatch>, columns: usize, query: &str, heading: impl Fn(u8) -> bool) -> Self {
        assert!((1..=64).contains(&columns), "invalid emoji columns");
        let grouped = query.trim().is_empty();
        let mut rows = Vec::new();
        let mut item_rows = vec![0; items.len()];
        let mut start = 0;
        while start < items.len() {
            let section = items[start].section;
            let end = if grouped { start + items[start..].iter().take_while(|e| e.section == section).count() } else { items.len() };
            if grouped && heading(section) { rows.push(EmojiRow::Group(section)); }
            for first in (start..end).step_by(columns) {
                let last = (first + columns).min(end);
                item_rows[first..last].fill(rows.len());
                rows.push(EmojiRow::Items(first..last));
            }
            start = end;
        }
        Self { items, rows, item_rows, grouped }
    }
    pub fn row_for(&self, index: usize) -> Option<usize> { self.item_rows.get(index).copied() }
    /// Section at the actual viewport start, including a visible heading.
    pub fn section_for_row(&self, row: usize) -> Option<u8> {
        match self.rows.get(row)? {
            EmojiRow::Group(section) => Some(*section),
            EmojiRow::Items(range) => self.items.get(range.start).map(|item| item.section),
        }
    }
    pub fn section_start(&self, section: u8) -> Option<usize> {
        self.items.iter().position(|item| item.section == section)
    }
    pub fn section_row(&self, section: u8) -> Option<usize> {
        if !self.grouped { return None; }
        self.rows.iter().position(|row| *row == EmojiRow::Group(section))
            .or_else(|| self.section_start(section).and_then(|index| self.row_for(index)))
    }
    pub fn index_of(&self, emoji: &str) -> Option<usize> {
        let identity = super::entry(emoji)?.text;
        self.items.iter().position(|item| item.base == identity)
    }
    pub fn navigate(&self, current: Option<usize>, key: NavigationKey, rtl: bool, page_rows: usize) -> Option<usize> {
        use NavigationKey::*;
        if self.items.is_empty() { return None; }
        let last = self.items.len() - 1;
        let current = current.filter(|i| *i <= last);
        let index = current.unwrap_or(0);
        let row = self.item_rows[index];
        let column = match &self.rows[row] { EmojiRow::Items(range) => index - range.start, _ => 0 };
        let vertical = |forward: bool, steps: usize| {
            let mut result = index;
            let candidates: Box<dyn Iterator<Item = &EmojiRow>> = if forward {
                Box::new(self.rows[row + 1..].iter())
            } else { Box::new(self.rows[..row].iter().rev()) };
            for (count, range) in candidates.filter_map(|r| match r { EmojiRow::Items(r) => Some(r), _ => None }).enumerate() {
                result = (range.start + column).min(range.end - 1);
                if count + 1 >= steps.max(1) { break; }
            }
            result
        };
        Some(match key {
            Home => 0, End => last,
            Up if current.is_some() => vertical(false, 1),
            Down if current.is_some() => vertical(true, 1),
            PageUp => vertical(false, page_rows), PageDown => vertical(true, page_rows),
            Left if !rtl => index.saturating_sub(1), Right if rtl => index.saturating_sub(1),
            Left | Right => if current.is_some() { (index + 1).min(last) } else { 0 },
            _ => index,
        })
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn navigation_crosses_headers_and_short_rows_without_mounting_cells() {
        let mut search = super::super::EmojiSearch::default();
        search.search(&super::super::EmojiQuery::default()).unwrap();
        let grid = EmojiGrid::new(search.matches().cloned().collect(), 8);
        assert!(grid.items.len() > 1800);
        let next_group = grid.section_start(1).unwrap();
        assert!(matches!(grid.rows[grid.row_for(next_group).unwrap()-1], EmojiRow::Group(1)));
        assert_eq!(grid.navigate(Some(next_group - 1), NavigationKey::Right, false, 6), Some(next_group));
        assert_eq!(grid.navigate(Some(next_group), NavigationKey::Right, true, 6), Some(next_group - 1));
        assert_eq!(grid.navigate(None, NavigationKey::Down, false, 6), Some(0));
        assert_eq!(grid.navigate(Some(0), NavigationKey::PageDown, false, 6), Some(48));
        assert_eq!(grid.navigate(Some(grid.items.len()-1), NavigationKey::Down, false, 6), Some(grid.items.len()-1));
    }
    #[test]
    fn omitted_headings_preserve_section_navigation_and_search_has_no_sections() {
        let mut search = super::super::EmojiSearch::default();
        search.search(&super::super::EmojiQuery::default()).unwrap();
        let items: Vec<_> = [0, 1].into_iter().flat_map(|section|
            search.matches().filter(move |item| item.section == section).take(2).cloned()).collect();
        let grid = EmojiGrid::with_headers(items.clone(), 3, "", |_| false);
        assert_eq!(grid.rows, vec![EmojiRow::Items(0..2), EmojiRow::Items(2..4)]);
        assert_eq!(grid.section_row(1), Some(1));
        assert_eq!(grid.section_for_row(1), Some(1));
        assert_eq!(grid.navigate(Some(0), NavigationKey::Down, false, 1), Some(2));
        assert_eq!(EmojiGrid::for_query(items, 3, "match").section_row(1), None);
    }
    #[test]
    fn query_grid_packs_sections_without_heading_rows() {
        let mut search = super::super::EmojiSearch::default();
        search.search(&super::super::EmojiQuery::default()).unwrap();
        let items: Vec<_> = [0, 1].into_iter().flat_map(|section|
            search.matches().filter(move |item| item.section == section).take(2).cloned()
        ).collect();
        assert_eq!(items.len(), 4);
        let grouped = EmojiGrid::new(items.clone(), 3);
        assert_eq!(grouped.rows.len(), 4);
        let results = EmojiGrid::for_query(items, 3, "match");
        assert_eq!(results.rows, vec![EmojiRow::Items(0..3), EmojiRow::Items(3..4)]);
        assert_eq!(results.row_for(2), Some(0));
        assert_eq!(results.row_for(3), Some(1));
        assert_eq!(results.section_for_row(0), Some(0));
        assert_eq!(results.section_for_row(1), Some(1));
        assert_eq!(results.navigate(Some(0), NavigationKey::Down, false, 6), Some(3));
    }

}
