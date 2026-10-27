use jadix::style::{Inertia, InertiaOptions, Spring, SpringOptions, SpringPhysics};

#[test]
fn spring_and_inertia_have_finite_samples_and_settle() {
    let spring = Spring::from_physics(0.0, 100.0, 0.0, SpringPhysics::default_css());
    for milliseconds in [0.0, 16.0, 100.0, 500.0] {
        let sample = spring.next(milliseconds);
        assert!(sample.value.is_finite() && sample.velocity.is_finite());
    }
    assert_eq!(spring.next(20_000.0).value, 100.0);
    let mut inertia = Inertia::new(
        0.0,
        InertiaOptions {
            velocity: 600.0,
            ..Default::default()
        },
    );
    assert!(inertia.next(100.0).value.is_finite());
    assert!(inertia.next(20_000.0).done);
}

#[test]
fn feature_selection_preserves_immediate_completion_contract() {
    let spring = Spring::new(0.0, 1.0, 0.0, SpringOptions::default());
    let first = spring.next(0.0);
    if cfg!(feature = "animation-base") {
        assert!(!first.done);
        assert_eq!(first.value, 0.0);
    } else {
        assert!(first.done);
        assert_eq!(first.value, 1.0);
    }
}
