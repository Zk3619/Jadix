use super::AppState;
use winit::dpi::PhysicalSize;

#[test]
fn shrink_prefers_resized_event_not_stale_large_inner() {
    let inner = PhysicalSize::new(800, 947);
    let resized = PhysicalSize::new(800, 525);
    let target = AppState::resolve_surface_size(inner, Some(resized));
    assert_eq!(
        (target.width, target.height),
        (800, 525),
        "shrink must adopt Resized height; max(inner,resized) left layout tall and disabled scroll"
    );
}

#[test]
fn grow_prefers_resized_event_when_inner_lags() {
    let inner = PhysicalSize::new(800, 525);
    let resized = PhysicalSize::new(800, 947);
    let target = AppState::resolve_surface_size(inner, Some(resized));
    assert_eq!((target.width, target.height), (800, 947));
}

#[test]
fn without_resized_event_uses_inner() {
    let inner = PhysicalSize::new(640, 480);
    let target = AppState::resolve_surface_size(inner, None);
    assert_eq!((target.width, target.height), (640, 480));
}

#[test]
fn minimize_zero_size_does_not_pin_layout() {
    let inner = PhysicalSize::new(800, 600);
    let target = AppState::resolve_surface_size(inner, Some(PhysicalSize::new(0, 0)));
    assert_eq!(
        (target.width, target.height),
        (800, 600),
        "0×0 minimize must fall back to inner so restore can rebuild"
    );
}
