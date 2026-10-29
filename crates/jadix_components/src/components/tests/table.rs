use super::*;

#[test]
fn table_builds_and_sort_cycles() {
    let _el: Element = table("t1")
        .columns([
            TableColumn::new("a", "A").width(80.0),
            TableColumn::new("b", "B").width(120.0).align("end"),
        ])
        .row_count(50)
        .into();
    let s = TableSortState::default();
    let s1 = cycle_sort(&s, "a");
    assert_eq!(s1.dir, TableSortDir::Asc);
    let s2 = cycle_sort(&s1, "a");
    assert_eq!(s2.dir, TableSortDir::Desc);
}

#[test]
fn builder_ui_table_header_cell_style() {
    let el: Element = ui_table()
        .header_with(|h| {
            h.child(table_row().child(table_head().style(|b| b.text_size(20.0)).text("Invoice")))
        })
        .into();
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "table_head .style(|b| b.text_size(20)) must land on the header tree"
    );
}

#[test]
fn density_presets_and_filter_paginate() {
    let _c: Element = table_compact("tc").row_count(3).into();
    let _f: Element = table_comfortable("tf").row_count(3).into();
    let cell: Rc<dyn Fn(usize, usize) -> String> = Rc::new(|r, c| format!("{}-{}", r, c));
    let cols = [TableColumn::new("a", "A")];
    let order = filtered_row_order(5, &cols, &cell, "2-");
    assert_eq!(order, vec![2]);
    let page = TablePaginationState::new(1, 2);
    let paged = paginate_order(vec![0, 1, 2, 3, 4], page);
    assert_eq!(paged, vec![2, 3]);
}

#[test]
fn parse_table_col_width_bare_number_is_px() {
    use crate::layout::dimension::{dim_length_px, CssDim};
    let dim = parse_table_col_width("100");
    assert!(matches!(dim, CssDim::Px(v) if (v - 100.0).abs() < 1e-4));
    let px = parse_table_col_width("6.25rem");
    assert!(dim_length_px(px.as_immediate_dimension().unwrap()).unwrap() > 90.0);
}

#[test]
fn table_row_unpacks_four_heads_from_fragment() {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    frag = frag.children(vec![
        TableHead(TableHeadProps {
            col_w: Some("100".into()),
            align: None,
            children: Some(Element::from(text("Invoice"))),
            ..Default::default()
        }),
        TableHead(TableHeadProps {
            col_w: None,
            align: None,
            children: Some(Element::from(text("Status"))),
            ..Default::default()
        }),
        TableHead(TableHeadProps {
            col_w: None,
            align: None,
            children: Some(Element::from(text("Method"))),
            ..Default::default()
        }),
        TableHead(TableHeadProps {
            col_w: None,
            align: Some("end".into()),
            children: Some(Element::from(text("Amount"))),
            ..Default::default()
        }),
    ]);
    let row = TableRow(TableRowProps {
        id: "hdr".into(),
        selected: Some(false),
        on_click: Some(Handler::default()),
        children: Some(frag),
        ..Default::default()
    });
    assert_eq!(row.children.len(), 4, "each TableHead must be a row column");
    assert_eq!(
        row.children[0].attrs.get("data-slot").map(|s| s.as_str()),
        Some("table-head")
    );
}

