use jadix_core::range::Range;

#[test]
fn continuous_pointer_values_and_discrete_steps_keep_endpoints_reachable() {
    let range = Range::new(-5., 5., 3.).unwrap();
    assert_eq!(range.at_fraction(0.47), Some(-0.3000000000000007));
    assert_eq!(range.snap(0.3), Some(1.));
    assert_eq!(range.advance(4., 1.), Some(5.));
    assert_eq!(range.advance(-5., -1.), Some(-5.));
    assert_eq!(range.at_fraction(2.), Some(5.));
    assert_eq!(range.fraction(f64::NAN), None);
    assert!(Range::new(-f64::MAX, f64::MAX, 1.).is_err());
    assert!(Range::new(0., 1., f64::MIN_POSITIVE / 100.).is_err());
}
