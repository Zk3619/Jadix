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
    let initial = super::effective_bounds(&element).unwrap();
    assert!((initial.x0 - 20.0).abs() < 0.01);
    assert!((initial.x1 - initial.x0 - 100.0).abs() < 0.01);
    tick_animations(0.05);
    let middle = super::effective_bounds(&element).unwrap();
    assert!((middle.x0 - 70.0).abs() < 0.01);
    assert!((middle.x1 - middle.x0 - 150.0).abs() < 0.01);
}
