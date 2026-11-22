//! Selection mapping for controlled native text replacements. Coordinates are UTF-16.
use unicode_segmentation::UnicodeSegmentation;

/// Preserve positions in the unchanged prefix/suffix; move positions inside a
/// replacement to its end. Never return a position inside a grapheme cluster.
pub fn remap_utf16(before: &str, after: &str, selection: [usize; 2]) -> [usize; 2] {
    let old: Vec<_> = before.graphemes(true).collect();
    let new: Vec<_> = after.graphemes(true).collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b).count();
    let units = |parts: &[&str]| parts.iter().map(|s| s.encode_utf16().count()).sum::<usize>();
    let start = units(&old[..prefix]);
    let old_end = units(&old[..old.len() - suffix]);
    let new_end = units(&new[..new.len() - suffix]);
    let old_len = units(&old);
    let mut boundaries = vec![0];
    for cluster in &new { boundaries.push(boundaries.last().unwrap() + cluster.encode_utf16().count()); }
    let map = |position: usize| {
        let position = position.min(old_len);
        let mapped = if position < start { position }
            else if position >= old_end { new_end + position - old_end }
            else { new_end };
        let index = boundaries.partition_point(|boundary| *boundary <= mapped);
        boundaries[index.saturating_sub(1)]
    };
    selection.map(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controlled_replacements_preserve_suffix_and_unicode_boundaries() {
        assert_eq!(remap_utf16("world", "hello world", [0, 5]), [6, 11]);
        assert_eq!(remap_utf16("abXYZcd", "ab!cd", [6, 3]), [4, 3]);
        assert_eq!(remap_utf16("a👨‍👩‍👧‍👦é", "a👨‍👩‍👧‍👦é", [5, 13]), [1, 12]);
        assert_eq!(remap_utf16("abcdef", "ab", [usize::MAX, 4]), [2, 2]);
        assert_eq!(remap_utf16("", "🙂", [0, 0]), [2, 2]);
    }
}
