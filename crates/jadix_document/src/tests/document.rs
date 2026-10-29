use super::*;

#[test]
fn indexes_empty_trailing_crlf_and_unicode_lines() {
    let document = TextDocument::new("alpha\r\n\u{4e2d}\u{6587}\n\n");
    assert_eq!(document.line_count(), 4);
    assert_eq!(document.line(0), Some("alpha"));
    assert_eq!(document.line(1), Some("\u{4e2d}\u{6587}"));
    assert_eq!(document.line(2), Some(""));
    assert_eq!(document.line(3), Some(""));
    assert_eq!(document.line(4), None);
    assert_eq!(document.line_full_range(0), Some(0..7));
    assert_eq!(document.line_at_byte(7), Some(1));
    assert_eq!(document.line_at_byte(8), None);
    assert_eq!(document.line_at_byte(document.len()), Some(3));
    assert_eq!(document.line_at_byte(document.len() + 1), None);
    assert_eq!(TextDocument::default().line(0), Some(""));
    assert_eq!(TextDocument::new("a\rb").line(0), Some("a\rb"));
}

#[test]
fn replacement_preserves_snapshot_and_checks_byte_boundaries() {
    let original = TextDocument::new("a\n\u{4e2d}\u{6587}\n");
    assert!(Arc::ptr_eq(&original.0, &original.clone().0));
    let edited = original.replace(2..8, "next\nlast").unwrap();
    assert_eq!(original.text(), "a\n\u{4e2d}\u{6587}\n");
    assert_eq!(edited.text(), "a\nnext\nlast\n");
    assert_eq!(edited.line_count(), 4);
    assert_ne!(original.revision(), edited.revision());
    assert_eq!(
        original.replace(3..4, "").unwrap_err(),
        DocumentEditError::NotCharBoundary
    );
    let reversed = Range { start: 8, end: 2 };
    assert_eq!(
        original.replace(reversed, "").unwrap_err(),
        DocumentEditError::InvalidRange
    );
    assert_eq!(
        original.replace(0..99, "").unwrap_err(),
        DocumentEditError::InvalidRange
    );
}
