use super::*;

#[test]
fn table_is_sorted_and_disjoint() {
    for pair in RANGES.windows(2) {
        assert!(pair[0].0 <= pair[0].1);
        assert!(pair[0].1 < pair[1].0, "overlap at {:?}", pair);
    }
}

#[test]
fn known_members() {
    for c in ['©', '®', '‼', '⌚', '☀', '✅', '😀', '🚀', '🫠'] {
        assert!(is_extended_pictographic(c), "expected {c:?} to match");
    }
}

#[test]
fn known_non_members() {
    for c in ['a', '0', ' ', '中', 'π', '√', '⌫', '\u{1F1E6}'] {
        assert!(!is_extended_pictographic(c), "expected {c:?} to not match");
    }
}
