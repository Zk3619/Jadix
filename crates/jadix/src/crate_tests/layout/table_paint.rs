#![cfg(feature = "components")]
//! Table hover / collect-skip must keep cell glyphs and row chrome.
//!
//! Gallery invoices + data_table used to flash empty then restyle: hover chrome
//! treated the row as a ghost Button (wiping `border-b`) and dest-reset the row
//! box without recapturing nested cell text.

use crate::app::damage::{paint_element_key, take_frame_paint_nodes, PaintDirty, PaintDirtyKind};
use crate::app::draw_cull::{
    node_present_capture_text_indices, node_present_scissor_logical, node_present_scissor_pad,
};
use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::button::sync_button_hover_paint_subtree;
use crate::components::table::{
    table, Table, TableBody, TableBodyProps, TableCell, TableCellProps, TableColumn, TableFooter,
    TableFooterProps, TableHead, TableHeadProps, TableHeader, TableHeaderProps, TableProps,
    TableRow, TableRowProps,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{overflow_clip_axes, Element, Handler, LayoutStatus, LayoutTree};
use crate::layout::layout_tree::RenderResult;
use crate::primitives::text::text;
use crate::style::{sync_class_variants_subtree, tick_active_css_transitions_subtree};
use std::cell::RefCell;
use std::rc::Rc;

const W: f32 = 480.0;
const H: f32 = 280.0;

fn frag(children: Vec<Element>) -> Element {
    let mut el = Element::new();
    el.set_attr("jadix_fragment", "1");
    el.children(children)
}

fn invoice_table() -> Element {
    let header = TableHeader(TableHeaderProps {
        children: Some(TableRow(TableRowProps {
            id: "inv-h".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(frag(vec![
                TableHead(TableHeadProps {
                    col_w: Some("100px".into()),
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
                    col_w: Some("96px".into()),
                    align: Some("end".into()),
                    children: Some(Element::from(text("Amount"))),
                    ..Default::default()
                }),
            ])),
            ..Default::default()
        })),
        ..Default::default()
    });
    let body = TableBody(TableBodyProps {
        children: Some(frag(vec![
            TableRow(TableRowProps {
                id: "inv-0".into(),
                selected: Some(false),
                on_click: Some(Handler::default()),
                children: Some(frag(vec![
                    TableCell(TableCellProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("INV001"))),
                        ..Default::default()
                    }),
                    TableCell(TableCellProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("Paid"))),
                        ..Default::default()
                    }),
                    TableCell(TableCellProps {
                        col_w: None,
                        align: Some("end".into()),
                        children: Some(Element::from(text("$250.00"))),
                        ..Default::default()
                    }),
                ])),
                ..Default::default()
            }),
            TableRow(TableRowProps {
                id: "inv-1".into(),
                selected: Some(false),
                on_click: Some(Handler::default()),
                children: Some(frag(vec![
                    TableCell(TableCellProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("INV002"))),
                        ..Default::default()
                    }),
                    TableCell(TableCellProps {
                        col_w: None,
                        align: None,
                        children: Some(Element::from(text("Pending"))),
                        ..Default::default()
                    }),
                    TableCell(TableCellProps {
                        col_w: None,
                        align: Some("end".into()),
                        children: Some(Element::from(text("$150.00"))),
                        ..Default::default()
                    }),
                ])),
                ..Default::default()
            }),
        ])),
        ..Default::default()
    });
    Table(TableProps {
        children: Some(frag(vec![header, body])),
        ..Default::default()
    })
}

fn invoice_cell(text_s: &str, col_w: Option<&str>, align: Option<&str>) -> Element {
    TableCell(TableCellProps {
        col_w: col_w.map(|s| s.into()),
        align: align.map(|s| s.into()),
        children: Some(Element::from(text(text_s))),
        ..Default::default()
    })
}

fn invoice_body_row4(id: &str, invoice: &str, status: &str, method: &str, amount: &str) -> Element {
    TableRow(TableRowProps {
        id: id.into(),
        selected: Some(false),
        on_click: Some(Handler::default()),
        children: Some(frag(vec![
            invoice_cell(invoice, Some("100px"), None),
            invoice_cell(status, None, None),
            invoice_cell(method, None, None),
            invoice_cell(amount, Some("96px"), Some("end")),
        ])),
        ..Default::default()
    })
}

fn invoice_table_with_method() -> Element {
    let header = TableHeader(TableHeaderProps {
        children: Some(TableRow(TableRowProps {
            id: "inv-h".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(frag(vec![
                TableHead(TableHeadProps {
                    col_w: Some("100px".into()),
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
                    col_w: Some("96px".into()),
                    align: Some("end".into()),
                    children: Some(Element::from(text("Amount"))),
                    ..Default::default()
                }),
            ])),
            ..Default::default()
        })),
        ..Default::default()
    });
    let body = TableBody(TableBodyProps {
        children: Some(frag(vec![
            invoice_body_row4("inv-0", "INV001", "Paid", "Credit Card", "$250.00"),
            invoice_body_row4("inv-1", "INV002", "Pending", "PayPal", "$150.00"),
            invoice_body_row4("inv-2", "INV003", "Unpaid", "Bank Transfer", "$350.00"),
        ])),
        ..Default::default()
    });
    Table(TableProps {
        children: Some(frag(vec![header, body])),
        ..Default::default()
    })
}

