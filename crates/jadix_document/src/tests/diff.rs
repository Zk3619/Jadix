use super::*;

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
