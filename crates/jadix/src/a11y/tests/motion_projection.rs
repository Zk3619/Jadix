#[test]
fn accessibility_bounds_follow_layout_projection() {
    use crate::core::tree::Element;
    use crate::motion::{LayoutAnimation, LayoutMotion};
    use crate::style::{tick_animations, TimingFunction, ValueDriver};

    let motion = LayoutMotion::new(LayoutAnimation::Both)
        .transition(ValueDriver::tween(100.0, TimingFunction::Linear));
    motion.update([20.0, 10.0, 100.0, 60.0]);
    motion.update([120.0, 10.0, 200.0, 60.0]);
    let mut element = Element::new().layout_motion(&motion);
    element.layout_bounds = [120.0, 10.0, 200.0, 60.0];
    let (initial_x, initial_width, middle_x, middle_width) = if cfg!(feature = "animation-ex") {
        (20.0, 100.0, 70.0, 150.0)
    } else {
        (120.0, 200.0, 120.0, 200.0)
    };
    let initial = super::effective_bounds(&element).unwrap();
    assert!((initial.x0 - initial_x).abs() < 0.01);
    assert!((initial.x1 - initial.x0 - initial_width).abs() < 0.01);
    assert_eq!(motion.is_active(), cfg!(feature = "animation-ex"));
    tick_animations(0.05);
    let middle = super::effective_bounds(&element).unwrap();
    assert!((middle.x0 - middle_x).abs() < 0.01);
    assert!((middle.x1 - middle.x0 - middle_width).abs() < 0.01);
    tick_animations(0.05);
    let final_bounds = super::effective_bounds(&element).unwrap();
    assert!((final_bounds.x0 - 120.0).abs() < 0.01);
    assert!((final_bounds.x1 - final_bounds.x0 - 200.0).abs() < 0.01);
}
