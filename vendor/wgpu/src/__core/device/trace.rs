#[cfg(jadix_wgpu_core_feature_trace)]
mod record;
#[cfg(jadix_wgpu_core_feature_replay)]
mod replay;

use core::convert::Infallible;

use alloc::{borrow::Cow, string::String, vec::Vec};
use macro_rules_attribute::apply;

use crate::__core::{
    command::{serde_object_reference_struct, BasePass, Command, ReferenceType, RenderCommand},
    id::{markers, PointerId},
    pipeline::GeneralRenderPipelineDescriptor,
};

#[cfg(jadix_wgpu_core_feature_trace)]
pub use record::*;
#[cfg(jadix_wgpu_core_feature_replay)]
pub use replay::*;

type FileName = String;

pub const FILE_NAME: &str = "trace.ron";

#[derive(Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Data {
    File(FileName),
    String(DataKind, String),
    Binary(DataKind, Vec<u8>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum DataKind {
    Bin,
    Wgsl,

    /// IR of Naga module, serialized in RON format
    Ron,
    Spv,
    Dxil,
    Hlsl,
    MetalLib,
    Msl,
    Glsl,
}

impl core::fmt::Display for DataKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            DataKind::Bin => "bin",
            DataKind::Wgsl => "wgsl",
            DataKind::Ron => "ron",
            DataKind::Spv => "spv",
            DataKind::Dxil => "dxil",
            DataKind::Hlsl => "hlsl",
            DataKind::MetalLib => "metallib",
            DataKind::Msl => "metal",
            DataKind::Glsl => "glsl",
        };
        write!(f, "{s}")
    }
}

impl DataKind {
    #[cfg(jadix_wgpu_core_feature_replay)]
    fn is_string(&self) -> bool {
        match *self {
            DataKind::Wgsl | DataKind::Ron | DataKind::Hlsl | DataKind::Msl | DataKind::Glsl => {
                true
            }
            DataKind::Bin | DataKind::Spv | DataKind::Dxil | DataKind::MetalLib => false,
        }
    }
}

