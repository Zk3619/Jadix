use super::*;

#[test]
fn diff_view_builder_has_one_shared_scrollport_in_split_mode() {
    let document = DiffDocument::new("old\n", "new\n");
    let view: Element = diff_view("diff-split-test", document.clone())
        .mode(DiffMode::Split)
        .viewport_h(220.0)
        .selectable(false)
        .into();
    assert_eq!(
        view.attrs.get("data-mode").map(String::as_str),
        Some("split")
    );
    assert_eq!(
        view.attrs.get("data-slot").map(String::as_str),
        Some("diff-view")
    );
    let row = diff_row(
        "row",
        &document,
        document.rows(DiffMode::Split)[0],
        DiffMode::Split,
        true,
        22.0,
    );
    assert_eq!(row.children.len(), 2);
    assert_eq!(
        row.children[0].attrs.get("data-change").map(String::as_str),
        Some("delete")
    );
    assert_eq!(
        row.children[1].attrs.get("data-change").map(String::as_str),
        Some("insert")
    );
}
