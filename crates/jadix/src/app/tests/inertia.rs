use super::decay_scroll_velocity;

#[test]
fn decay_scroll_velocity_reduces_and_stops() {
    let [vx, vy] = decay_scroll_velocity(400.0, -200.0, 0.016, 4.5);
    assert!(vx.abs() < 400.0 && vx > 0.0);
    assert!(vy.abs() < 200.0 && vy < 0.0);
    let mut v = [400.0_f32, 0.0];
    for _ in 0..120 {
        v = decay_scroll_velocity(v[0], v[1], 0.016, 4.5);
    }
    assert_eq!(v, [0.0, 0.0]);
}