#[test]
fn table_row_grid_template_uses_col_w_attr() {
    use taffy::prelude::Display;

    let fixed = apply_table_cell_width(r#box().into(), Some("100px"));
    let flex = apply_table_cell_width(r#box().into(), None);
    let tracks = table_row_grid_template_from_cells(&[fixed, flex]);
    assert_eq!(tracks.len(), 2);

    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    frag = frag.children(vec![
        apply_table_cell_width(r#box().into(), Some("100px")),
        apply_table_cell_width(r#box().into(), None),
    ]);
    let row = TableRow(TableRowProps {
        id: "grid-r".into(),
        selected: Some(false),
        on_click: Some(Handler::default()),
        children: Some(frag),
        ..Default::default()
    });
    assert_eq!(row.style.display, Display::Grid);
    assert_eq!(row.style.grid_template_columns.len(), 2);
}

#[test]
fn apply_cell_align_end_fills_text_leaf() {
    use crate::style::TextAlign;
    use taffy::prelude::JustifyContent;

    let cell = apply_cell_align(
        hstack().child(text("$250.00").size(14.0)).into(),
        TableAlign::End,
    );
    assert_eq!(cell.style.justify_content, Some(JustifyContent::FLEX_END));
    assert_eq!(cell.children.len(), 1);
    let leaf = &cell.children[0];
    assert_eq!(leaf.text_style.align, Some(TextAlign::End));
}

#[test]
fn table_head_amount_align_end() {
    use crate::style::TextAlign;
    use taffy::prelude::JustifyContent;

    let head = TableHead(TableHeadProps {
        col_w: None,
        align: Some("end".into()),
        children: Some(Element::from(text("Amount"))),
        ..Default::default()
    });
    assert_eq!(head.style.justify_content, Some(JustifyContent::FLEX_END));
    assert_eq!(head.children.len(), 1);
    let leaf = &head.children[0];
    assert_eq!(leaf.text_style.align, Some(TextAlign::End));
}

#[test]
fn composite_rows_share_header_column_tracks() {
    use taffy::prelude::Display;

    let header = TableHeader(TableHeaderProps {
        children: Some(TableRow(TableRowProps {
            id: "h".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some({
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag = frag.children(vec![
                    TableHead(TableHeadProps {
                        col_w: Some("100px".into()),
                        align: None,
                        children: Some(Element::from(text("A"))),
                        ..Default::default()
                    }),
                    TableHead(TableHeadProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("B"))),
                        ..Default::default()
                    }),
                    TableHead(TableHeadProps {
                        col_w: Some("96px".into()),
                        align: Some("end".into()),
                        children: Some(Element::from(text("C"))),
                        ..Default::default()
                    }),
                ]);
                frag
            }),
            ..Default::default()
        })),
        ..Default::default()
    });
    let body = TableBody(TableBodyProps {
        children: Some(TableRow(TableRowProps {
            id: "b".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some({
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag = frag.children(vec![
                    TableCell(TableCellProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("aaaaaa"))),
                        ..Default::default()
                    }),
                    TableCell(TableCellProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("b"))),
                        ..Default::default()
                    }),
                    TableCell(TableCellProps {
                        col_w: None,
                        align: Some("end".into()),
                        children: Some(Element::from(text("1"))),
                        ..Default::default()
                    }),
                ]);
                frag
            }),
            ..Default::default()
        })),
        ..Default::default()
    });
    let mut kids = Element::new();
    kids.set_attr("jadix_fragment", "1");
    kids = kids.children(vec![header, body]);
    let el = Table(TableProps {
        children: Some(kids),
        ..Default::default()
    });
    let inner = &el.children[0];
    let header_row = find_first_table_row(
        inner
            .children
            .iter()
            .find(|c| slot_of(c) == Some(SLOT_HEADER))
            .expect("header"),
    )
    .expect("header row");
    let body_row = find_first_table_row(
        inner
            .children
            .iter()
            .find(|c| slot_of(c) == Some(SLOT_BODY))
            .expect("body"),
    )
    .expect("body row");
    assert_eq!(header_row.style.display, Display::Grid);
    assert_eq!(body_row.style.display, Display::Grid);
    assert_eq!(
        header_row.style.grid_template_columns.len(),
        body_row.style.grid_template_columns.len()
    );
    assert_eq!(
        header_row.style.grid_template_columns, body_row.style.grid_template_columns,
        "body rows must reuse the header column tracks"
    );
}

