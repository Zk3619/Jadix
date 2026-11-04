use jadix_winit::dpi::PhysicalPosition;
use jadix_winit::window::InputRegionRect;

#[test]
fn region_edges_are_half_open_and_zero_size_is_empty() {
    let region = InputRegionRect::new(-10, -20, 20, 40);
    assert!(region.contains(PhysicalPosition::new(-10, -20)));
    assert!(region.contains(PhysicalPosition::new(9, 19)));
    assert!(!region.contains(PhysicalPosition::new(10, 0)));
    assert!(!region.contains(PhysicalPosition::new(0, 20)));
    assert!(!region.contains(PhysicalPosition::new(-11, 0)));
    assert!(!InputRegionRect::new(0, 0, 0, 10).contains(PhysicalPosition::new(0, 0)));
}

#[test]
fn region_arithmetic_handles_extreme_coordinates() {
    let region = InputRegionRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
    assert!(region.contains(PhysicalPosition::new(i32::MIN, i32::MIN)));
    assert!(region.contains(PhysicalPosition::new(i32::MAX - 1, i32::MAX - 1)));
    assert!(!region.contains(PhysicalPosition::new(i32::MAX, 0)));
    assert!(
        InputRegionRect::new(i32::MAX, 0, u32::MAX, 1).contains(PhysicalPosition::new(i32::MAX, 0))
    );
}
