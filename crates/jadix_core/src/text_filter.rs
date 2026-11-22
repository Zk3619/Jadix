//! Cached, case-insensitive substring filtering for declarative lists.

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Non-overlapping substring matches in the original text's UTF-8 coordinates.
/// Uses the same Unicode lowercase semantics as [`TextFilter`], without trimming,
/// canonical normalization, or full case folding (for example, `ß` is not `ss`).
/// Empty queries yield no ranges. Matches expand to whole grapheme clusters;
/// overlapping expansions merge, retaining the original spelling for painting.
pub fn substring_match_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let folded = text.to_lowercase();
    let query = query.to_lowercase();
    let matches: Vec<_> = folded
        .match_indices(query.as_str())
        .map(|(start, matched)| start..start + matched.len())
        .collect();
    if matches.is_empty() || (text.is_ascii() && !text.contains("\r\n")) {
        return matches;
    }

    // Consume the whole-string lowercase result to retain contextual final sigma.
    // Lowercasing expands scalars (İ -> i + dot); final sigma substitutes one scalar.
    let mut chars = folded.char_indices().peekable();
    let mut clusters = Vec::new();
    for (start, cluster) in text.grapheme_indices(true) {
        let folded_start = chars.peek().map_or(folded.len(), |(offset, _)| *offset);
        for _ in cluster.chars().flat_map(char::to_lowercase) {
            chars.next();
        }
        let folded_end = chars.peek().map_or(folded.len(), |(offset, _)| *offset);
        clusters.push((folded_start..folded_end, start..start + cluster.len()));
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for matched in matches {
        let first = clusters.partition_point(|(range, _)| range.end <= matched.start);
        let last = clusters.partition_point(|(range, _)| range.start < matched.end) - 1;
        let range = clusters[first].1.start..clusters[last].1.end;
        if let Some(previous) = ranges
            .last_mut()
            .filter(|previous| previous.end > range.start)
        {
            previous.end = previous.end.max(range.end);
        } else {
            ranges.push(range);
        }
    }
    ranges
}

#[derive(Default)]
struct Entry {
    fields: Vec<(String, String)>,
    matched: bool,
}

/// Retains normalized fields and match results across complete list passes.
///
/// Call [`Self::begin`], visit every candidate in order with [`Self::matches`],
/// then call [`Self::finish`]. Fields are matched separately, without trimming
/// the query. Changed or reordered candidates are checked against their current
/// text; removed candidates are released at the end of the pass.
#[derive(Default)]
pub struct TextFilter {
    query: String,
    entries: Vec<Entry>,
    cursor: usize,
    complete: bool,
    active: bool,
    unchanged: bool,
    narrowing: bool,
    #[cfg(test)]
    searched: usize,
}

impl TextFilter {
    /// Prepare one Unicode lowercase query for the next list pass.
    pub fn begin(&mut self, query: &str) {
        let query = query.to_lowercase();
        self.unchanged = self.complete && query == self.query;
        self.narrowing = self.complete && query.contains(&self.query);
        self.query = query;
        self.cursor = 0;
        self.complete = false;
        self.active = true;
        #[cfg(test)]
        {
            self.searched = 0;
        }
    }