fn invoice_body_row(id: &str, invoice: &str, status: &str, amount: &str) -> Element {
    TableRow(TableRowProps {
        id: id.into(),
        selected: Some(false),
        on_click: Some(Handler::default()),
        children: Some(frag(vec![
            invoice_cell(invoice, None, None),
            invoice_cell(status, None, None),
            invoice_cell(amount, None, Some("end")),
        ])),
        ..Default::default()
    })
}

/// Gallery composite invoices: last body (INV003) + footer Total share a 1px seam.
fn invoice_table_with_footer() -> Element {
    let header = TableHeader(TableHeaderProps {
        children: Some(TableRow(TableRowProps {
            id: "inv-h".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(frag(vec![
                TableHead(TableHeadProps {
                    col_w: Some("100px".into()),
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
                    col_w: Some("96px".into()),
                    align: Some("end".into()),
                    children: Some(Element::from(text("Amount"))),
                    ..Default::default()
                }),
            ])),
            ..Default::default()
        })),
        ..Default::default()
    });
    let body = TableBody(TableBodyProps {
        children: Some(frag(vec![
            invoice_body_row("inv-0", "INV001", "Paid", "$250.00"),
            invoice_body_row("inv-1", "INV002", "Pending", "$150.00"),
            invoice_body_row("inv-2", "INV003", "Unpaid", "$350.00"),
        ])),
        ..Default::default()
    });
    let footer = TableFooter(TableFooterProps {
        children: Some(TableRow(TableRowProps {
            id: "inv-f".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(frag(vec![
                invoice_cell("Total", Some("100px"), None),
                invoice_cell("", None, None),
                invoice_cell("$750.00", Some("96px"), Some("end")),
            ])),
            ..Default::default()
        })),
        ..Default::default()
    });
    Table(TableProps {
        children: Some(frag(vec![header, body, footer])),
        ..Default::default()
    })
}

/// Gallery data page: outer `scroll-data` + table-container.
fn invoice_in_outer_scroll() -> Element {
    invoice_in_outer_scroll_ex(false)
}

fn invoice_in_outer_scroll_ex(tall_sibling: bool) -> Element {
    let mut col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w("100%")
        .child(invoice_table());
    if tall_sibling {
        col = col.child(
            Element::new()
                .w("100%")
                .h(1600.0)
                .min_h(1600.0)
                .flex_shrink(0.0)
                .bg([0.95, 0.95, 0.95, 1.0]),
        );
    }
    Element::new()
        .w("100%")
        .h("100%")
        .overflow_y("auto")
        .overflow_x("auto")
        .focus_id("scroll:scroll-data")
        .child(col)
}

fn collect_texts(result: &RenderResult) -> Vec<String> {
    result.text_nodes.iter().map(|n| n.text.clone()).collect()
}

fn walk_slots<'a>(el: &'a Element, slot: &str, out: &mut Vec<&'a Element>) {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        out.push(el);
    }
    for c in &el.children {
        walk_slots(c, slot, out);
    }
}

fn row_bottom_border(el: &Element) -> f32 {
    el.style.resolved_border_widths()[2]
}

fn assert_invoice_glyphs_not_clip_culled(when: &str, result: &RenderResult) {
    for needle in ["INV001", "Paid", "$250.00"] {
        let node = result
            .text_nodes
            .iter()
            .find(|n| n.text.contains(needle))
            .unwrap_or_else(|| panic!("{when}: missing `{needle}`"));
        assert!(
            node.width > 1.0 && node.height > 1.0,
            "{when}: `{needle}` glyph size {}x{}",
            node.width,
            node.height
        );
        if let Some(c) = node.clip_bounds {
            let hit =
                crate::app::damage::intersect_rects([node.x, node.y, node.width, node.height], c);
            assert!(
                hit.is_some_and(|h| h[2] > 1.0 && h[3] > 1.0),
                "{when}: `{needle}` clip-culled; glyph=[{},{},{},{}] clip={c:?}",
                node.x,
                node.y,
                node.width,
                node.height
            );
        }
    }
}

fn assert_invoice_texts(when: &str, texts: &[String]) {
    for needle in ["Invoice", "Status", "Amount", "INV001", "Paid", "$250.00"] {
        assert!(
            texts.iter().any(|t| t.contains(needle)),
            "{when}: collect must keep `{needle}`; texts={texts:?}"
        );
    }
}

#[test]
fn composite_table_collect_keeps_cell_texts() {
    let (_handle, mut root) = create_root(|| invoice_table());
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
    let texts = collect_texts(&first);
    assert_invoice_texts("first collect", &texts);
    for n in &first.text_nodes {
        if ["INV001", "Paid", "$250.00"]
            .iter()
            .any(|s| n.text.contains(s))
        {
            assert!(
                n.width > 1.0 && n.height > 1.0,
                "cell glyph `{}` must have size, got {}x{}",
                n.text,
                n.width,
                n.height
            );
        }
    }
}

