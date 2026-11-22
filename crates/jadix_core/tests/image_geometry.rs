use jadix_core::presentation::style::{ImageFit, ImagePosition};

#[test]
fn fitting_preserves_aspect_clips_cover_and_positions_remaining_space() {
    let target = [100., 100.];
    let image = [200., 50.];
    assert_eq!(ImageFit::Contain.geometry(target, image, ImagePosition::CENTER), Some([0., 37.5, 100., 25.]));
    assert_eq!(ImageFit::Cover.geometry(target, image, ImagePosition::CENTER), Some([-150., 0., 400., 100.]));
    assert_eq!(ImageFit::Fill.geometry(target, image, ImagePosition::CENTER), Some([0., 0., 100., 100.]));
    assert_eq!(ImageFit::None.geometry(target, image, ImagePosition::new([1., 0.], [-4., 7.])), Some([-104., 7., 200., 50.]));
    assert_eq!(ImageFit::ScaleDown.geometry(target, [20., 10.], ImagePosition::CENTER), Some([40., 45., 20., 10.]));
    assert!(ImageFit::Cover.geometry(target, [0., 1.], ImagePosition::CENTER).is_none());
    assert!(ImageFit::Cover.geometry(target, image, ImagePosition::new([f32::NAN, 0.], [0.; 2])).is_none());
}
