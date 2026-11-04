mod common;
use jadix_3d::*;
use serde_json::json;

#[test]
fn playback_pause_reverse_and_interruptible_fade() {
    let scene = Scene::from_slice(&common::model(None)).unwrap();
    let mut clock = Animator::default();
    clock.play(&scene, Some(0), 0.0, false).unwrap();
    clock.update(0.8).unwrap();
    assert!(
        (clock.sample_local(&scene).unwrap().transforms[1]
            .translation
            .x
            - 0.8)
            .abs()
            < 1e-5
    );
    clock.set_playing(false);
    clock.update(0.3).unwrap();
    assert!((clock.time() - 0.8).abs() < 1e-5);
    clock.set_playing(true);
    clock.set_speed(-1.0).unwrap();
    clock.update(0.2).unwrap();
    assert!((clock.time() - 0.6).abs() < 1e-5);
    clock.play(&scene, None, 1.0, true).unwrap();
    clock.update(0.5).unwrap();
    let halfway = clock.sample_local(&scene).unwrap();
    assert!((halfway.transforms[1].translation.x - 0.3).abs() < 1e-5);
    clock.play(&scene, Some(0), 1.0, false).unwrap();
    assert_eq!(
        halfway.transforms[1].translation,
        clock.sample_local(&scene).unwrap().transforms[1].translation
    );
    assert!(clock.play(&scene, Some(100), 1.0, false).is_err());
    assert_eq!(
        halfway.transforms[1].translation,
        clock.sample_local(&scene).unwrap().transforms[1].translation
    );
}

#[test]
fn bounds_picking_and_scene_visibility() {
    let mut scene = Scene::default();
    scene.add_mesh(
        "box",
        vec![geometry::cuboid(Vec3::splat(2.0), Material::default()).unwrap()],
        Transform::default(),
    );
    let pose = scene.pose(None, 0.0, false).unwrap();
    let bounds = scene.bounds(&pose).unwrap().unwrap();
    assert_eq!(bounds.min, Vec3::splat(-1.0));
    assert_eq!(bounds.max, Vec3::ONE);
    let ray = Ray::new(Vec3::Z * 4.0, -Vec3::Z).unwrap();
    let hit = scene.raycast(&pose, ray).unwrap().unwrap();
    assert!((hit.distance - 3.0).abs() < 1e-5);
    assert!((hit.barycentric.element_sum() - 1.0).abs() < 1e-5);
    scene.roots.clear();
    let invisible = scene.pose(None, 0.0, false).unwrap();
    assert!(scene.bounds(&invisible).unwrap().is_none());
    assert!(scene.raycast(&invisible, ray).unwrap().is_none());
}

#[test]
fn vrm_expression_versions_and_atomic_errors() {
    for (extension, value) in [
        (
            "VRMC_vrm",
            json!({"specVersion":"1.0", "expressions":{"preset":{"happy":{"morphTargetBinds":[{"node":0,"index":0,"weight":0.8}]}}}}),
        ),
        (
            "VRM",
            json!({"blendShapeMaster":{"blendShapeGroups":[{"name":"happy","binds":[{"mesh":0,"index":0,"weight":80.0}]}]}}),
        ),
    ] {
        let mut scene = Scene::from_slice(&common::model(Some((extension, value)))).unwrap();
        let mut pose = scene.local_pose(None, 0.0, false).unwrap();
        scene.apply_expression(&mut pose, "happy", 0.5).unwrap();
        assert!((pose.weights[0][0] - 0.4).abs() < 1e-5);
        scene.apply_expression(&mut pose, "happy", 0.5).unwrap();
        assert!((pose.weights[0][0] - 0.8).abs() < 1e-5);
        scene.vrm.as_mut().unwrap().extensions.insert("VRMC_vrm".into(), json!({"expressions":{"preset":{"invalid":{"morphTargetBinds":[{"node":0,"index":0,"weight":1.0},{"node":0,"index":200,"weight":1.0}]}}}}));
        let before = pose.weights.clone();
        assert!(scene.apply_expression(&mut pose, "invalid", 1.0).is_err());
        assert_eq!(pose.weights, before);
    }
}

#[test]
fn ccd_ik_reaches_target_without_mutating_scene() {
    let mut scene = Scene::from_slice(&common::model(None)).unwrap();
    scene.nodes[0].mesh = None;
    scene.nodes[0].skin = None;
    scene.nodes[0].transform = Transform::default();
    scene.nodes[0].children = vec![1];
    scene.nodes[1].transform.translation = Vec3::X;
    scene.nodes[2].transform.translation = Vec3::X;
    scene.roots = vec![0];
    let mut pose = scene.local_pose(None, 0.0, false).unwrap();
    let target = Vec3::new(1.0, 1.0, 0.0);
    assert!(scene.solve_ik(&mut pose, 2, 2, target, 64, 0.001).unwrap() < 0.001);
    assert!(
        (scene.evaluate_pose(pose).unwrap().world[2].transform_point3(Vec3::ZERO) - target)
            .length()
            < 0.001
    );
    assert_eq!(scene.nodes[0].transform.rotation, Quat::IDENTITY);
}