#[test]
fn composite_table_hover_chrome_must_not_wipe_row_border_or_glyphs() {
    let (_handle, mut root) = create_root(|| invoice_table());
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
    assert_invoice_texts("setup", &collect_texts(&first));

    let mut rows = Vec::new();
    walk_slots(&root, "table-row", &mut rows);
    let border_before = {
        let body_row = rows
            .iter()
            .find(|r| r.focus_id.as_deref() == Some("inv-0"))
            .expect("body row");
        let w = row_bottom_border(body_row);
        assert!(w > 0.5, "body row must start with border-b, got {w}");
        w
    };

    let _ = take_frame_paint_nodes();
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, W, H, [0.0, 0.0]);
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        W,
        H,
        &mut tree,
        &mut impact,
    );

    let border_after = {
        let mut rows = Vec::new();
        walk_slots(&root, "table-row", &mut rows);
        let body_row = rows
            .iter()
            .find(|r| r.focus_id.as_deref() == Some("inv-0"))
            .expect("body row after chrome");
        row_bottom_border(body_row)
    };
    assert!(
        (border_after - border_before).abs() < 0.01,
        "ghost Button hover chrome must not wipe table-row border-b ({border_before} → {border_after})"
    );

    tree.set_collect_skip_allowed(true);
    let second = tree.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], false, false);
    assert_invoice_texts("after hover chrome + skip", &collect_texts(&second));
}

#[test]
fn composite_table_row_hover_must_recapture_cell_glyphs() {
    let (_handle, mut root) = create_root(|| invoice_table());
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], false, false);
    let _ = take_frame_paint_nodes();

    let (mouse, on_hover) = {
        let mut rows = Vec::new();
        walk_slots(&root, "table-row", &mut rows);
        let body = rows
            .iter()
            .find(|r| r.focus_id.as_deref() == Some("inv-0"))
            .expect("body row");
        let b = body.layout_bounds;
        (
            [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5],
            body.on_hover.clone(),
        )
    };
    if let Some(hover) = on_hover {
        hover.borrow_mut()(true);
    }

    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, W, H, [0.0, 0.0]);
    sync_class_variants_subtree(&mut root, mouse, "", false, W, H, &mut tree, &mut impact);

    let dirties = take_frame_paint_nodes();
    let hovered = tree.build_and_layout_fast(&mut root, W, H, mouse, true, false);
    assert_invoice_texts("hovered collect", &collect_texts(&hovered));

    assert!(
        !dirties.is_empty(),
        "row hover must push node-present dirty (empty damage FullClears / drops glyphs)"
    );
    let captured =
        node_present_capture_text_indices(&dirties, &hovered.text_nodes, &hovered.node_paints);
    let captured_texts: Vec<&str> = captured
        .iter()
        .filter_map(|&i| hovered.text_nodes.get(i).map(|n| n.text.as_str()))
        .collect();
    for needle in ["INV001", "Paid", "$250.00"] {
        assert!(
            captured_texts.iter().any(|t| t.contains(needle)),
            "dest-reset of the hovered row must recapture `{needle}`; captured={captured_texts:?} dirty={dirties:?}"
        );
    }
}

