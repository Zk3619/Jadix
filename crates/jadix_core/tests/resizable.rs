use jadix_core::resizable::resize_pair;

#[test]
fn adjacent_constraints_are_intersected_without_changing_other_panels() {
    assert_eq!(resize_pair([30., 50.], 60., [[10., 70.], [25., 60.]]), Some([55., 25.]));
    assert_eq!(resize_pair([30., 50.], -60., [[10., 70.], [25., 60.]]), Some([20., 60.]));
    assert_eq!(resize_pair([30., 50.], 1., [[60., 70.], [25., 60.]]), None);
    assert_eq!(resize_pair([30., 50.], f64::NAN, [[10., 70.], [25., 60.]]), None);
    assert_eq!(resize_pair([f64::MAX, f64::MAX], 1., [[0., f64::MAX]; 2]), None);
}
