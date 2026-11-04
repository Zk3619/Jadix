//! Independent glTF/GLB and VRM scene assets, animation sampling and GPU rendering.
//! Loading does no GPU work. Rendering uses the same jadix_wgpu device as the host.

mod animation;
mod avatar;
pub mod geometry;
mod loader;
mod playback;
mod render;
mod spatial;

pub use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
pub use playback::{Animator, LocalPose};
pub use render::{Camera, Renderer};
pub use spatial::{Bounds, OrbitCamera, Ray, RayHit};
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}
impl Transform {
    pub fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Option<String>,
    pub children: Vec<usize>,
    pub transform: Transform,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub weights: Vec<f32>,
    pub camera: Option<Projection>,
    pub light: Option<Light>,
}

#[derive(Clone, Copy, Debug)]
pub enum LightKind {
    Directional,
    Point,
    Spot { inner: f32, outer: f32 },
}

#[derive(Clone, Copy, Debug)]
pub enum Projection {
    Perspective {
        fov_y: f32,
        near: f32,
        far: f32,
    },
    Orthographic {
        xmag: f32,
        ymag: f32,
        near: f32,
        far: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub color: Vec3,
    pub intensity: f32,
    pub range: Option<f32>,
    pub kind: LightKind,
}

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub color: Vec4,
    pub joints: [u16; 4],
    pub weights: Vec4,
}

#[derive(Clone, Debug)]
pub struct MorphTarget {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlphaMode {
    Opaque,
    Mask(f32),
    Blend,
}

#[derive(Clone, Debug)]
pub struct TextureMap {
    pub image: usize,
    pub sampler: wgpu::SamplerDescriptor<'static>,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub base_color: Vec4,
    pub base_color_texture: Option<usize>,
    pub metallic_roughness_texture: Option<TextureMap>,
    pub normal_texture: Option<TextureMap>,
    pub normal_scale: f32,
    pub occlusion_texture: Option<TextureMap>,
    pub occlusion_strength: f32,
    pub emissive_texture: Option<TextureMap>,
    pub sampler: wgpu::SamplerDescriptor<'static>,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: Vec3,
    pub alpha: AlphaMode,
    pub double_sided: bool,
    pub unlit: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: Vec4::ONE,
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive_texture: None,
            sampler: wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
            metallic: 1.0,
            roughness: 1.0,
            emissive: Vec3::ZERO,
            alpha: AlphaMode::Opaque,
            double_sided: false,
            unlit: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub morph_targets: Vec<MorphTarget>,
    pub material: Material,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub name: Option<String>,
    pub primitives: Vec<Primitive>,
    pub weights: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Vec<Mat4>,
}

#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Vrm {
    /// Preserves VRM 0.x and VRMC_vrm 1.x extension contents, including metadata,
    /// humanoid mappings, expression definitions and license declarations.
    pub extensions: serde_json::Map<String, serde_json::Value>,
    pub human_bones: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Copy, Debug)]
pub enum Property {
    Translation,
    Rotation,
    Scale,
    Weights,
}
#[derive(Clone, Copy, Debug)]
pub enum Interpolation {
    Step,
    Linear,
    CubicSpline,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub node: usize,
    pub property: Property,
    pub interpolation: Interpolation,
    pub times: Vec<f32>,
    pub values: Vec<f32>,
    pub components: usize,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: Option<String>,
    pub duration: f32,
    pub channels: Vec<Channel>,
}

#[derive(Clone, Debug)]
pub struct SceneDefinition {
    pub name: Option<String>,
    pub roots: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub scenes: Vec<SceneDefinition>,
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
    pub meshes: Vec<Mesh>,
    pub skins: Vec<Skin>,
    pub images: Vec<Image>,
    pub animations: Vec<Animation>,
    pub vrm: Option<Vrm>,
}

#[derive(Clone, Debug)]
pub struct Pose {
    pub world: Vec<Mat4>,
    pub weights: Vec<Vec<f32>>,
    pub visible: Vec<bool>,
}

impl Scene {
    pub fn select_scene(&mut self, index: usize) -> Result<()> {
        let roots = self
            .scenes
            .get(index)
            .ok_or_else(|| Error("scene index out of range".into()))?
            .roots
            .clone();
        if roots.iter().any(|index| *index >= self.nodes.len()) {
            return Err(Error("root node out of range".into()));
        }
        self.roots = roots;
        Ok(())
    }

    /// Load a .gltf, .glb or .vrm file, resolving external buffers/images relative to it.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        loader::load(path.as_ref())
    }

    /// Load a GLB/VRM or self-contained glTF byte buffer (no external asset paths).
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        loader::from_slice(bytes)
    }

    /// Sample a clip at an absolute time. None produces the bind pose.
    pub fn pose(&self, clip: Option<usize>, seconds: f32, repeat: bool) -> Result<Pose> {
        animation::pose(self, clip, seconds, repeat)
    }

    /// CPU-deformed vertices in world space, including morph targets and skin joints.
    pub fn deformed_vertices(
        &self,
        pose: &Pose,
        node_index: usize,
        primitive_index: usize,
    ) -> Result<Vec<Vertex>> {
        let node = self
            .nodes
            .get(node_index)
            .ok_or_else(|| Error("node index out of range".into()))?;
        let mesh = node
            .mesh
            .and_then(|index| self.meshes.get(index))
            .ok_or_else(|| Error("node has no mesh".into()))?;
        let primitive = mesh
            .primitives
            .get(primitive_index)
            .ok_or_else(|| Error("primitive index out of range".into()))?;
        let model = *pose
            .world
            .get(node_index)
            .ok_or_else(|| Error("pose does not match scene".into()))?;
        let morph_weights = pose
            .weights
            .get(node_index)
            .ok_or_else(|| Error("pose does not match scene".into()))?;
        let skin = node
            .skin
            .map(|index| {
                self.skins
                    .get(index)
                    .ok_or_else(|| Error("skin index out of range".into()))
            })
            .transpose()?;
        let mut vertices = primitive.vertices.clone();
        for (vertex_index, vertex) in vertices.iter_mut().enumerate() {
            for (morph, weight) in primitive.morph_targets.iter().zip(morph_weights) {
                if let Some(delta) = morph.positions.get(vertex_index) {
                    vertex.position += delta * *weight;
                }
                if let Some(delta) = morph.normals.get(vertex_index) {
                    vertex.normal += delta * *weight;
                }
            }
            let transform = if let Some(skin) = skin {
                let mut transform = Mat4::ZERO;
                let total = vertex.weights.element_sum();
                if total > 0.0 {
                    for influence in 0..4 {
                        let weight = vertex.weights[influence] / total;
                        if weight == 0.0 {
                            continue;
                        }
                        let joint = vertex.joints[influence] as usize;
                        let world = skin
                            .joints
                            .get(joint)
                            .and_then(|index| pose.world.get(*index))
                            .ok_or_else(|| Error("skin joint index out of range".into()))?;
                        let inverse = skin
                            .inverse_bind_matrices
                            .get(joint)
                            .ok_or_else(|| Error("missing inverse bind matrix".into()))?;
                        transform += *world * *inverse * weight;
                    }
                    transform
                } else {
                    model
                }
            } else {
                model
            };
            vertex.position = transform.transform_point3(vertex.position);
            vertex.normal = if transform.determinant().abs() > 1e-10 {
                transform
                    .inverse()
                    .transpose()
                    .transform_vector3(vertex.normal)
                    .normalize_or_zero()
            } else {
                Vec3::Y
            };
        }
        Ok(vertices)
    }
}