impl Data {
    pub fn kind(&self) -> DataKind {
        match self {
            Data::File(file) => {
                if file.ends_with(".bin") {
                    DataKind::Bin
                } else if file.ends_with(".wgsl") {
                    DataKind::Wgsl
                } else if file.ends_with(".ron") {
                    DataKind::Ron
                } else if file.ends_with(".spv") {
                    DataKind::Spv
                } else if file.ends_with(".dxil") {
                    DataKind::Dxil
                } else if file.ends_with(".hlsl") {
                    DataKind::Hlsl
                } else if file.ends_with(".metallib") {
                    DataKind::MetalLib
                } else if file.ends_with(".metal") {
                    DataKind::Msl
                } else if file.ends_with(".glsl") {
                    DataKind::Glsl
                } else {
                    panic!("unknown data file extension: {file}");
                }
            }
            Data::String(kind, _) => *kind,
            Data::Binary(kind, _) => *kind,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
#[apply(serde_object_reference_struct)]
pub enum Action<'a, R: ReferenceType> {
    Init {
        desc: crate::__core::device::DeviceDescriptor<'a>,
        backend: crate::__types::Backend,
    },
    ConfigureSurface(
        R::Surface,
        crate::__types::SurfaceConfiguration<Vec<crate::__types::TextureFormat>>,
    ),
    CreateBuffer(R::Buffer, crate::__core::resource::BufferDescriptor<'a>),
    DestroyBuffer(R::Buffer),
    DropBuffer(R::Buffer),
    CreateTexture(R::Texture, crate::__core::resource::TextureDescriptor<'a>),
    CreateTextureError(R::Texture, crate::__core::resource::TextureDescriptor<'a>),
    DestroyTexture(R::Texture),
    DropTexture(R::Texture),
    CreateTextureView {
        id: R::TextureView,
        parent: R::Texture,
        desc: crate::__core::resource::TextureViewDescriptor<'a>,
    },
    DropTextureView(R::TextureView),
    CreateExternalTexture {
        id: R::ExternalTexture,
        desc: crate::__core::resource::ExternalTextureDescriptor<'a>,
        planes: alloc::boxed::Box<[R::TextureView]>,
    },
    DestroyExternalTexture(R::ExternalTexture),
    DropExternalTexture(R::ExternalTexture),
    CreateSampler(
        PointerId<markers::Sampler>,
        crate::__core::resource::SamplerDescriptor<'a>,
    ),
    DropSampler(PointerId<markers::Sampler>),
    GetSurfaceTexture {
        id: R::Texture,
        parent: R::Surface,
    },
    Present(R::Surface),
    DiscardSurfaceTexture(R::Surface),
    ReleaseSurfaceTexture(R::Surface),
    CreateBindGroupLayout(
        PointerId<markers::BindGroupLayout>,
        crate::__core::binding_model::BindGroupLayoutDescriptor<'a>,
    ),
    GetRenderPipelineBindGroupLayout {
        id: PointerId<markers::BindGroupLayout>,
        pipeline: PointerId<markers::RenderPipeline>,
        index: u32,
    },
    GetComputePipelineBindGroupLayout {
        id: PointerId<markers::BindGroupLayout>,
        pipeline: PointerId<markers::ComputePipeline>,
        index: u32,
    },
    DropBindGroupLayout(PointerId<markers::BindGroupLayout>),
    CreatePipelineLayout(
        PointerId<markers::PipelineLayout>,
        crate::__core::binding_model::ResolvedPipelineLayoutDescriptor<
            'a,
            PointerId<markers::BindGroupLayout>,
        >,
    ),
    DropPipelineLayout(PointerId<markers::PipelineLayout>),
    CreateBindGroup(PointerId<markers::BindGroup>, TraceBindGroupDescriptor<'a>),
    DropBindGroup(PointerId<markers::BindGroup>),
    CreateShaderModule {
        id: PointerId<markers::ShaderModule>,
        desc: crate::__core::pipeline::ShaderModuleDescriptor<'a>,
        data: Data,
    },
    CreateShaderModulePassthrough {
        id: PointerId<markers::ShaderModule>,
        data: Vec<Data>,

        label: crate::__core::Label<'a>,
        entry_points: Cow<'a, [crate::__types::PassthroughShaderEntryPoint<'a>]>,
    },
    DropShaderModule(PointerId<markers::ShaderModule>),
    CreateComputePipeline {
        id: PointerId<markers::ComputePipeline>,
        desc: TraceComputePipelineDescriptor<'a>,
    },
    DropComputePipeline(PointerId<markers::ComputePipeline>),
    CreateGeneralRenderPipeline {
        id: PointerId<markers::RenderPipeline>,
        desc: TraceGeneralRenderPipelineDescriptor<'a>,
    },
    DropRenderPipeline(PointerId<markers::RenderPipeline>),
    CreatePipelineCache {
        id: PointerId<markers::PipelineCache>,
        desc: crate::__core::pipeline::PipelineCacheDescriptor<'a>,
    },
    DropPipelineCache(PointerId<markers::PipelineCache>),
    CreateRenderBundle {
        id: R::RenderBundle,
        desc: crate::__core::command::RenderBundleEncoderDescriptor<'a>,
        base: BasePass<RenderCommand<R>, Infallible>,
    },
    DropRenderBundle(PointerId<markers::RenderBundle>),
    CreateQuerySet {
        id: PointerId<markers::QuerySet>,
        desc: crate::__core::resource::QuerySetDescriptor<'a>,
    },
    DropQuerySet(PointerId<markers::QuerySet>),
    DestroyQuerySet(PointerId<markers::QuerySet>),
    WriteBuffer {
        id: R::Buffer,
        data: Data,
        offset: crate::__types::BufferAddress,
        size: crate::__types::BufferAddress,
        queued: bool,
    },
    WriteTexture {
        to: crate::__types::TexelCopyTextureInfo<R::Texture>,
        data: Data,
        layout: crate::__types::TexelCopyBufferLayout,
        size: crate::__types::Extent3d,
    },
    Submit(crate::__core::SubmissionIndex, Vec<Command<R>>),
    FailedCommands {
        commands: Option<Vec<Command<R>>>,
        /// If `None`, then encoding failed due to a validation error (returned
        /// from `CommandEncoder::finish`). If `Some`, submission failed due to
        /// a resource having been destroyed.
        failed_at_submit: Option<crate::__core::SubmissionIndex>,
        error: String,
    },
    CreateBlas {
        id: R::Blas,
        desc: crate::__core::resource::BlasDescriptor<'a>,
        sizes: crate::__types::BlasGeometrySizeDescriptors,
    },
    DropBlas(R::Blas),
    CreateTlas {
        id: R::Tlas,
        desc: crate::__core::resource::TlasDescriptor<'a>,
    },
    DropTlas(R::Tlas),
}

/// cbindgen:ignore
pub type TraceBindGroupDescriptor<'a> = crate::__core::binding_model::BindGroupDescriptor<
    'a,
    PointerId<markers::BindGroupLayout>,
    PointerId<markers::Buffer>,
    PointerId<markers::Sampler>,
    PointerId<markers::TextureView>,
    PointerId<markers::Tlas>,
    PointerId<markers::ExternalTexture>,
>;

/// Not a public API. For use by `player` only.
///
/// cbindgen:ignore
#[doc(hidden)]
pub type TraceGeneralRenderPipelineDescriptor<'a> = GeneralRenderPipelineDescriptor<
    'a,
    PointerId<markers::PipelineLayout>,
    PointerId<markers::ShaderModule>,
    PointerId<markers::PipelineCache>,
>;

/// Not a public API. For use by `player` only.
///
/// cbindgen:ignore
#[doc(hidden)]
pub type TraceComputePipelineDescriptor<'a> = crate::__core::pipeline::ComputePipelineDescriptor<
    'a,
    PointerId<markers::PipelineLayout>,
    PointerId<markers::ShaderModule>,
    PointerId<markers::PipelineCache>,
>;
