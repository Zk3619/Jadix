use super::*;

fn box_xywh(x: f32, y: f32, w: f32, h: f32) -> BoxRect {
    BoxRect::from_xywh(x, y, w, h)
}

#[test]
fn restriction_bounds_all_edges_including_inversion() {
    let start = box_xywh(40.0, 28.0, 160.0, 100.0);
    let bounds = box_xywh(6.0, 6.0, 788.0, 436.0);
    for edges in [Edges::top_left(), Edges::bottom_right()] {
        for invert in [Invert::None, Invert::Reposition, Invert::Negate] {
            let opts = ResizeOpts::default()
                .edges(edges)
                .invert(invert)
                .restrict_rect(bounds);
            for pointer in [[-10000.0, -10000.0], [10000.0, 10000.0]] {
                let rect = resize_box(start, [200.0, 128.0], pointer, opts, 1.6).normalized();
                assert!(rect.x >= bounds.x && rect.y >= bounds.y);
                assert!(rect.right() <= bounds.right() && rect.bottom() <= bounds.bottom());
            }
        }
    }
}

#[test]
fn right_edge_grows_width() {
    let start = box_xywh(10.0, 20.0, 100.0, 50.0);
    let out = resize_box(
        start,
        [110.0, 45.0],
        [130.0, 45.0],
        ResizeOpts::default().edges(Edges::right()),
        2.0,
    );
    assert!((out.width - 120.0).abs() < 1e-4);
    assert!((out.x - 10.0).abs() < 1e-4);
    assert!((out.height - 50.0).abs() < 1e-4);
}

#[test]
fn invert_none_stops_at_opposite_edge() {
    let start = box_xywh(0.0, 0.0, 80.0, 40.0);
    let out = resize_box(
        start,
        [80.0, 20.0],
        [-40.0, 20.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .min_size(0.0, 0.0),
        2.0,
    );
    assert!(out.width >= 0.0);
    assert!(out.width.abs() < 1e-4);
    assert!((out.x - 0.0).abs() < 1e-4);
}

#[test]
fn invert_reposition_swaps_when_crossed() {
    let start = box_xywh(0.0, 0.0, 80.0, 40.0);
    let out = resize_box(
        start,
        [80.0, 20.0],
        [-20.0, 20.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .invert(Invert::Reposition),
        2.0,
    );
    assert!(out.width > 0.0);
    assert!((out.width - 20.0).abs() < 1e-3);
    assert!(out.x < 0.0);
}

#[test]
fn invert_negate_keeps_negative_width() {
    let start = box_xywh(0.0, 0.0, 80.0, 40.0);
    let out = resize_box(
        start,
        [80.0, 20.0],
        [-20.0, 20.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .invert(Invert::Negate),
        2.0,
    );
    assert!(out.width < 0.0);
}

#[test]
fn square_corner_equalizes() {
    let start = box_xywh(0.0, 0.0, 40.0, 40.0);
    let out = resize_box(
        start,
        [40.0, 40.0],
        [70.0, 50.0],
        ResizeOpts::default()
            .edges(Edges::bottom_right())
            .square(true),
        1.0,
    );
    assert!((out.width - out.height).abs() < 1e-3);
    assert!(out.width >= 30.0);
}

#[test]
fn preserve_aspect_on_width() {
    let start = box_xywh(0.0, 0.0, 80.0, 40.0);
    let out = resize_box(
        start,
        [80.0, 20.0],
        [120.0, 20.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .preserve_aspect(true),
        2.0,
    );
    assert!((out.width / out.height - 2.0).abs() < 1e-3);
    assert!((out.width - 120.0).abs() < 1e-3);
}

#[test]
fn min_and_max_size() {
    let start = box_xywh(0.0, 0.0, 80.0, 40.0);
    let too_small = resize_box(
        start,
        [80.0, 20.0],
        [10.0, 20.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .min_size(40.0, 10.0),
        2.0,
    );
    assert!((too_small.width - 40.0).abs() < 1e-3);
    let too_big = resize_box(
        start,
        [80.0, 20.0],
        [400.0, 20.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .max_size(100.0, 200.0),
        2.0,
    );
    assert!((too_big.width - 100.0).abs() < 1e-3);
}

#[test]
fn from_center_grows_both_sides() {
    let start = box_xywh(50.0, 50.0, 40.0, 40.0);
    let out = resize_box(
        start,
        [90.0, 70.0],
        [110.0, 70.0],
        ResizeOpts::default()
            .edges(Edges::right())
            .from_center(true),
        1.0,
    );
    assert!((out.width - 80.0).abs() < 1e-3);
    assert!((out.center()[0] - 70.0).abs() < 1e-3);
}

#[test]
fn hit_edges_margin() {
    let r = box_xywh(10.0, 10.0, 80.0, 40.0);
    let e = hit_edges(r, [10.0, 30.0], 8.0);
    assert!(e.left && !e.right);
    let miss = hit_edges(r, [200.0, 200.0], 8.0);
    assert!(!miss.any());
    assert_eq!(miss.cursor(), "default");
    let tiny = box_xywh(10.0, 10.0, 8.0, 8.0);
    assert_eq!(hit_edges(tiny, [18.0, 14.0], 8.0), Edges::top_right());
    assert_eq!(hit_edges(tiny, [10.0, 18.0], 8.0), Edges::bottom_left());
    assert_eq!(
        hit_edges(box_xywh(90.0, 50.0, -80.0, -40.0), [10.0, 30.0], 8.0),
        Edges::left()
    );
}

#[test]
fn session_tick_matches_free_fn() {
    let start = box_xywh(0.0, 0.0, 10.0, 10.0);
    let opts = ResizeOpts::default().edges(Edges::bottom_right());
    let sess = ResizeSession::begin(start, [10.0, 10.0], opts);
    let a = sess.tick([18.0, 22.0]);
    let b = resize_box(start, [10.0, 10.0], [18.0, 22.0], opts, 1.0);
    assert_eq!(a.to_xywh(), b.to_xywh());
}
