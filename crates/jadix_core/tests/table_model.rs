use jadix_core::table::*;

fn columns(order: &[&str]) -> TableColumns {
    TableColumns::new(order.iter().map(|key| TableColumn::new(*key, *key).width(100.)).collect()).unwrap()
}

#[test]
fn column_windows_preserve_edges_keys_and_pin_geometry() {
    let old = columns(&["a", "b", "c", "d", "e", "f"]);
    let next = columns(&["f", "a", "b", "d", "e", "c"]);
    assert_eq!(next.remap_offset(&old, 0., 200., 150.).unwrap(), 0.);
    assert_eq!(next.remap_offset(&old, 400., 200., 150.).unwrap(), 450.);
    // b remains the first visible column with the same intra-column offset.
    assert_eq!(next.remap_offset(&old, 125., 200., 200.).unwrap(), 225.);
    let removed = columns(&["a", "c", "d", "e", "f"]);
    assert_eq!(removed.remap_offset(&old, 125., 200., 200.).unwrap(), 100.);
    let pinned = TableColumns::new(vec![TableColumn::new("left", "L").width(40.).pin(ColumnPin::Left),
        TableColumn::new("right", "R").width(60.).pin(ColumnPin::Right),
        TableColumn::new("a", "A").width(100.), TableColumn::new("b", "B").width(100.),
        TableColumn::new("c", "C").width(100.)]).unwrap();
    assert_eq!(pinned.viewport(250.).unwrap(), 150.);
    let window = pinned.window(130., 150., 0).unwrap();
    assert_eq!(window.keys, ["b", "c"]);
    assert_eq!((window.leading, window.trailing), (100., 0.));
    assert_eq!(pinned.reveal("c", 0., 250.).unwrap(), 150.);
    assert_eq!(pinned.reveal("left", 130., 250.).unwrap(), 130.);
    assert!(pinned.window(0., 0., usize::MAX).unwrap().keys.is_empty());
    assert!(pinned.window(f64::NAN, 10., 0).is_err());
    assert!(TableColumns::new(vec![TableColumn::new("x", "X").width(0.)]).is_err());
    assert!(TableColumns::new(vec![TableColumn::new("x", "X"), TableColumn::new("x", "X")]).is_err());
}

#[test]
fn queries_preserve_hidden_selection_and_reject_invalid_replacements_atomically() {
    let mut rows = TableRows::new((0..100).map(|i| format!("r{i:03}")).collect()).unwrap();
    rows.select("r025", false).unwrap();
    rows.query(|key| key.ends_with('0'), |a, b| b.cmp(a), Some((usize::MAX, 3))).unwrap();
    assert_eq!(rows.visible(), ["r000"]);
    assert!(rows.selected().contains("r025"));
    assert_eq!(rows.query(|_| true, |a, b| a.cmp(b), Some((0, 0))), Err(TableError::InvalidExtent));
    assert_eq!(rows.visible(), ["r000"]);
    assert_eq!(rows.set_rows(vec!["x".into(), "x".into()]), Err(TableError::DuplicateKey));
    assert_eq!(rows.visible(), ["r000"]);
    rows.query(|_| true, |a, b| a.cmp(b), None).unwrap();
    assert_eq!(rows.index("r025"), Some(25));
    rows.set_rows(vec!["r025".into(), "r100".into()]).unwrap();
    assert!(rows.selected().contains("r025"));
    rows.select("r025", true).unwrap();
    assert!(rows.selected().is_empty());
}

#[test]
fn overflowing_pins_have_bounded_windows_and_remain_reachable() {
    let columns = TableColumns::new((0..3000).map(|i| TableColumn::new(format!("c{i}"), "Column").width(100.)
        .pin(if i < 1000 { ColumnPin::Left } else if i < 2000 { ColumnPin::None } else { ColumnPin::Right })).collect()).unwrap();
    let widths = columns.band_viewports(600.).unwrap();
    assert_eq!(widths, [200., 200., 200.]);
    for (band, key) in [(0, "c999"), (1, "c1999"), (2, "c2999")] {
        let (target, offset) = columns.reveal_in_band(key, 0., 600.).unwrap();
        assert_eq!(target, band);
        let window = columns.band_window(band, offset, widths[band], 1).unwrap();
        assert!(window.keys.len() <= 4);
        assert!(window.keys.iter().any(|value| value == key));
    }
    assert!(columns.band_viewports(f64::INFINITY).is_err());
    assert!(columns.band_window(3, 0., 100., 0).is_err());
}
