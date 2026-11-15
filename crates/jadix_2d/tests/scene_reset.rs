use jadix_2d::*;
fn pixel(scene: &Scene2D) -> [f32; 4] {
    let mut canvas = RasterCanvas::new_linear(8, 8);
    scene.draw(&mut canvas, Default::default()).unwrap();
    canvas.buf[..4].try_into().unwrap()
}
#[test]
fn reset_detaches_snapshots_and_retained_instances() {
    let mut scene = Scene2D::record([8, 8], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0., 0., 8., 8.);
        c.request_frame();
    })
    .unwrap();
    let clone = scene.clone();
    let parent = Scene2D::record([8, 8], |c| {
        c.draw_scene(&scene, Default::default()).unwrap();
    })
    .unwrap();
    scene.reset();
    assert_eq!(scene.stats(), SceneStats::default());
    assert!(!scene.requests_frame());
    assert_eq!(pixel(&scene), [0.; 4]);
    assert_eq!(pixel(&clone), [1., 0., 0., 1.]);
    assert_eq!(pixel(&parent), [1., 0., 0., 1.]);
    scene
        .rerecord([8, 8], Default::default(), |c| {
            c.set_fill("#0000ff");
            c.fill_rect(0., 0., 8., 8.);
        })
        .unwrap();
    assert_eq!(pixel(&scene), [0., 0., 1., 1.]);
    assert_eq!(pixel(&clone), [1., 0., 0., 1.]);
}
#[test]
fn failed_rerecord_and_resize_are_atomic() {
    let mut scene = Scene2D::record([8, 8], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0., 0., 8., 8.);
    })
    .unwrap();
    assert_eq!(scene.reset_and_resize([0, 8]), Err(SceneError::InvalidSize));
    assert_eq!(
        scene.rerecord([8, 8], Default::default(), |c| c.save()),
        Err(SceneError::UnbalancedState)
    );
    assert_eq!(scene.size(), [8, 8]);
    assert_eq!(pixel(&scene), [1., 0., 0., 1.]);
    scene.reset_and_resize([16, 32]).unwrap();
    assert_eq!(scene.size(), [16, 32]);
    assert_eq!(scene.stats(), SceneStats::default());
}