    /// Whether a list pass is currently collecting candidates.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Match a candidate's label, value, keywords, or other independent fields.
    pub fn matches<'a>(&mut self, fields: impl Iterator<Item = &'a str> + Clone) -> bool {
        if self.cursor == self.entries.len() {
            self.entries.push(Entry::default());
        }
        let entry = &mut self.entries[self.cursor];
        self.cursor += 1;
        let changed = !fields
            .clone()
            .eq(entry.fields.iter().map(|(text, _)| text.as_str()));
        if changed {
            entry.fields = fields
                .map(|text| (text.to_owned(), text.to_lowercase()))
                .collect();
        }
        if self.query.is_empty() {
            entry.matched = true;
        } else if changed || (!self.unchanged && (!self.narrowing || entry.matched)) {
            #[cfg(test)]
            {
                self.searched += 1;
            }
            entry.matched = entry
                .fields
                .iter()
                .any(|(_, text)| text.contains(&self.query));
        }
        entry.matched
    }

    /// Complete the pass and release cached candidates that were removed.
    pub fn finish(&mut self) {
        self.entries.truncate(self.cursor);
        self.complete = true;
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{TextFilter, substring_match_ranges};
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn substring_ranges_agree_with_filter_and_preserve_graphemes() {
        let mut filter = TextFilter::default();
        for (text, query, expected) in [
            ("中文 ALpha alpha🙂", "aLP", vec!["ALp", "alp"]),
            ("ÉCOLE école", "éCO", vec!["ÉCO", "éco"]),
            ("İstanbul", "i", vec!["İ"]),
            ("İstanbul", "\u{307}", vec!["İ"]),
            ("İstanbul", "i\u{307}s", vec!["İs"]),
            ("İİİ", "\u{307}i", vec!["İİİ"]),
            ("ΟΣ ΟΣΑ", "ος", vec!["ΟΣ"]),
            ("e\u{301}cho", "E", vec!["e\u{301}"]),
            ("e\u{301}cho", "é", vec![]),
            ("👩‍💻 dev", "💻", vec!["👩‍💻"]),
            ("\r\nx", "\n", vec!["\r\n"]),
            ("aaaaa", "aa", vec!["aa", "aa"]),
            ("Straße", "SS", vec![]),
            ("Straße", "ß", vec!["ß"]),
            ("Alpha", "", vec![]),
            ("Alpha", "missing", vec![]),
            ("", "a", vec![]),
        ] {
            let ranges = substring_match_ranges(text, query);
            assert_eq!(
                ranges
                    .iter()
                    .map(|range| &text[range.clone()])
                    .collect::<Vec<_>>(),
                expected,
                "{text:?} / {query:?}",
            );
            assert!(ranges.windows(2).all(|pair| pair[0].end <= pair[1].start));
            let boundaries: Vec<_> = text
                .grapheme_indices(true)
                .map(|(start, _)| start)
                .chain(std::iter::once(text.len()))
                .collect();
            for range in &ranges {
                assert!(boundaries.contains(&range.start) && boundaries.contains(&range.end));
            }
            filter.begin(query);
            assert_eq!(
                filter.matches([text].into_iter()),
                query.is_empty() || !ranges.is_empty()
            );
            filter.finish();
        }
    }

    #[test]
    fn cached_filter_tracks_edits_and_narrows_five_thousand_candidates() {
        let mut filter = TextFilter::default();
        let mut rows: Vec<Vec<String>> = (0..5000)
            .map(|i| vec![format!("Entry {i:04}"), format!("id-{i}"), "ÉCOLE".into()])
            .collect();
        for query in [
            "", "49", "490", "490", "49", "Éco", "éCO", " Entry", "id-", "missing",
        ] {
            filter.begin(query);
            for row in &rows {
                assert_eq!(
                    filter.matches(row.iter().map(String::as_str)),
                    row.iter()
                        .any(|field| field.to_lowercase().contains(&query.to_lowercase()))
                );
            }
            filter.finish();
            match query {
                "490" if filter.unchanged => assert_eq!(filter.searched, 0),
                "490" => assert!(filter.searched < 200),
                "éCO" => assert_eq!(filter.searched, 0),
                _ => {}
            }
        }
        // A previously rejected row can start matching even while the query narrows.
        rows[0] = vec!["Missing now".into()];
        rows.swap(1, 4999);
        rows.truncate(20);
        filter.begin("missing n");
        assert!(filter.matches(rows[0].iter().map(String::as_str)));
        for row in &rows[1..] {
            assert!(!filter.matches(row.iter().map(String::as_str)));
        }
        filter.finish();
        assert_eq!(filter.entries.len(), 20);
        // Fields never match across boundaries, and abandoned passes aren't reusable.
        filter.begin("ab");
        assert!(!filter.matches(["a", "b"].into_iter()));
        filter.begin("a");
        assert!(filter.matches(["a", "b"].into_iter()));
        filter.finish();
    }
}
