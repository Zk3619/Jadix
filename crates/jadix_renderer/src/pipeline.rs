use bytemuck::{Pod, Zeroable};
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct VertexInput {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}
impl VertexInput {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Globals {
    pub resolution: [f32; 2],
    pub premultiply: f32,
    /// Shader clip-enable flag; zero for the default unclipped bind group.
    pub _pad: f32,
}

impl Globals {
    pub fn new(width: f32, height: f32, premultiply: f32) -> Self {
        Self {
            resolution: [width, height],
            premultiply,
            _pad: 0.0,
        }
    }
}