#[test]
fn data_table_sort_headers_align_with_body_through_layout_and_hover() {
    for sortable in [false, true] {
        reset_registered_anchors();
        crate::app::tree_hit::clear_hover_tracking();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (_handle, _) = create_root({
            let live = Rc::clone(&live);
            move || {
                create_effect({
                    let live = Rc::clone(&live);
                    move || {
                        *live.borrow_mut() = table("diag-th-align")
                            .columns([
                                TableColumn::new("name", "Name").width(120.0),
                                TableColumn::new("role", "Role").width(100.0),
                                TableColumn::new("score", "Score").width(80.0).align("end"),
                            ])
                            .sortable(sortable)
                            .rows([vec!["Item 05".into(), "User".into(), "65".into()]])
                            .height(80.0)
                            .into();
                    }
                });
            }
        });
        drain_deferred_view_remounts();

        for cycle in 0..6 {
            let click = {
                let mut root = live.borrow_mut();
                let mut tree = LayoutTree::new();
                let _ = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
                for hovered in [false, true, false] {
                    let mouse = if hovered && sortable {
                        let head =
                            find_focus(&root, "diag-th-align:th:name").expect("sortable header");
                        let [x, y, w, h] = head.layout_bounds;
                        [x + w * 0.5, y + h * 0.5]
                    } else {
                        [-1.0, -1.0]
                    };
                    sync_table_hover(&mut root, &mut tree, mouse, W, H);
                    let _ = tree.build_and_layout_fast(&mut root, W, H, mouse, false, false);
                    for (header, value, end) in [
                        ("Name", "Item 05", false),
                        ("Role", "User", false),
                        ("Score", "65", true),
                    ] {
                        let head = crate::core::tree::find_in_tree(&root, |el| {
                            el.text.as_deref().is_some_and(|s| s.starts_with(header))
                        })
                        .expect("header caption");
                        let body = crate::core::tree::find_in_tree(&root, |el| {
                            el.text.as_deref() == Some(value)
                        })
                        .expect("body caption");
                        let edge = |el: &Element| {
                            el.layout_bounds[0] + if end { el.layout_bounds[2] } else { 0.0 }
                        };
                        assert!((edge(head) - edge(body)).abs() < 1.0,
                            "sortable={sortable}, cycle={cycle}, hover={hovered}, {header}: header={:?}, body={:?}, justify={:?}",
                            head.layout_bounds, body.layout_bounds,
                            find_focus(&root, "diag-th-align:th:name").map(|el| el.style.justify_content));
                    }
                }
                let sort_column = if cycle < 3 { "name" } else { "score" };
                find_focus(&root, &format!("diag-th-align:th:{sort_column}"))
                    .and_then(|el| el.on_click.clone())
            };
            if let Some(click) = click {
                click.borrow_mut()();
                drain_deferred_view_remounts();
            }
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
        crate::app::tree_hit::clear_hover_tracking();
    }
}

#[test]
fn data_table_collect_keeps_visible_cell_texts() {
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (_handle, _) = create_root({
        let live = Rc::clone(&live);
        move || {
            create_effect({
                let live = Rc::clone(&live);
                move || {
                    let el: Element = table("diag-table")
                        .columns([
                            TableColumn::new("name", "Name").width(120.0),
                            TableColumn::new("role", "Role").width(100.0),
                        ])
                        .row_count(8)
                        .height(200.0)
                        .into();
                    *live.borrow_mut() = el;
                }
            });
        }
    });
    drain_deferred_view_remounts();

    let mut root = live.borrow_mut();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut *root, W, H, [-1.0, -1.0]);
    let texts = collect_texts(&first);
    for needle in ["Name", "Role", "0:0", "0:1"] {
        assert!(
            texts.iter().any(|t| t.contains(needle)),
            "data_table collect must paint `{needle}`; texts={texts:?}"
        );
    }

    tree.set_collect_skip_allowed(true);
    let second = tree.build_and_layout_fast(&mut *root, W, H, [-1.0, -1.0], false, false);
    let texts = collect_texts(&second);
    assert!(
        texts.iter().any(|t| t.contains("0:0")),
        "skip-replay must keep visible cells; texts={texts:?}"
    );

    drop(root);
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn data_table_select_rebuild_must_keep_cell_texts() {
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let sel = Rc::new(RefCell::new(None));
    let (_handle, _) = create_root({
        let live = Rc::clone(&live);
        let sel = Rc::clone(&sel);
        move || {
            let selected = crate::core::reactivity::use_signal("diag-table__sel", || None::<usize>);
            *sel.borrow_mut() = Some(selected);
            create_effect({
                let live = Rc::clone(&live);
                move || {
                    let el: Element = table("diag-table-sel")
                        .columns([TableColumn::new("name", "Name").width(120.0)])
                        .row_count(6)
                        .height(200.0)
                        .selected_signal(selected)
                        .into();
                    *live.borrow_mut() = el;
                }
            });
        }
    });
    drain_deferred_view_remounts();

    {
        let mut root = live.borrow_mut();
        let mut tree = LayoutTree::new();
        let first = tree.build_and_layout(&mut *root, W, H, [-1.0, -1.0]);
        let texts = collect_texts(&first);
        assert!(
            texts.iter().any(|t| t.contains("0:0")),
            "setup: data_table must paint cells; texts={texts:?}"
        );
    }

    sel.borrow().expect("selected signal").set(Some(0));
    drain_deferred_view_remounts();

    {
        let mut root = live.borrow_mut();
        let mut tree = LayoutTree::new();
        let after = tree.build_and_layout(&mut *root, W, H, [-1.0, -1.0]);
        let texts = collect_texts(&after);
        assert!(
            texts.iter().any(|t| t.contains("0:0")),
            "select rebuild must not drop cell glyphs; texts={texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Name")),
            "select rebuild must keep header; texts={texts:?}"
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// `selected.get()` rebuilds the table island. The frame *before* deferred remount
/// must still show cells — an empty placeholder is the gallery flash.
#[test]
fn data_table_select_must_not_blank_before_remount_drain() {
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let sel = Rc::new(RefCell::new(None));
    let (_handle, _) = create_root({
        let live = Rc::clone(&live);
        let sel = Rc::clone(&sel);
        move || {
            let selected =
                crate::core::reactivity::use_signal("diag-table__sel2", || None::<usize>);
            *sel.borrow_mut() = Some(selected);
            create_effect({
                let live = Rc::clone(&live);
                move || {
                    let el: Element = table("diag-table-blank")
                        .columns([TableColumn::new("name", "Name").width(120.0)])
                        .row_count(6)
                        .height(200.0)
                        .selected_signal(selected)
                        .into();
                    *live.borrow_mut() = el;
                }
            });
        }
    });
    drain_deferred_view_remounts();

    {
        let mut root = live.borrow_mut();
        let mut tree = LayoutTree::new();
        let first = tree.build_and_layout(&mut *root, W, H, [-1.0, -1.0]);
        assert!(
            collect_texts(&first).iter().any(|t| t.contains("0:0")),
            "setup must paint cells"
        );
    }

    sel.borrow().expect("selected signal").set(Some(0));
    {
        let mut root = live.borrow_mut();
        let mut tree = LayoutTree::new();
        let mid = tree.build_and_layout(&mut *root, W, H, [-1.0, -1.0]);
        let texts = collect_texts(&mid);
        assert!(
            texts.iter().any(|t| t.contains("0:0")),
            "select must not blank the table before remount drain; texts={texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Name")),
            "header must survive the select frame; texts={texts:?}"
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn composite_table_row_dest_reset_must_recapture_cell_glyphs() {
    let (_handle, mut root) = create_root(|| invoice_table());
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
    assert_invoice_texts("setup", &collect_texts(&result));

    let (key, bounds) = {
        let mut rows = Vec::new();
        walk_slots(&root, "table-row", &mut rows);
        let body = rows
            .iter()
            .find(|r| r.focus_id.as_deref() == Some("inv-0"))
            .expect("body row");
        (paint_element_key(body), body.layout_bounds)
    };
    assert!(bounds[2] > 1.0 && bounds[3] > 1.0, "row bounds={bounds:?}");

    let dirty = PaintDirty {
        key,
        bounds,
        kind: PaintDirtyKind::Node,
    };
    let captured =
        node_present_capture_text_indices(&[dirty], &result.text_nodes, &result.node_paints);
    let captured_texts: Vec<&str> = captured
        .iter()
        .filter_map(|&i| result.text_nodes.get(i).map(|n| n.text.as_str()))
        .collect();
    for needle in ["INV001", "Paid", "$250.00"] {
        assert!(
            captured_texts.iter().any(|t| t.contains(needle)),
            "row dest-reset must recapture `{needle}`; captured={captured_texts:?} paints={:?}",
            result
                .node_paints
                .iter()
                .map(|p| (p.key, p.bounds, p.text))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn composite_table_hover_paint_must_not_zero_row_border_width() {
    let (_handle, mut row) = create_root(|| {
        TableRow(TableRowProps {
            id: "border-row".into(),
            selected: Some(false),
            on_click: Some(Handler::default()),
            children: Some(TableCell(TableCellProps {
                col_w: None,
                align: None,
                children: Some(Element::from(text("INV001"))),
                ..Default::default()
            })),
            ..Default::default()
        })
    });
    row = row.w(400.0).h(40.0);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut row, 400.0, 40.0, [0.0, 0.0]);
    let before = row_bottom_border(&row);
    assert!(before > 0.5, "row must have border-b, got {before}");

    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut row, &mut tree, &mut impact, 400.0, 40.0, [0.0, 0.0]);
    let after = row_bottom_border(&row);
    assert!(
        (after - before).abs() < 0.01,
        "sync_button_hover_paint must not clear table-row border ({before} → {after})"
    );
}

/// First paint of invoices inside `scroll-data` must keep cell glyphs after the
/// AppState chrome + skip frame (gallery: rows appear, then the body goes white).
#[test]
fn nested_scroll_table_texts_survive_chrome_skip_and_hover() {
    const VW: f32 = 1280.0;
    const VH: f32 = 800.0;
    let (_handle, mut root) = create_root(invoice_in_outer_scroll);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, VW, VH, [-1.0, -1.0]);
    assert_invoice_texts("first collect", &collect_texts(&first));
    assert_invoice_glyphs_not_clip_culled("first collect", &first);
    {
        let mut wraps = Vec::new();
        walk_slots(&root, "table-container", &mut wraps);
        let wrap = wraps.first().expect("table-container");
        let (clip_x, clip_y) = overflow_clip_axes(wrap);
        assert!(
            !clip_y,
            "table-container must not invent a Y clip (overflow-x auto is a scrollport); clip=({clip_x},{clip_y})"
        );
    }

    let _ = take_frame_paint_nodes();
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        VW,
        VH,
        &mut tree,
        &mut impact,
    );
    tick_active_css_transitions_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        VW,
        VH,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    let skipped = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    assert_invoice_texts("chrome + skip", &collect_texts(&skipped));
    assert_invoice_glyphs_not_clip_culled("chrome + skip", &skipped);

    let dirties = take_frame_paint_nodes();
    if !dirties.is_empty() {
        let captured =
            node_present_capture_text_indices(&dirties, &skipped.text_nodes, &skipped.node_paints);
        let captured_texts: Vec<&str> = captured
            .iter()
            .filter_map(|&i| skipped.text_nodes.get(i).map(|n| n.text.as_str()))
            .collect();
        for needle in ["INV001", "Paid", "$250.00"] {
            assert!(
                captured_texts.iter().any(|t| t.contains(needle))
                    || skipped
                        .text_nodes
                        .iter()
                        .any(|n| n.text.contains(needle)),
                "dest-reset after mount chrome must keep `{needle}`; captured={captured_texts:?} dirty={dirties:?}"
            );
        }
    }

    let (mouse, on_hover) = {
        let mut rows = Vec::new();
        walk_slots(&root, "table-row", &mut rows);
        let body = rows
            .iter()
            .find(|r| r.focus_id.as_deref() == Some("inv-0"))
            .expect("body row");
        let b = body.layout_bounds;
        (
            [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5],
            body.on_hover.clone(),
        )
    };
    if let Some(hover) = on_hover {
        hover.borrow_mut()(true);
    }
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    sync_class_variants_subtree(&mut root, mouse, "", false, VW, VH, &mut tree, &mut impact);
    tick_active_css_transitions_subtree(
        &mut root,
        mouse,
        "",
        false,
        VW,
        VH,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let hovered = tree.build_and_layout_fast(&mut root, VW, VH, mouse, true, false);
    assert_invoice_texts("hover", &collect_texts(&hovered));
    let hover_dirties = take_frame_paint_nodes();
    assert!(
        !hover_dirties.is_empty(),
        "row hover must push node-present dirty"
    );
    let captured = node_present_capture_text_indices(
        &hover_dirties,
        &hovered.text_nodes,
        &hovered.node_paints,
    );
    let captured_texts: Vec<&str> = captured
        .iter()
        .filter_map(|&i| hovered.text_nodes.get(i).map(|n| n.text.as_str()))
        .collect();
    for needle in ["INV001", "Paid", "$250.00"] {
        assert!(
            captured_texts.iter().any(|t| t.contains(needle)),
            "hover dest-reset must recapture `{needle}`; captured={captured_texts:?}"
        );
    }
}

fn row_bg(root: &Element, id: &str) -> [f32; 4] {
    let mut rows = Vec::new();
    walk_slots(root, "table-row", &mut rows);
    rows.iter()
        .find(|r| r.focus_id.as_deref() == Some(id))
        .expect("row")
        .style
        .bg_color
}

fn row_mouse(root: &Element, id: &str) -> [f32; 2] {
    let mut rows = Vec::new();
    walk_slots(root, "table-row", &mut rows);
    let b = rows
        .iter()
        .find(|r| r.focus_id.as_deref() == Some(id))
        .expect("row")
        .layout_bounds;
    [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
}

fn sync_table_hover(root: &mut Element, tree: &mut LayoutTree, mouse: [f32; 2], w: f32, h: f32) {
    crate::app::tree_hit::apply_pointer_hover_reconcile(root, mouse[0], mouse[1], false);
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(root, tree, &mut impact, w, h, [0.0, 0.0]);
    sync_class_variants_subtree(root, mouse, "", false, w, h, tree, &mut impact);
}

/// First enter paints muted/50 on that row only; switching must not leave the
/// previous row hovered (gallery: no style on enter, both rows flash on move).
#[test]
fn composite_table_row_hover_fill_follows_exclusive_row() {
    crate::app::tree_hit::clear_hover_tracking();
    let hover_bg = crate::style::parse_color("muted/50");
    let rest_bg = [0.0, 0.0, 0.0, 0.0];
    let (_handle, mut root) = create_root(|| invoice_table());
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);

    let m0 = row_mouse(&root, "inv-0");
    sync_table_hover(&mut root, &mut tree, m0, W, H);
    let bg0 = row_bg(&root, "inv-0");
    let bg1 = row_bg(&root, "inv-1");
    assert!(
        (bg0[3] - hover_bg[3]).abs() < 0.05 && (bg0[0] - hover_bg[0]).abs() < 0.05,
        "first hover on inv-0 must apply muted/50; got {bg0:?} want {hover_bg:?}"
    );
    assert!(
        bg1[3] < 0.05,
        "sibling inv-1 must stay rest while inv-0 is hovered; got {bg1:?}"
    );

    let m1 = row_mouse(&root, "inv-1");
    sync_table_hover(&mut root, &mut tree, m1, W, H);
    let bg0 = row_bg(&root, "inv-0");
    let bg1 = row_bg(&root, "inv-1");
    assert!(
        bg0[3] < 0.05,
        "leaving inv-0 must drop hover fill immediately; got {bg0:?} rest={rest_bg:?}"
    );
    assert!(
        (bg1[3] - hover_bg[3]).abs() < 0.05,
        "inv-1 must take hover fill; got {bg1:?} want {hover_bg:?}"
    );
    crate::app::tree_hit::clear_hover_tracking();
}

fn xywh_overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && a[0] + a[2] > b[0] && a[1] < b[1] + b[3] && a[1] + a[3] > b[1]
}

fn dest_reset_scissor(
    dirty: &PaintDirty,
    node_paints: &[crate::layout::layout_tree::NodePaint],
) -> [f32; 4] {
    let prev: Vec<(u64, [f32; 4])> = node_paints.iter().map(|p| (p.key, p.bounds)).collect();
    node_present_scissor_logical(
        dirty.kind,
        dirty.key,
        dirty.bounds,
        &prev,
        node_paints,
        node_present_scissor_pad(dirty.kind),
    )
}

fn assert_dirties_spare_seams(
    when: &str,
    dirties: &[PaintDirty],
    node_paints: &[crate::layout::layout_tree::NodePaint],
    top_seam: [f32; 4],
    bottom_seam: [f32; 4],
) {
    for d in dirties {
        let scissor = dest_reset_scissor(d, node_paints);
        assert!(
            !xywh_overlaps(scissor, top_seam),
            "{when}: dest-reset must not punch footer border-t; scissor={scissor:?} seam={top_seam:?} dirty={d:?}"
        );
        assert!(
            !xywh_overlaps(scissor, bottom_seam),
            "{when}: dest-reset must not punch Total border-b; scissor={scissor:?} seam={bottom_seam:?} dirty={d:?}"
        );
    }
}

fn find_slot<'a>(root: &'a Element, slot: &str) -> Option<&'a Element> {
    let mut found = Vec::new();
    walk_slots(root, slot, &mut found);
    found.into_iter().next()
}

