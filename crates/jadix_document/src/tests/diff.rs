use super::*;

#[test]
fn inline_changes_keep_absolute_graphemes_and_line_endings() {
    let original = "same\r\n\te\u{301} 👩‍💻甲\r\nkeep\nremoved\n";
    let modified = "same\r\n\to\u{308} 👩‍🚀乙\r\nkeep\n";
    let diff = DiffDocument::new(original, modified);
    let old: Vec<_> = diff
        .original_inline_changes()
        .iter()
        .map(|r| &original[r.clone()])
        .collect();
    let new: Vec<_> = diff
        .modified_inline_changes()
        .iter()
        .map(|r| &modified[r.clone()])
        .collect();
    assert_eq!(old, ["e\u{301}", "👩‍💻甲"]);
    assert_eq!(new, ["o\u{308}", "👩‍🚀乙"]);
    assert!(!diff.has_coarse_inline_changes());
    let inserted = DiffDocument::new("ab\r\n", "axb\r\n");
    assert!(inserted.original_inline_changes().is_empty());
    assert_eq!(inserted.modified_inline_changes(), &[1..2]);
    let removed = DiffDocument::new("axb", "ab");
    assert_eq!(removed.original_inline_changes(), &[1..2]);
    assert!(removed.modified_inline_changes().is_empty());
    let endings = DiffDocument::new("a\r\n", "a\n");
    assert!(endings.original_inline_changes().is_empty());
    assert!(endings.modified_inline_changes().is_empty());
}

#[test]
fn oversized_inline_pairs_fall_back_without_including_crlf() {
    let original = format!("{}\r\n", "🙂".repeat(3_000));
    let modified = format!("{}\r\n", "👩".repeat(3_000));
    let diff = DiffDocument::new(original.clone(), modified.clone());
    assert!(diff.has_coarse_inline_changes());
    assert_eq!(diff.original_inline_changes(), &[0..original.len() - 2]);
    assert_eq!(diff.modified_inline_changes(), &[0..modified.len() - 2]);
    assert_eq!(diff.rows(DiffMode::Split).len(), 1);
    assert_eq!(diff.original().text(), original);
    assert_eq!(diff.modified().text(), modified);
}

#[test]
fn unified_and_split_keep_source_line_coordinates() {
    let diff = DiffDocument::new("a\nb\nc\n", "a\nx\ny\nc\n");
    assert_eq!(diff.rows(DiffMode::Unified).len(), 5);
    let split = diff.rows(DiffMode::Split);
    assert_eq!(split.len(), 4);
    assert_eq!(
        split[1],
        DiffRow {
            kind: DiffLineKind::Replace,
            original_line: Some(1),
            modified_line: Some(1)
        }
    );
    assert_eq!(
        split[2],
        DiffRow {
            kind: DiffLineKind::Insert,
            original_line: None,
            modified_line: Some(2)
        }
    );
    assert_eq!(split[3].original_line, Some(2));
    assert_eq!(split[3].modified_line, Some(3));
    assert!(!diff.is_identical());
}

#[test]
fn empty_sources_and_final_newline_are_not_phantom_lines() {
    assert!(DiffDocument::new("", "").rows(DiffMode::Unified).is_empty());
    assert_eq!(DiffDocument::new("", "\n").rows(DiffMode::Split).len(), 1);
    assert!(!DiffDocument::new("a", "a\n").is_identical());
    assert!(!DiffDocument::new("a\r\n", "a\n").is_identical());
    assert!(DiffDocument::new("\u{4e2d}\n", "\u{4e2d}\n").is_identical());
    let removed = DiffDocument::new("a\nb\n", "");
    assert!(removed
        .rows(DiffMode::Split)
        .iter()
        .all(|row| row.modified_line.is_none()));
}

#[test]
fn rows_reconstruct_both_sources_including_line_endings() {
    let originals = [
        "",
        "a",
        "a\n",
        "a\r\n\n",
        "a\rb\rc",
        "a\nb\na\n",
        "\u{4e2d}\n\u{6587}",
    ];
    for original in originals {
        for modified in originals {
            let diff = DiffDocument::new(original, modified);
            for mode in [DiffMode::Unified, DiffMode::Split] {
                let old: String = diff
                    .rows(mode)
                    .iter()
                    .filter_map(|row| row.original_line)
                    .map(|line| {
                        &diff.original().text()[diff.original().line_full_range(line).unwrap()]
                    })
                    .collect();
                let new: String = diff
                    .rows(mode)
                    .iter()
                    .filter_map(|row| row.modified_line)
                    .map(|line| {
                        &diff.modified().text()[diff.modified().line_full_range(line).unwrap()]
                    })
                    .collect();
                assert_eq!(old, original);
                assert_eq!(new, modified);
            }
        }
    }
}