#[test]
fn composite_table_column_x_aligns_across_rows() {
    use crate::core::reactivity::{create_effect, create_root};
    use crate::core::tree::LayoutTree;

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let header = TableHeader(TableHeaderProps {
                children: Some(TableRow(TableRowProps {
                    id: "lh".into(),
                    selected: Some(false),
                    on_click: Some(Handler::default()),
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("jadix_fragment", "1");
                        frag = frag.children(vec![
                            TableHead(TableHeadProps {
                                col_w: Some("100px".into()),
                                align: None,
                                children: Some(Element::from(text("Inv"))),
                                ..Default::default()
                            }),
                            TableHead(TableHeadProps {
                                col_w: None,
                                align: Some("end".into()),
                                children: Some(Element::from(text("Amt"))),
                                ..Default::default()
                            }),
                        ]);
                        frag
                    }),
                    ..Default::default()
                })),
                ..Default::default()
            });
            let body = TableBody(TableBodyProps {
                children: Some(TableRow(TableRowProps {
                    id: "lb".into(),
                    selected: Some(false),
                    on_click: Some(Handler::default()),
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("jadix_fragment", "1");
                        frag = frag.children(vec![
                            TableCell(TableCellProps {
                                col_w: None,
                                align: None,
                                children: Some(Element::from(text("INV001"))),
                                ..Default::default()
                            }),
                            TableCell(TableCellProps {
                                col_w: None,
                                align: Some("end".into()),
                                children: Some(Element::from(text("$1"))),
                                ..Default::default()
                            }),
                        ]);
                        frag
                    }),
                    ..Default::default()
                })),
                ..Default::default()
            });
            let mut kids = Element::new();
            kids.set_attr("jadix_fragment", "1");
            kids = kids.children(vec![header, body]);
            let table = Table(TableProps {
                children: Some(kids),
                ..Default::default()
            });
            let mut root = Element::new().w(400.0).h(200.0).child(table);
            let mut tree = LayoutTree::new();
            tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);

            let inner = &root.children[0].children[0];
            let header_row = find_first_table_row(
                inner
                    .children
                    .iter()
                    .find(|c| slot_of(c) == Some(SLOT_HEADER))
                    .unwrap(),
            )
            .unwrap();
            let body_row = find_first_table_row(
                inner
                    .children
                    .iter()
                    .find(|c| slot_of(c) == Some(SLOT_BODY))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(header_row.children.len(), 2);
            assert_eq!(body_row.children.len(), 2);
            for i in 0..2 {
                let hx = header_row.children[i].layout_bounds[0];
                let bx = body_row.children[i].layout_bounds[0];
                let hw = header_row.children[i].layout_bounds[2];
                let bw = body_row.children[i].layout_bounds[2];
                assert!((hx - bx).abs() < 1.0, "col {i} x: header={hx} body={bx}");
                assert!((hw - bw).abs() < 1.0, "col {i} w: header={hw} body={bw}");
            }
            let amt = &body_row.children[1];
            assert!(
                amt.layout_bounds[2] > 200.0,
                "end column should grow, got {}",
                amt.layout_bounds[2]
            );
            let leaf = &amt.children[0];
            let leaf_right = leaf.layout_bounds[0] + leaf.layout_bounds[2];
            let cell_right = amt.layout_bounds[0] + amt.layout_bounds[2];
            assert!(
                    (cell_right - leaf_right).abs() < 12.0,
                    "end-aligned text should sit on the cell's trailing edge; leaf_right={leaf_right} cell_right={cell_right}"
                );
        });
    });
}

#[test]
fn composite_table_builds() {
    let head = TableHead(TableHeadProps {
        col_w: Some("100".into()),
        align: None,
        children: Some(Element::from(text("Invoice"))),
        ..Default::default()
    });
    let header = TableHeader(TableHeaderProps {
        children: Some(TableRow(TableRowProps {
            id: "tr-h".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(head),
            ..Default::default()
        })),
        ..Default::default()
    });
    let body = TableBody(TableBodyProps {
        children: Some(TableRow(TableRowProps {
            id: "tr-b".into(),
            selected: Some(true),
            on_click: Some(Handler::default()),
            children: Some(TableCell(TableCellProps {
                col_w: None,
                align: None,
                children: Some(Element::from(text("INV001"))),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    });
    let el = Table(TableProps {
        children: Some(vstack().child(header).child(body).into()),
        ..Default::default()
    });
    assert_eq!(el.attrs.get("jadix_table").map(|s| s.as_str()), Some("1"));
}

#[test]
fn table_cell_picks_up_stylesheet_slot_rule() {
    use crate::style::{clear_stylesheets, parse_stylesheet, register_stylesheet};
    clear_stylesheets();
    let sheet = parse_stylesheet(
        r#"
[[rule]]
selectors = ["[data-slot=table-cell]"]
classes = "underline"
"#,
    )
    .unwrap();
    register_stylesheet(sheet).unwrap();
    let cell = TableCell(TableCellProps {
        col_w: None,
        align: None,
        children: Some(Element::from(text("INV001"))),
        ..Default::default()
    });
    let cs = cell.class_set.as_ref().expect("class_set");
    assert!(
        cs.base.iter().any(|c| c == "underline"),
        "TableCell must bind [data-slot=table-cell]; got {:?}",
        cs.base
    );
    clear_stylesheets();
}
