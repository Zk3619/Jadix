use crate::*;

fn vertex(position: Vec3, normal: Vec3, uv: Vec2) -> Vertex {
    Vertex {
        position,
        normal,
        uv,
        color: Vec4::ONE,
        joints: [0; 4],
        weights: Vec4::ZERO,
    }
}

/// XZ plane, facing +Y, with UVs covering the unit square.
pub fn plane(width: f32, depth: f32, material: Material) -> Result<Primitive> {
    if !width.is_finite() || !depth.is_finite() || width <= 0.0 || depth <= 0.0 {
        return Err(Error("plane dimensions must be positive and finite".into()));
    }
    Ok(Primitive {
        vertices: vec![
            vertex(
                Vec3::new(-width * 0.5, 0.0, -depth * 0.5),
                Vec3::Y,
                Vec2::ZERO,
            ),
            vertex(Vec3::new(width * 0.5, 0.0, -depth * 0.5), Vec3::Y, Vec2::X),
            vertex(Vec3::new(width * 0.5, 0.0, depth * 0.5), Vec3::Y, Vec2::ONE),
            vertex(Vec3::new(-width * 0.5, 0.0, depth * 0.5), Vec3::Y, Vec2::Y),
        ],
        indices: vec![0, 2, 1, 0, 3, 2],
        morph_targets: vec![],
        material,
    })
}

/// Six independent faces, preserving hard normals and per-face UVs.
pub fn cuboid(size: Vec3, material: Material) -> Result<Primitive> {
    if !size.is_finite() || size.min_element() <= 0.0 {
        return Err(Error("box size must be positive and finite".into()));
    }
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, horizontal, vertical) in [
        (Vec3::X, -Vec3::Z, Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, -Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, -Vec3::X, Vec3::Y),
    ] {
        let start = vertices.len() as u32;
        for uv in [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y] {
            vertices.push(vertex(
                (normal + horizontal * (uv.x * 2.0 - 1.0) + vertical * (uv.y * 2.0 - 1.0))
                    * size
                    * 0.5,
                normal,
                uv,
            ));
        }
        indices.extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    Ok(Primitive {
        vertices,
        indices,
        morph_targets: vec![],
        material,
    })
}

pub fn uv_sphere(radius: f32, sectors: u32, rings: u32, material: Material) -> Result<Primitive> {
    if !radius.is_finite()
        || radius <= 0.0
        || !(3..=1024).contains(&sectors)
        || !(2..=1024).contains(&rings)
    {
        return Err(Error(
            "sphere requires positive radius, 3..1024 sectors and 2..1024 rings".into(),
        ));
    }
    let mut vertices = Vec::new();
    for ring in 0..=rings {
        for sector in 0..=sectors {
            let uv = Vec2::new(sector as f32 / sectors as f32, ring as f32 / rings as f32);
            let latitude = uv.y * std::f32::consts::PI;
            let longitude = uv.x * std::f32::consts::TAU;
            let normal = Vec3::new(
                latitude.sin() * longitude.cos(),
                latitude.cos(),
                latitude.sin() * longitude.sin(),
            );
            vertices.push(vertex(normal * radius, normal, uv));
        }
    }
    let mut indices = Vec::new();
    for ring in 0..rings {
        for sector in 0..sectors {
            let first = ring * (sectors + 1) + sector;
            let next = first + sectors + 1;
            if ring > 0 {
                indices.extend([first, first + 1, next]);
            }
            if ring + 1 < rings {
                indices.extend([first + 1, next + 1, next]);
            }
        }
    }
    Ok(Primitive {
        vertices,
        indices,
        morph_targets: vec![],
        material,
    })
}

impl Scene {
    /// Add a standalone mesh root; children can subsequently be edited through nodes.
    pub fn add_mesh(
        &mut self,
        name: impl Into<String>,
        primitives: Vec<Primitive>,
        transform: Transform,
    ) -> usize {
        let name = name.into();
        let mesh = self.meshes.len();
        self.meshes.push(Mesh {
            name: Some(name.clone()),
            primitives,
            weights: vec![],
        });
        let node = self.nodes.len();
        self.nodes.push(Node {
            name: Some(name),
            children: vec![],
            transform,
            mesh: Some(mesh),
            skin: None,
            weights: vec![],
            camera: None,
            light: None,
        });
        self.roots.push(node);
        node
    }
}
