use crate::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }
    pub fn radius(self) -> f32 {
        self.size().length() * 0.5
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Result<Self> {
        if !origin.is_finite() || !direction.is_finite() || direction.length_squared() < 1e-12 {
            return Err(Error("invalid ray".into()));
        }
        Ok(Self {
            origin,
            direction: direction.normalize(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub node: usize,
    pub primitive: usize,
    pub triangle: usize,
    pub distance: f32,
    pub position: Vec3,
    pub normal: Vec3,
    pub barycentric: Vec3,
    pub uv: Vec2,
}

impl Scene {
    pub fn bounds(&self, pose: &Pose) -> Result<Option<Bounds>> {
        let mut result: Option<Bounds> = None;
        self.visit_vertices(pose, |_, _, _, vertices| {
            for vertex in vertices {
                if let Some(bounds) = &mut result {
                    bounds.min = bounds.min.min(vertex.position);
                    bounds.max = bounds.max.max(vertex.position);
                } else {
                    result = Some(Bounds {
                        min: vertex.position,
                        max: vertex.position,
                    });
                }
            }
            Ok(())
        })?;
        Ok(result)
    }

    /// Closest triangle hit against the current deformed, visible geometry.
    pub fn raycast(&self, pose: &Pose, ray: Ray) -> Result<Option<RayHit>> {
        let ray = Ray::new(ray.origin, ray.direction)?;
        let mut closest: Option<RayHit> = None;
        self.visit_vertices(pose, |node, primitive, source, vertices| {
            for (triangle, indices) in source.indices.chunks_exact(3).enumerate() {
                let mut corners = [vertices.first(); 3];
                for (corner, index) in corners.iter_mut().zip(indices) {
                    *corner = vertices.get(*index as usize);
                }
                let [Some(first), Some(second), Some(third)] = corners else {
                    return Err(Error("triangle index out of range".into()));
                };
                let edge_one = second.position - first.position;
                let edge_two = third.position - first.position;
                let cross = ray.direction.cross(edge_two);
                let determinant = edge_one.dot(cross);
                if determinant.abs() < 1e-8 {
                    continue;
                }
                let relative = ray.origin - first.position;
                let horizontal = relative.dot(cross) / determinant;
                let vertical = ray.direction.dot(relative.cross(edge_one)) / determinant;
                let distance = edge_two.dot(relative.cross(edge_one)) / determinant;
                if horizontal < 0.0
                    || vertical < 0.0
                    || horizontal + vertical > 1.0
                    || distance < 0.0
                    || closest.is_some_and(|hit| hit.distance <= distance)
                {
                    continue;
                }
                let barycentric = Vec3::new(1.0 - horizontal - vertical, horizontal, vertical);
                closest = Some(RayHit {
                    node,
                    primitive,
                    triangle,
                    distance,
                    position: ray.origin + ray.direction * distance,
                    normal: (first.normal * barycentric.x
                        + second.normal * barycentric.y
                        + third.normal * barycentric.z)
                        .normalize_or_zero(),
                    uv: first.uv * barycentric.x
                        + second.uv * barycentric.y
                        + third.uv * barycentric.z,
                    barycentric,
                });
            }
            Ok(())
        })?;
        Ok(closest)
    }

    fn visit_vertices(
        &self,
        pose: &Pose,
        mut visitor: impl FnMut(usize, usize, &Primitive, &[Vertex]) -> Result<()>,
    ) -> Result<()> {
        if pose.visible.len() != self.nodes.len() {
            return Err(Error("pose does not match scene".into()));
        }
        for (node, source) in self
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| pose.visible[*index])
        {
            let Some(mesh) = source.mesh else {
                continue;
            };
            let mesh = self
                .meshes
                .get(mesh)
                .ok_or_else(|| Error("mesh index out of range".into()))?;
            for (primitive, source) in mesh.primitives.iter().enumerate() {
                let vertices = self.deformed_vertices(pose, node, primitive)?;
                visitor(node, primitive, source, &vertices)?;
            }
        }
        Ok(())
    }
}

impl Camera {
    /// Viewport-local physical pixels, with the origin at the upper left.
    pub fn ray(&self, point: Vec2, viewport: Vec2) -> Result<Ray> {
        if !point.is_finite()
            || !viewport.is_finite()
            || viewport.min_element() <= 0.0
            || !self.view_projection.is_finite()
            || self.view_projection.determinant().abs() < 1e-12
        {
            return Err(Error("invalid picking camera or viewport".into()));
        }
        let ndc = Vec2::new(
            point.x / viewport.x * 2.0 - 1.0,
            1.0 - point.y / viewport.y * 2.0,
        );
        let inverse = self.view_projection.inverse();
        let near = inverse.project_point3(ndc.extend(0.0));
        let far = inverse.project_point3(ndc.extend(1.0));
        Ray::new(near, far - near)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            distance: 3.0,
            fov_y: 45.0_f32.to_radians(),
            near: 0.01,
            far: 1000.0,
        }
    }
}

impl OrbitCamera {
    pub fn orbit(&mut self, radians: Vec2) {
        if radians.is_finite() {
            self.yaw = (self.yaw + radians.x).rem_euclid(std::f32::consts::TAU);
            self.pitch = (self.pitch + radians.y).clamp(-1.56, 1.56);
        }
    }
    pub fn zoom(&mut self, logarithmic_delta: f32) {
        if logarithmic_delta.is_finite() {
            self.distance =
                (self.distance * logarithmic_delta.clamp(-20.0, 20.0).exp()).clamp(1e-4, 1e8);
        }
    }
    pub fn fit(&mut self, bounds: Bounds, aspect: f32) -> Result<()> {
        if !bounds.min.is_finite() || !bounds.max.is_finite() || bounds.size().min_element() < 0.0 {
            return Err(Error("invalid bounds".into()));
        }
        self.camera(aspect)?;
        self.target = bounds.center();
        let half_angle = (self.fov_y * 0.5).min(((self.fov_y * 0.5).tan() * aspect).atan());
        self.distance = (bounds.radius() / half_angle.sin() * 1.1).max(0.01);
        self.near = (self.distance - bounds.radius() * 1.5).max(0.001);
        self.far = (self.distance + bounds.radius() * 2.0).max(self.near + 1.0);
        Ok(())
    }
    pub fn camera(&self, aspect: f32) -> Result<Camera> {
        if ![
            self.yaw,
            self.pitch,
            self.distance,
            self.fov_y,
            self.near,
            self.far,
            aspect,
        ]
        .iter()
        .all(|value| value.is_finite())
            || !self.target.is_finite()
            || self.distance <= 0.0
            || aspect <= 0.0
            || !(0.0..std::f32::consts::PI).contains(&self.fov_y)
            || self.fov_y == 0.0
            || self.near <= 0.0
            || self.far <= self.near
            || self.pitch.abs() >= std::f32::consts::FRAC_PI_2
        {
            return Err(Error("invalid orbit camera".into()));
        }
        let position = self.target
            + Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.cos() * self.pitch.cos(),
            ) * self.distance;
        Ok(Camera::perspective(
            position,
            self.target,
            self.fov_y,
            aspect,
            self.near,
            self.far,
        ))
    }
}
