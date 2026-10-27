use jadix_components::core::geometry::{Axis, Rect};
use jadix_components::core::reactivity::{create_root, use_signal};
use jadix_components::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
#[cfg(feature = "dnd")]
use jadix_components::utils::drag::{self, Candidate, CollisionAlg};
use jadix_components::utils::edge_scroll::{self, AutoScroll, ScrollHit};

#[cfg(feature = "dnd")]
#[test]
fn legacy_dnd_paths_accept_core_geometry_without_conversion() {
    let legacy_axis: drag::geometry::Axis = Axis::Y;
    let public_axis: drag::Axis = legacy_axis;
    assert_eq!(public_axis, Axis::Y);

    let outer: drag::geometry::Rect = Rect::from_bounds([0.0, 0.0, 200.0, 200.0]);
    let inner: drag::Rect = Rect::from_bounds([50.0, 50.0, 40.0, 40.0]);
    let candidates = [
        Candidate {
            id: "outer".into(),
            rect: outer,
        },
        Candidate {
            id: "inner".into(),
            rect: inner,
        },
    ];
    let selected = drag::detect(
        CollisionAlg::PointerWithin,
        [60.0, 60.0],
        Rect::from_bounds([55.0, 55.0, 10.0, 10.0]),
        &candidates,
        None,
    );
    assert_eq!(selected.as_deref(), Some("inner"));
}

#[test]
fn shared_geometry_drives_edge_scrolling_without_starting_a_dnd_session() {
    let (_owner, (horizontal, vertical)) = create_root(|| {
        let viewport = "shared-geometry";
        let horizontal = use_signal(&scroll_offset_x_signal_key(viewport), || 0.0_f32);
        let vertical = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32);
        use_signal(&scroll_viewport_w_signal_key(viewport), || 200.0_f32);
        use_signal(&scroll_viewport_h_signal_key(viewport), || 200.0_f32);
        use_signal(&scroll_content_w_signal_key(viewport), || 800.0_f32);
        use_signal(&scroll_content_h_signal_key(viewport), || 800.0_f32);
        let views = [ScrollHit {
            rest: viewport.into(),
            rect: Rect::from_bounds([0.0, 0.0, 200.0, 200.0]),
        }];
        assert!(edge_scroll::apply(
            [195.0, 195.0],
            &views,
            AutoScroll::default(),
            1.0 / 60.0,
            Axis::Y,
        ));
        (horizontal.peek(), vertical.peek())
    });
    assert_eq!(horizontal, 0.0);
    assert!(vertical > 0.0);
}
