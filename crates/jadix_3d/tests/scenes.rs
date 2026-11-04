mod common;
use jadix_3d::*;
use serde_json::json;

fn scene() -> Scene {
    Scene::from_slice(&common::model(None)).unwrap()
}
fn close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.distance(expected) < 1e-5,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn glb_hierarchy_and_skin_use_joint_world_not_mesh_world() {
    let scene = scene();
    let pose = scene.pose(Some(0), 0.5, false).unwrap();
    close(
        pose.world[2].transform_point3(Vec3::ZERO),
        Vec3::new(0.5, 1.0, 0.0),
    );
    close(
        scene.deformed_vertices(&pose, 0, 0).unwrap()[0].position,
        Vec3::new(0.0, -0.5, 0.0),
    );
    assert_eq!(scene.meshes[0].primitives[0].indices, [0, 1, 2]);
}

#[test]
fn morph_is_applied_before_skinning() {
    let mut scene = scene();
    scene.nodes[0].weights = vec![0.5];
    let pose = scene.pose(Some(0), 0.5, false).unwrap();
    close(
        scene.deformed_vertices(&pose, 0, 0).unwrap()[0].position,
        Vec3::new(0.0, -0.375, 0.0),
    );
}

#[test]
fn looping_and_clamping_have_distinct_endpoints() {
    let scene = scene();
    close(
        scene.pose(Some(0), 1.0, true).unwrap().world[1].transform_point3(Vec3::ZERO),
        Vec3::ZERO,
    );
    close(
        scene.pose(Some(0), 3.0, false).unwrap().world[1].transform_point3(Vec3::ZERO),
        Vec3::X,
    );
    close(
        scene.pose(Some(0), -1.0, false).unwrap().world[1].transform_point3(Vec3::ZERO),
        Vec3::ZERO,
    );
}

#[test]
fn step_and_cubic_interpolation_follow_gltf_tangents() {
    let mut scene = scene();
    scene.animations[0].channels[0].interpolation = Interpolation::Step;
    close(
        scene.pose(Some(0), 0.5, false).unwrap().world[1].transform_point3(Vec3::ZERO),
        Vec3::ZERO,
    );
    let channel = &mut scene.animations[0].channels[0];
    channel.interpolation = Interpolation::CubicSpline;
    channel.values = vec![
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    close(
        scene.pose(Some(0), 0.5, false).unwrap().world[1].transform_point3(Vec3::ZERO),
        Vec3::new(0.75, 0.0, 0.0),
    );
}

#[test]
fn quaternion_interpolation_and_zero_quaternions() {
    let mut scene = scene();
    let channel = &mut scene.animations[0].channels[0];
    channel.property = Property::Rotation;
    channel.components = 4;
    channel.values = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];
    close(
        scene.pose(Some(0), 0.5, false).unwrap().world[1].transform_vector3(Vec3::X),
        Vec3::Y,
    );
    scene.animations[0].channels[0].values[0..4].fill(0.0);
    assert!(scene.pose(Some(0), 0.0, false).is_err());
}

#[test]
fn cycles_bad_channels_and_bad_joints_fail() {
    let mut scene = scene();
    scene.nodes[2].children.push(1);
    assert!(scene.pose(None, 0.0, false).is_err());
    scene.nodes[2].children.clear();
    scene.animations[0].channels[0].times[1] = 0.0;
    assert!(scene.pose(Some(0), 0.5, false).is_err());
    scene.skins[0].joints[0] = usize::MAX;
    let pose = scene.pose(None, 0.0, false).unwrap();
    assert!(scene.deformed_vertices(&pose, 0, 0).is_err());
    assert!(scene.pose(None, f32::NAN, false).is_err());
}

#[test]
fn selected_scene_controls_visibility() {
    let mut scene = scene();
    assert_eq!(scene.scenes[1].name.as_deref(), Some("other"));
    scene.select_scene(1).unwrap();
    assert_eq!(
        scene.pose(None, 0.0, false).unwrap().visible,
        [false, false, true]
    );
    assert!(scene.select_scene(99).is_err());
}

#[test]
fn vrm_zero_and_one_keep_humanoid_and_metadata() {
    for (name, data) in [
        (
            "VRM",
            json!({"meta":{"title":"sample"},"humanoid":{"humanBones":[{"bone":"hips","node":1}]}}),
        ),
        (
            "VRMC_vrm",
            json!({"specVersion":"1.0","humanoid":{"humanBones":{"hips":{"node":1}}}}),
        ),
    ] {
        let scene = Scene::from_slice(&common::model(Some((name, data.clone())))).unwrap();
        let vrm = scene.vrm.unwrap();
        assert_eq!(vrm.human_bones["hips"], 1);
        assert_eq!(vrm.extensions[name], data);
    }
}

#[test]
fn invalid_files_return_errors() {
    assert!(Scene::from_slice(b"not a model").is_err());
    assert!(Scene::from_slice(&common::model(None)[..48]).is_err());
    assert!(Scene::from_slice(&common::model(Some(("EXT_unknown", json!({}))))).is_err());
}
