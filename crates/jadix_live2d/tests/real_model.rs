use jadix_live2d::{Player, View};

#[test]
#[ignore = "requires a licensed model: set JADIX_LIVE2D_MODEL and run --ignored"]
fn licensed_model_playback_and_hit_testing() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("JADIX_LIVE2D_MODEL")?;
    let mut player = Player::load(path)?;
    let motions: Vec<_> = player
        .motions()
        .map(|(group, index)| (group.to_owned(), index))
        .collect();
    let expressions: Vec<_> = player.expressions().map(str::to_owned).collect();
    for (group, index) in motions {
        player.play_motion(&group, index, false)?;
        for _ in 0..120 {
            player.update(1.0 / 60.0)?;
        }
    }
    for expression in expressions {
        player.set_expression(&expression)?;
        player.update(0.25)?;
    }
    player.set_look_at(0.5, -0.25)?;
    player.set_mouth(0.75)?;
    player.update(0.25)?;
    player.clear_mouth();
    player.clear_expressions();
    player.stop_motion();
    player.update(0.0)?;
    let meshes = player.runtime().meshes();
    assert!(!meshes.is_empty(), "model must produce drawables");
    for mesh in meshes {
        assert!(mesh.opacity().is_finite());
        assert!(mesh
            .vertices()
            .iter()
            .all(|vertex| vertex.position().iter().all(|value| value.is_finite())));
    }
    let view = View::fit([-1.0, -1.0, 1.0, 1.0], [512, 512], 0.05)?;
    let point = view.model_position([256.0, 256.0]).unwrap();
    let _hit = player.runtime().hit_test(point[0], point[1]);
    assert!(player.update(f32::NAN).is_err());
    assert!(player.update(1.1).is_err());
    assert!(player.set_mouth(f32::INFINITY).is_err());
    Ok(())
}