fn find_row<'a>(root: &'a Element, id: &str) -> Option<&'a Element> {
    let mut rows = Vec::new();
    walk_slots(root, "table-row", &mut rows);
    rows.into_iter().find(|r| r.focus_id.as_deref() == Some(id))
}

/// Hovering INV003 / Total (footer stays hoverable) must not dest-reset the
/// footer wrapper `border-t` or the Total row `border-b`.
#[test]
fn composite_table_footer_seams_must_survive_adjacent_row_hover() {
    let (_handle, mut root) = create_root(invoice_table_with_footer);
    let mut tree = LayoutTree::new();
    let laid = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);

    let (footer_top, footer_box) = {
        let footer = find_slot(&root, "table-footer").expect("table-footer");
        (
            footer.style.resolved_border_widths()[0],
            footer.layout_bounds,
        )
    };
    assert!(
        footer_top > 0.5,
        "footer wrapper must start with border-t, got {footer_top}"
    );
    let top_seam = [footer_box[0], footer_box[1], footer_box[2], 1.0];

    let (last_body_key, last_body_hover, last_body_border) = {
        let last_body = find_row(&root, "inv-2").expect("INV003");
        (
            paint_element_key(last_body),
            last_body.on_hover.clone(),
            row_bottom_border(last_body),
        )
    };
    assert!(
        last_body_border < 0.5,
        "tbody last row must strip border-b (ShadCN last-child:border-0)"
    );

    let (total_box, total_bottom, total_hover, total_has_paint) = {
        let total = find_row(&root, "inv-f").expect("Total");
        (
            total.layout_bounds,
            row_bottom_border(total),
            total.on_hover.clone(),
            total.authoring_state.get::<crate::components::button::ButtonHoverPaint>().is_some(),
        )
    };
    assert!(
        total_bottom > 0.5,
        "footer row must keep border-b, got {total_bottom}"
    );
    assert!(
        total_hover.is_some() && total_has_paint,
        "footer row stays hoverable; same-color fill must not dest-reset the seams"
    );
    let bottom_seam = [
        total_box[0],
        total_box[1] + total_box[3] - 1.0,
        total_box[2],
        1.0,
    ];

    let _ = take_frame_paint_nodes();
    let mouse = row_mouse(&root, "inv-2");
    if let Some(hover) = last_body_hover {
        hover.borrow_mut()(true);
    }
    sync_table_hover(&mut root, &mut tree, mouse, W, H);
    let dirties = take_frame_paint_nodes();
    assert!(
        dirties.iter().any(|d| d.key == last_body_key),
        "INV003 hover must dest-reset that row's fill; dirties={dirties:?}"
    );
    assert_dirties_spare_seams(
        "INV003 hover",
        &dirties,
        &laid.node_paints,
        top_seam,
        bottom_seam,
    );

    let footer_after = find_slot(&root, "table-footer").expect("table-footer after hover");
    assert!(
        (footer_after.style.resolved_border_widths()[0] - footer_top).abs() < 0.01,
        "hover paint must not change footer border-t width"
    );
    let total_after = find_row(&root, "inv-f").expect("Total after hover");
    assert!(
        (row_bottom_border(total_after) - total_bottom).abs() < 0.01,
        "hover paint must not change Total border-b width"
    );

    let _ = take_frame_paint_nodes();
    let mouse_f = row_mouse(&root, "inv-f");
    if let Some(hover) = total_hover {
        hover.borrow_mut()(true);
    }
    sync_table_hover(&mut root, &mut tree, mouse_f, W, H);
    let switch_dirties = take_frame_paint_nodes();
    assert_dirties_spare_seams(
        "INV003 → Total",
        &switch_dirties,
        &laid.node_paints,
        top_seam,
        bottom_seam,
    );
}

