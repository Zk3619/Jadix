use super::*;

fn frameless() -> WindowChrome {
    WindowChrome {
        decorated: false,
        buttons: WindowButtons::all(),
        resizable: true,
        maximized: false,
        fullscreen: false,
        tiled: TiledEdges::default(),
    }
}

#[test]
fn shadow_is_not_a_resize_extension_line_and_tiled_edges_stay_attached() {
    use ResizeDirection::*;
    let size = [300.0, 200.0];
    let tiled = TiledEdges::default();
    let insets = tiled.frame_insets(20.0);
    for (point, direction) in [
        ([20.0, 20.0], NorthWest),
        ([150.0, 20.0], North),
        ([280.0, 20.0], NorthEast),
        ([20.0, 100.0], West),
        ([280.0, 100.0], East),
        ([20.0, 180.0], SouthWest),
        ([150.0, 180.0], South),
        ([280.0, 180.0], SouthEast),
    ] {
        assert_eq!(
            resize_direction_at_frame(size, point, insets, tiled, 4.0),
            Some(direction)
        );
    }
    for point in [
        [1.0, 20.0],
        [20.0, 1.0],
        [1.0, 1.0],
        [150.0, 100.0],
        [300.0, 20.0],
        [f64::NAN, 20.0],
    ] {
        assert_eq!(
            resize_direction_at_frame(size, point, insets, tiled, 4.0),
            None
        );
    }
    let tiled = TiledEdges {
        top: true,
        left: true,
        ..tiled
    };
    assert_eq!(tiled.frame_insets(20.0), [0.0, 20.0, 20.0, 0.0]);
    assert_eq!(
        resize_direction_at_frame(size, [0.0, 0.0], tiled.frame_insets(20.0), tiled, 4.0),
        None
    );
    assert_eq!(
        resize_direction_at_frame(size, [280.0, 0.0], tiled.frame_insets(20.0), tiled, 4.0),
        Some(East)
    );
    assert_eq!(
        resize_direction_at_frame(size, [1.0, 1.0], [160.0; 4], tiled, 4.0),
        None
    );
    assert_eq!(
        resize_direction_at_frame(size, [1.0, 1.0], [0.0; 4], tiled, f64::INFINITY),
        None
    );
}

#[test]
fn every_tiling_combination_and_live_resize_policy_is_checked() {
    use ResizeDirection::*;
    for bits in 0..16 {
        let tiled = TiledEdges {
            top: bits & 1 != 0,
            right: bits & 2 != 0,
            bottom: bits & 4 != 0,
            left: bits & 8 != 0,
        };
        let mut state = WindowChrome {
            tiled,
            ..frameless()
        };
        for (direction, mask) in [
            (North, 1),
            (East, 2),
            (South, 4),
            (West, 8),
            (NorthEast, 3),
            (SouthEast, 6),
            (SouthWest, 12),
            (NorthWest, 9),
        ] {
            assert_eq!(state.can_resize(direction), bits & mask == 0);
        }
        for mode in 0..3 {
            state = WindowChrome {
                tiled,
                ..frameless()
            };
            match mode {
                0 => state.resizable = false,
                1 => state.fullscreen = true,
                _ => state.maximized = true,
            }
            for direction in [
                North, East, South, West, NorthEast, SouthEast, SouthWest, NorthWest,
            ] {
                assert!(!state.can_resize(direction));
            }
        }
    }
}

#[test]
fn resize_hit_test_covers_eight_edges_and_rejects_invalid_geometry() {
    use crate::window::ResizeDirection::*;
    for (point, direction) in [
        ([1.0, 1.0], NorthWest),
        ([50.0, 1.0], North),
        ([99.0, 1.0], NorthEast),
        ([1.0, 40.0], West),
        ([99.0, 40.0], East),
        ([1.0, 79.0], SouthWest),
        ([50.0, 79.0], South),
        ([99.0, 79.0], SouthEast),
    ] {
        assert_eq!(
            resize_direction_at([100.0, 80.0], point, 5.0),
            Some(direction)
        );
    }
    for point in [
        [50.0, 40.0],
        [-1.0, 5.0],
        [100.0, 5.0],
        [1.0, 80.0],
        [f64::NAN, 0.0],
    ] {
        assert_eq!(resize_direction_at([100.0, 80.0], point, 5.0), None);
    }
    assert_eq!(resize_direction_at([0.0, 80.0], [0.0, 1.0], 5.0), None);
    assert_eq!(
        resize_direction_at([100.0, 80.0], [0.0, 1.0], f64::INFINITY),
        None
    );
    assert_eq!(resize_direction_at([100.0, 80.0], [0.0, 1.0], 0.0), None);
    assert_eq!(
        resize_direction_at([2.0, 2.0], [1.0, 1.0], 5.0),
        Some(SouthEast)
    );
    // Fractional logical coordinates: no rounding to physical integer pixels.
    assert_eq!(
        resize_direction_at([100.5, 80.5], [99.75, 40.0], 1.0),
        Some(East)
    );
}
