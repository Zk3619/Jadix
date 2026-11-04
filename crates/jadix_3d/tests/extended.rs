use jadix_3d::*;

#[test]
fn generated_geometry_has_finite_vertices_and_valid_triangles() {
    let material = Material::default();
    for primitive in [
        geometry::plane(2.0, 3.0, material.clone()).unwrap(),
        geometry::cuboid(Vec3::ONE, material.clone()).unwrap(),
        geometry::uv_sphere(1.0, 16, 8, material).unwrap(),
    ] {
        assert!(!primitive.vertices.is_empty());
        assert_eq!(primitive.indices.len() % 3, 0);
        assert!(primitive
            .indices
            .iter()
            .all(|index| (*index as usize) < primitive.vertices.len()));
        assert!(primitive
            .vertices
            .iter()
            .all(|vertex| vertex.position.is_finite()
                && vertex.normal.is_finite()
                && vertex.uv.is_finite()));
        for triangle in primitive.indices.chunks_exact(3) {
            let [first, second, third] = [triangle[0], triangle[1], triangle[2]]
                .map(|index| primitive.vertices[index as usize]);
            let cross = (second.position - first.position).cross(third.position - first.position);
            assert!(cross.dot(first.normal + second.normal + third.normal) > 0.0);
        }
    }
}

#[test]
fn orbit_camera_and_ray_input_validation() {
    let mut camera = OrbitCamera::default();
    assert!(Ray::new(Vec3::ZERO, Vec3::ZERO).is_err());
    let distance = camera.distance;
    camera.zoom(f32::NAN);
    assert_eq!(camera.distance, distance);
    let camera = camera.camera(1.0).unwrap();
    let ray = camera.ray(Vec2::splat(50.0), Vec2::splat(100.0)).unwrap();
    assert!(ray.direction.is_normalized());
}
