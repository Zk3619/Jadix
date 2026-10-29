use super::{wheel_delta_for_scroll_range, wheel_delta_logical_px, WHEEL_LINE_HEIGHT_PX};
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

#[test]
fn vertical_wheel_moves_horizontal_only_viewports() {
    assert_eq!(
        wheel_delta_for_scroll_range(0.0, -75.0, 300.0, 0.0),
        (-75.0, 0.0)
    );
    assert_eq!(
        wheel_delta_for_scroll_range(0.0, 75.0, 300.0, 0.0),
        (75.0, 0.0)
    );
}

#[test]
fn wheel_preserves_vertical_and_explicit_horizontal_input() {
    for (dx, dy, max_x, max_y) in [
        (0.0, -75.0, 0.0, 300.0),
        (0.0, -75.0, 300.0, 300.0),
        (-25.0, -75.0, 300.0, 0.0),
        (0.0, -75.0, 0.0, 0.0),
    ] {
        assert_eq!(wheel_delta_for_scroll_range(dx, dy, max_x, max_y), (dx, dy));
    }
}

#[test]
fn line_delta_is_lines_not_notches_times_spi() {
    // winit 0.31 Windows: one detent with SPI=3 arrives as LineDelta(0, 3).
    // Re-multiplying by SPI (old 0.30 path) would be 3 * 3 * 25 = 225.
    let (dx, dy) = wheel_delta_logical_px(MouseScrollDelta::LineDelta(0.0, 3.0), 1.5);
    assert!((dx).abs() < 1e-4);
    assert!((dy - 3.0 * WHEEL_LINE_HEIGHT_PX).abs() < 1e-4);
}

#[test]
fn pixel_delta_is_physical_to_logical() {
    let (dx, dy) = wheel_delta_logical_px(
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 150.0)),
        1.5,
    );
    assert!((dx).abs() < 1e-4);
    assert!((dy - 100.0).abs() < 1e-4);
}