fn find_focus<'a>(root: &'a Element, id: &str) -> Option<&'a Element> {
    if root.focus_id.as_deref() == Some(id) {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_focus(c, id))
}

/// data_table sort headers are ghost Buttons flush with the header row.
/// Their dest-reset must not punch the header `border-b` or the top hairline.
#[test]
fn data_table_header_button_hover_must_not_punch_header_seams() {
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (_handle, _) = create_root({
        let live = Rc::clone(&live);
        move || {
            create_effect({
                let live = Rc::clone(&live);
                move || {
                    let el: Element = table("diag-th-seams")
                        .columns([
                            TableColumn::new("name", "Name").width(160.0),
                            TableColumn::new("role", "Role").width(120.0),
                        ])
                        .row_count(6)
                        .height(200.0)
                        .into();
                    *live.borrow_mut() = el;
                }
            });
        }
    });
    drain_deferred_view_remounts();

    let mut root = live.borrow_mut();
    let mut tree = LayoutTree::new();
    let laid = tree.build_and_layout(&mut *root, W, H, [-1.0, -1.0]);

    let (hb, header_border_b) = {
        let header_row = find_slot(&root, "table-header")
            .and_then(find_first_header_row)
            .expect("header row");
        (header_row.layout_bounds, row_bottom_border(header_row))
    };
    assert!(header_border_b > 0.5, "header row must have border-b");
    let top_seam = [hb[0], hb[1], hb[2], 1.0];
    let bottom_seam = [hb[0], hb[1] + hb[3] - 1.0, hb[2], 1.0];

    let (btn_id, btn_hover, mouse) = {
        let btn = find_focus(&root, "diag-th-seams:th:name").expect("sort header button");
        assert_eq!(
            btn.attrs.get("data-slot").map(String::as_str),
            Some("table-head"),
            "sort header must stay a table-head slot so dest-reset insets the fill"
        );
        let b = btn.layout_bounds;
        (
            paint_element_key(btn),
            btn.on_hover.clone(),
            [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5],
        )
    };

    let _ = take_frame_paint_nodes();
    if let Some(hover) = btn_hover {
        hover.borrow_mut()(true);
    }
    sync_table_hover(&mut root, &mut tree, mouse, W, H);
    let dirties = take_frame_paint_nodes();
    assert!(
        dirties.iter().any(|d| d.key == btn_id) || !dirties.is_empty(),
        "header button hover must dest-reset its fill; dirties={dirties:?}"
    );
    assert_dirties_spare_seams(
        "header button hover",
        &dirties,
        &laid.node_paints,
        top_seam,
        bottom_seam,
    );

    let _ = take_frame_paint_nodes();
    if let Some(btn) = find_focus(&root, "diag-th-seams:th:name") {
        if let Some(hover) = btn.on_hover.clone() {
            hover.borrow_mut()(false);
        }
    }
    sync_table_hover(&mut root, &mut tree, [-1.0, -1.0], W, H);
    let leave = take_frame_paint_nodes();
    assert_dirties_spare_seams(
        "header button leave",
        &leave,
        &laid.node_paints,
        top_seam,
        bottom_seam,
    );

    drop(root);
    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn find_first_header_row(header: &Element) -> Option<&Element> {
    if header.attrs.get("data-slot").map(String::as_str) == Some("table-row") {
        return Some(header);
    }
    header.children.iter().find_map(find_first_header_row)
}

/// Gallery data page: hovering a data_table row must not dest-reset the invoice
/// header. Button focus-ring sync used to treat every `border-b` row as chrome
/// and punch 「Status」 into the Method column as 「Sta」.
#[test]
fn data_table_row_hover_must_not_stamp_invoice_status_as_sta() {
    reset_registered_anchors();
    crate::app::tree_hit::clear_hover_tracking();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (_handle, _) = create_root({
        let live = Rc::clone(&live);
        move || {
            create_effect({
                let live = Rc::clone(&live);
                move || {
                    let dt: Element = table("diag-sta")
                        .columns([
                            TableColumn::new("name", "Name").width(160.0),
                            TableColumn::new("role", "Role").width(120.0),
                            TableColumn::new("score", "Score").width(80.0),
                        ])
                        .row_count(8)
                        .height(200.0)
                        .into();
                    *live.borrow_mut() = Element::new()
                        .flex_col()
                        .w("100%")
                        .gap(16.0)
                        .child(invoice_table_with_method())
                        .child(dt);
                }
            });
        }
    });
    drain_deferred_view_remounts();

    const VW: f32 = 1280.0;
    const VH: f32 = 800.0;
    let mut root = live.borrow_mut();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut *root, VW, VH, [-1.0, -1.0]);
    let status = first
        .text_nodes
        .iter()
        .find(|n| n.text == "Status")
        .expect("invoice Status header");
    let method = first
        .text_nodes
        .iter()
        .find(|n| n.text == "Method")
        .expect("invoice Method header");
    let status_box = [status.x, status.y, status.width, status.height];
    let method_box = [method.x, method.y, method.width, method.height];
    assert!(
        status_box[0] + status_box[2] <= method_box[0] + 1.0,
        "setup: Status must sit left of Method; status={status_box:?} method={method_box:?}"
    );

    let cell00 = first
        .text_nodes
        .iter()
        .find(|n| n.text == "0:0")
        .expect("data_table cell 0:0");
    let mouse = [
        cell00.x + cell00.width * 0.5,
        cell00.y + cell00.height * 0.5,
    ];
    assert!(
        mouse[1] > method_box[1] + method_box[3] + 8.0,
        "data_table cell must sit below invoice Method; mouse={mouse:?} method={method_box:?}"
    );

    let row_hover = find_focus(&root, "diag-sta:tr:0")
        .expect("data_table body row")
        .on_hover
        .clone();

    let _ = take_frame_paint_nodes();
    if let Some(hover) = row_hover {
        hover.borrow_mut()(true);
    }
    sync_table_hover(&mut root, &mut tree, mouse, VW, VH);
    let dirties = take_frame_paint_nodes();
    assert!(
        !dirties.is_empty(),
        "data_table row hover must dest-reset; dirties={dirties:?}"
    );
    for d in &dirties {
        assert!(
            !xywh_overlaps(d.bounds, status_box) && !xywh_overlaps(d.bounds, method_box),
            "data_table dest-reset must not cover invoice Status/Method; dirty={d:?} dirties={dirties:?} mouse={mouse:?}"
        );
    }

    let hovered = tree.build_and_layout_fast(&mut *root, VW, VH, mouse, true, false);
    let captured =
        node_present_capture_text_indices(&dirties, &hovered.text_nodes, &hovered.node_paints);
    let captured_texts: Vec<&str> = captured
        .iter()
        .filter_map(|&i| hovered.text_nodes.get(i).map(|n| n.text.as_str()))
        .collect();
    assert!(
        !captured_texts.iter().any(|t| *t == "Status" || t.contains("Sta")),
        "data_table hover dest-reset must not recapture invoice Status; captured={captured_texts:?}"
    );

    let sta_ghosts = hovered
        .text_nodes
        .iter()
        .filter(|n| n.text.contains("Sta") && n.text != "Status")
        .map(|n| (n.text.as_str(), n.x, n.y))
        .collect::<Vec<_>>();
    assert!(
        sta_ghosts.is_empty(),
        "hovering data_table must not invent Sta glyphs; ghosts={sta_ghosts:?}"
    );
    let method_after = hovered
        .text_nodes
        .iter()
        .filter(|n| n.text == "Method")
        .count();
    let status_after = hovered
        .text_nodes
        .iter()
        .filter(|n| n.text == "Status")
        .count();
    assert_eq!(method_after, 1, "Method header must stay a single run");
    assert_eq!(status_after, 1, "Status header must stay a single run");

    drop(root);
    clear_test_reconcile_root();
    reset_registered_anchors();
    crate::app::tree_hit::clear_hover_tracking();
}

/// A tall page sibling must not flex-shrink the table-container into an empty clip.
#[test]
fn nested_scroll_table_does_not_collapse_beside_tall_sibling() {
    let (_handle, mut root) = create_root(|| invoice_in_outer_scroll_ex(true));
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    assert_invoice_texts("tall sibling first collect", &collect_texts(&first));

    tree.set_collect_skip_allowed(true);
    let skipped = tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [-1.0, -1.0], false, false);
    assert_invoice_texts("tall sibling skip", &collect_texts(&skipped));
}
