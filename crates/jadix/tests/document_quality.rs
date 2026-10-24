#![cfg(all(feature = "document-science", feature = "document-diff"))]

use jadix::prelude::*;

#[test]
fn table_cells_and_diff_columns_reserve_separate_horizontal_space() {
    fn find<'a>(node: &'a Element, attribute: &str, value: &str) -> Option<&'a Element> {
        if node.get_attr(attribute).is_some_and(|found| found == value) {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|child| find(child, attribute, value))
    }
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let mut table: Element = document_view("quality-table-layout")
            .document("| First | Second |\n| --- | --- |\n| A | B |\n")
            .into();
        let mut layout = jadix::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut table, 800.0, 300.0, [-1.0, -1.0]);
        let row = find(&table, "data-document-kind", "TableRow { header: true }").unwrap();
        assert!(
            row.children[1].layout_bounds[0]
                >= row.children[0].layout_bounds[0] + row.children[0].layout_bounds[2] - 1.0
        );
        let mut diff: Element = diff_view(
            "quality-diff-layout",
            DiffDocument::new("before\n", "after\n"),
        )
        .mode(DiffMode::Split)
        .into();
        layout.build_and_layout(&mut diff, 800.0, 300.0, [-1.0, -1.0]);
        let row = find(&diff, "data-change", "replace").unwrap();
        assert!(
            row.children[1].layout_bounds[0]
                >= row.children[0].layout_bounds[0] + row.children[0].layout_bounds[2] - 1.0
        );
        let line = find(&diff, "data-slot", "text-view-line").unwrap();
        let number = &line.children[1];
        let text = &line.children[2];
        assert!(text.layout_bounds[0] >= number.layout_bounds[0] + number.layout_bounds[2] + 7.0);
    });
}

#[test]
fn markdown_groups_are_preserved_across_code_edits() {
    let document = Document::markdown(
        "| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```rust\nlet one = 1;\nlet two = 2;\n```\n",
    );
    let table: Vec<_> = document
        .blocks()
        .iter()
        .filter(|block| matches!(block.kind, DocumentBlockKind::TableRow { .. }))
        .collect();
    assert_eq!(table[0].edit_range, table[1].edit_range);
    assert!(
        document.source().text()[table[0].edit_range.clone().unwrap()].contains("| --- | --- |")
    );
    let body = document.source().text().find("let one").unwrap();
    let code = document
        .blocks()
        .iter()
        .find(|block| block.source.contains(&body))
        .unwrap();
    let group = code.edit_range.clone().unwrap();
    assert!(document.source().text()[group.clone()].starts_with("```rust"));
    let changed = document.replace(body + 4..body + 7, "first").unwrap();
    let next = changed
        .blocks()
        .iter()
        .find(|block| block.text().contains("let two"))
        .unwrap();
    assert_eq!(next.edit_range.as_ref().unwrap().start, group.start);
    assert_eq!(next.edit_range.as_ref().unwrap().end, group.end + 2);
}

#[test]
fn xyz_water_has_visible_connectivity_and_explicit_bonds_win() {
    let molecule =
        Molecule::from_xyz("3\nwater\nO 0 0 0\nH 0.958 0 0\nH -0.239 0.927 0\n").unwrap();
    assert_eq!(molecule.bonds().len(), 2);
    assert!(molecule
        .bonds()
        .iter()
        .all(|bond| bond.inferred && bond.atoms[0] == 0));
    let element: Element = molecule_view(molecule.clone()).into();
    assert_eq!(
        element.get_attr("data-bond-count").map(String::as_str),
        Some("2")
    );
    assert!(element.children.is_empty());
    let explicit = Molecule::from_json(r#"{"atoms":[{"element":"C","position":[0,0,0]},{"element":"O","position":[1.2,0,0]}],"bonds":[{"atoms":[0,1],"order":2}]}"#).unwrap();
    let preserved = explicit.infer_bonds().unwrap();
    assert_eq!(preserved.bonds()[0].order, 2);
    assert!(!preserved.bonds()[0].inferred);
    let separated = Molecule::from_xyz("2\nfar apart\nH 0 0 0\nH 20 0 0\n").unwrap();
    assert!(separated.bonds().is_empty());
}

#[cfg(feature = "document-math")]
#[test]
fn formula_resolution_tracks_dpi_without_growing_layout() {
    jadix::style::set_gradient_raster_dpi(1.0);
    let first: Element = math_view("\\frac{1}{2} + \\sqrt{x}").into();
    let first_width = first
        .get_attr("data-raster-width")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let first_height = first
        .get_attr("data-raster-height")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    jadix::style::set_gradient_raster_dpi(2.0);
    let second: Element = math_view("\\frac{1}{2} + \\sqrt{x}").into();
    let second_width = second
        .get_attr("data-raster-width")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let second_height = second
        .get_attr("data-raster-height")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    jadix::style::set_gradient_raster_dpi(1.0);
    assert!(second_width >= first_width * 2 - 1);
    assert!(second_height >= first_height * 2 - 1);
    assert!(first.image_texture.is_some() && second.image_texture.is_some());
    assert_ne!(first.image_texture, second.image_texture);
    assert_eq!(first.style.size, second.style.size);
    let chemistry: Element = chemistry_view("2 H2 + O2 -> 2 H2O").into();
    assert!(chemistry.get_attr("data-render-error").is_none());
}

#[test]
fn html_blocks_keep_complete_source_and_semantic_identity() {
    let source = "<table>\n<tr><td colspan=\"2\">A &amp; B</td></tr>\n</table>\n";
    let document = Document::markdown(source);
    let block = document
        .blocks()
        .iter()
        .find(|block| block.kind == DocumentBlockKind::Html)
        .unwrap();
    assert_eq!(block.payload.as_deref(), Some(source));
    assert_eq!(block.edit_range, Some(0..source.len()));
}
