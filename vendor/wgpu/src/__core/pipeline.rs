use alloc::{
    borrow::{Cow, ToOwned},
    boxed::Box,
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::{marker::PhantomData, mem::ManuallyDrop, num::NonZeroU32};

use crate::__naga::error::ShaderError;
use crate::__types::error::{ErrorType, WebGpuError};
use arrayvec::ArrayVec;
use thiserror::Error;

pub use crate::__core::pipeline_cache::PipelineCacheValidationError;
use crate::__core::{
    binding_model::{
        BindGroupLayout, CreateBindGroupLayoutError, CreatePipelineLayoutError,
        GetBindGroupLayoutError, PipelineLayout,
    },
    command::ColorAttachmentError,
    device::{
        AttachmentData, Device, DeviceError, MissingDownlevelFlags, MissingFeatures,
        RenderPassContext,
    },
    id::{PipelineCacheId, PipelineLayoutId, ShaderModuleId},
    resource::{InvalidResourceError, Labeled, ResourceState, TrackingData},
    resource_log,
    validation::{self, ShaderMetaData},
    Label,
};

/// Information about buffer bindings, which
/// is validated against the shader (and pipeline)
/// at draw time as opposed to initialization time.
#[derive(Debug, Default)]
pub(crate) struct LateSizedBufferGroup {
    // The order has to match `BindGroup::late_buffer_binding_sizes`.
    pub(crate) shader_sizes: Vec<crate::__types::BufferAddress>,
}

#[allow(clippy::large_enum_variant)]
pub enum ShaderModuleSource<'a> {
    #[cfg(jadix_wgpu_core_feature_wgsl)]
    Wgsl(Cow<'a, str>),
    #[cfg(jadix_wgpu_core_feature_glsl)]
    Glsl(Cow<'a, str>, crate::__naga::front::glsl::Options),
    #[cfg(jadix_wgpu_core_feature_spirv)]
    SpirV(Cow<'a, [u32]>, crate::__naga::front::spv::Options),
    Naga(Cow<'static, crate::__naga::Module>),
    /// Dummy variant because `Naga` doesn't have a lifetime and without enough active features it
    /// could be the last one active.
    #[doc(hidden)]
    Dummy(PhantomData<&'a ()>),
}

#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct ShaderModuleDescriptor<'a> {
    pub label: Label<'a>,
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub runtime_checks: crate::__types::ShaderRuntimeChecks,
}

pub type ShaderModuleDescriptorPassthrough<'a> =
    crate::__types::CreateShaderModuleDescriptorPassthrough<'a, Label<'a>>;

#[derive(Debug)]
pub struct ShaderModule {
    pub(crate) raw: ManuallyDrop<Box<dyn crate::__hal::DynShaderModule>>,
    pub(crate) device: Arc<Device>,
    pub(crate) interface: ShaderMetaData,
    /// The `label` from the descriptor used to create the resource.
    pub(crate) label: String,
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        resource_log!("Destroy raw {}", self.error_ident());
        // SAFETY: We are in the Drop impl and we don't use self.raw anymore after this point.
        let raw = unsafe { ManuallyDrop::take(&mut self.raw) };
        unsafe {
            self.device.raw().destroy_shader_module(raw);
        }
    }
}

crate::__core::impl_resource_type!(ShaderModule);
crate::__core::impl_labeled!(ShaderModule);
crate::__core::impl_parent_device!(ShaderModule);
crate::__core::impl_storage_item!(ShaderModule);

impl ShaderModule {
    pub(crate) fn raw(&self) -> &dyn crate::__hal::DynShaderModule {
        self.raw.as_ref()
    }

    pub(crate) fn finalize_entry_point_name(
        &self,
        stage: crate::__naga::ShaderStage,
        entry_point: Option<&str>,
    ) -> Result<String, validation::StageError> {
        match self.interface {
            ShaderMetaData::Interface(ref interface) => {
                interface.finalize_entry_point_name(stage, entry_point)
            }
            ShaderMetaData::Passthrough(ref interface) => {
                if let Some(ep) = entry_point {
                    if interface.entry_point_names.contains(ep) {
                        Ok(ep.to_owned())
                    } else {
                        Err(validation::StageError::MissingEntryPoint(ep.to_owned()))
                    }
                } else {
                    if interface.entry_point_names.len() != 1 {
                        return Err(validation::StageError::MultipleEntryPointsFound);
                    }
                    Ok(interface
                        .entry_point_names
                        .iter()
                        .next()
                        .unwrap()
                        .to_owned())
                }
            }
        }
    }
}

//Note: `Clone` would require `WithSpan: Clone`.
#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum CreateShaderModuleError {
    #[cfg(jadix_wgpu_core_feature_wgsl)]
    #[error(transparent)]
    Parsing(#[from] ShaderError<crate::__naga::front::wgsl::ParseError>),
    #[cfg(jadix_wgpu_core_feature_glsl)]
    #[error(transparent)]
    ParsingGlsl(#[from] ShaderError<crate::__naga::front::glsl::ParseErrors>),
    #[cfg(jadix_wgpu_core_feature_spirv)]
    #[error(transparent)]
    ParsingSpirV(#[from] ShaderError<crate::__naga::front::spv::Error>),
    #[error("Failed to generate the backend-specific code")]
    Generation,
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error(transparent)]
    Validation(#[from] ShaderError<crate::__naga::WithSpan<crate::__naga::valid::ValidationError>>),
    #[error(transparent)]
    MissingFeatures(#[from] MissingFeatures),
    #[error(
        "Shader global {bind:?} uses a group index {group} that exceeds the max_bind_groups limit of {limit}."
    )]
    InvalidGroupIndex {
        bind: crate::__naga::ResourceBinding,
        group: u32,
        limit: u32,
    },
    #[error("Generic shader passthrough does not contain any code compatible with this backend.")]
    NotCompiledForBackend,
    #[error(
        "Generic passthrough shaders which use GLSL or DXIL must contain exactly one entry point."
    )]
    IncorrectPassthroughEntryPointCount,
}

impl WebGpuError for CreateShaderModuleError {
    fn webgpu_error_type(&self) -> ErrorType {
        match self {
            Self::Device(e) => e.webgpu_error_type(),
            Self::MissingFeatures(e) => e.webgpu_error_type(),

            Self::Generation => ErrorType::Internal,

            Self::Validation(..)
            | Self::InvalidGroupIndex { .. }
            | Self::IncorrectPassthroughEntryPointCount
            | Self::NotCompiledForBackend => ErrorType::Validation,
            #[cfg(jadix_wgpu_core_feature_wgsl)]
            Self::Parsing(..) => ErrorType::Validation,
            #[cfg(jadix_wgpu_core_feature_glsl)]
            Self::ParsingGlsl(..) => ErrorType::Validation,
            #[cfg(jadix_wgpu_core_feature_spirv)]
            Self::ParsingSpirV(..) => ErrorType::Validation,
        }
    }
}

/// Describes a programmable pipeline stage.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct ProgrammableStageDescriptor<'a, SM = ShaderModuleId> {
    /// The compiled shader module for this stage.
    pub module: SM,
    /// The name of the entry point in the compiled shader. The name is selected using the
    /// following logic:
    ///
    /// * If `Some(name)` is specified, there must be a function with this name in the shader.
    /// * If a single entry point associated with this stage must be in the shader, then proceed as
    ///   if `Some(…)` was specified with that entry point's name.
    pub entry_point: Option<Cow<'a, str>>,
    /// Specifies the values of pipeline-overridable constants in the shader module.
    ///
    /// If an `@id` attribute was specified on the declaration,
    /// the key must be the pipeline constant ID as a decimal ASCII number; if not,
    /// the key must be the constant's identifier name.
    ///
    /// The value may represent any of WGSL's concrete scalar types.
    pub constants: crate::__naga::back::PipelineConstants,
    /// Whether workgroup scoped memory will be initialized with zero values for this stage.
    ///
    /// This is required by the WebGPU spec, but may have overhead which can be avoided
    /// for cross-platform applications
    pub zero_initialize_workgroup_memory: bool,
}

/// cbindgen:ignore
pub type ResolvedProgrammableStageDescriptor<'a> =
    ProgrammableStageDescriptor<'a, Arc<ShaderModule>>;

/// Number of implicit bind groups derived at pipeline creation.
pub type ImplicitBindGroupCount = u8;

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum ImplicitLayoutError {
    #[error("Unable to reflect the shader {0:?} interface")]
    ReflectionError(crate::__types::ShaderStages),
    #[error(transparent)]
    BindGroup(#[from] CreateBindGroupLayoutError),
    #[error(transparent)]
    Pipeline(#[from] CreatePipelineLayoutError),
    #[error("Unable to create implicit pipeline layout from passthrough shader stage: {0:?}")]
    Passthrough(crate::__types::ShaderStages),
}

impl WebGpuError for ImplicitLayoutError {
    fn webgpu_error_type(&self) -> ErrorType {
        match self {
            Self::ReflectionError(_) => ErrorType::Validation,
            Self::BindGroup(e) => e.webgpu_error_type(),
            Self::Pipeline(e) => e.webgpu_error_type(),
            Self::Passthrough(_) => ErrorType::Validation,
        }
    }
}

/// Describes a compute pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct ComputePipelineDescriptor<
    'a,
    PLL = PipelineLayoutId,
    SM = ShaderModuleId,
    PLC = PipelineCacheId,
> {
    pub label: Label<'a>,
    /// The layout of bind groups for this pipeline.
    pub layout: Option<PLL>,
    /// The compiled compute stage and its entry point.
    pub stage: ProgrammableStageDescriptor<'a, SM>,
    /// The pipeline cache to use when creating this pipeline.
    pub cache: Option<PLC>,
}

/// cbindgen:ignore
pub type ResolvedComputePipelineDescriptor<'a> =
    ComputePipelineDescriptor<'a, Arc<PipelineLayout>, Arc<ShaderModule>, Arc<PipelineCache>>;

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum CreateComputePipelineError {
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("Unable to derive an implicit layout")]
    Implicit(#[from] ImplicitLayoutError),
    #[error("Error matching shader requirements against the pipeline")]
    Stage(#[from] validation::StageError),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Pipeline constant error: {0}")]
    PipelineConstants(String),
    #[error(transparent)]
    MissingDownlevelFlags(#[from] MissingDownlevelFlags),
    #[error(transparent)]
    InvalidResource(#[from] InvalidResourceError),
}

impl WebGpuError for CreateComputePipelineError {
    fn webgpu_error_type(&self) -> ErrorType {
        match self {
            Self::Device(e) => e.webgpu_error_type(),
            Self::InvalidResource(e) => e.webgpu_error_type(),
            Self::MissingDownlevelFlags(e) => e.webgpu_error_type(),
            Self::Implicit(e) => e.webgpu_error_type(),
            Self::Stage(e) => e.webgpu_error_type(),
            Self::Internal(_) => ErrorType::Internal,
            Self::PipelineConstants(_) => ErrorType::Validation,
        }
    }
}

#[derive(Debug)]
pub struct ComputePipelineState {
    pub(crate) raw: ManuallyDrop<Box<dyn crate::__hal::DynComputePipeline>>,
    pub(crate) layout: Arc<PipelineLayout>,
    pub(crate) _shader_module: Arc<ShaderModule>,
}

#[derive(Debug)]
pub struct ComputePipeline {
    pub(crate) state: ResourceState<ComputePipelineState>,
    pub(crate) device: Arc<Device>,
    pub(crate) late_sized_buffer_groups:
        ArrayVec<LateSizedBufferGroup, { crate::__hal::MAX_BIND_GROUPS }>,
    pub(crate) immediate_slots_required: crate::__naga::valid::ImmediateSlots,
    /// The `label` from the descriptor used to create the resource.
    pub(crate) label: String,
    pub(crate) tracking_data: TrackingData,
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        resource_log!("Destroy raw {}", self.error_ident());
        #[cfg(jadix_wgpu_core_feature_trace)]
        {
            use crate::__core::device::trace;
            if let Some(t) = self.device.trace.lock().as_mut() {
                t.add(trace::Action::DropComputePipeline(unsafe {
                    trace::to_trace(self)
                }));
            }
        }
        let ResourceState::Valid(state) = &mut self.state else {
            return;
        };
        // SAFETY: We are in the Drop impl and we don't use self.raw anymore after this point.
        let raw = unsafe { ManuallyDrop::take(&mut state.raw) };
        unsafe {
            self.device.raw().destroy_compute_pipeline(raw);
        }
    }
}

crate::__core::impl_resource_type!(ComputePipeline);
crate::__core::impl_labeled!(ComputePipeline);
crate::__core::impl_parent_device!(ComputePipeline);
crate::__core::impl_storage_item!(ComputePipeline);
crate::__core::impl_trackable!(ComputePipeline);

impl ComputePipeline {
    pub(crate) fn raw(
        &self,
    ) -> Result<&dyn crate::__hal::DynComputePipeline, InvalidResourceError> {
        let ResourceState::Valid(state) = &self.state else {
            return Err(InvalidResourceError(self.error_ident()));
        };
        Ok(state.raw.as_ref())
    }

    pub(crate) fn layout(&self) -> Result<&Arc<PipelineLayout>, InvalidResourceError> {
        let ResourceState::Valid(state) = &self.state else {
            return Err(InvalidResourceError(self.error_ident()));
        };
        Ok(&state.layout)
    }

    pub(crate) fn check_valid(&self) -> Result<(), InvalidResourceError> {
        let ResourceState::Valid(_) = &self.state else {
            return Err(InvalidResourceError(self.error_ident()));
        };
        Ok(())
    }

    pub(crate) fn invalid(device: Arc<Device>, label: String) -> Arc<Self> {
        Arc::new(Self {
            tracking_data: TrackingData::new(device.tracker_indices.compute_pipelines.clone()),
            state: ResourceState::Invalid,
            device,
            late_sized_buffer_groups: ArrayVec::new(),
            immediate_slots_required: crate::__naga::valid::ImmediateSlots::default(),
            label,
        })
    }

    pub fn get_bind_group_layout_inner(
        self: &Arc<Self>,
        index: u32,
    ) -> Result<Arc<BindGroupLayout>, GetBindGroupLayoutError> {
        self.layout()?.get_bind_group_layout(index, self.into())
    }

    pub fn get_bind_group_layout(
        self: &Arc<Self>,
        index: u32,
    ) -> (Arc<BindGroupLayout>, Option<GetBindGroupLayoutError>) {
        let (bgl, error) = match self.get_bind_group_layout_inner(index) {
            Ok(bgl) => (bgl, None),
            Err(e) => (
                BindGroupLayout::invalid(&self.device, String::new()),
                Some(e),
            ),
        };
        #[cfg(jadix_wgpu_core_feature_trace)]
        if let Some(ref mut trace) = *self.device.trace.lock() {
            use crate::__core::device::trace;
            use trace::IntoTrace;
            trace.add(trace::Action::GetComputePipelineBindGroupLayout {
                id: bgl.to_trace(),
                pipeline: self.to_trace(),
                index,
            });
        };
        (bgl, error)
    }
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum CreatePipelineCacheError {
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("Pipeline cache validation failed")]
    Validation(#[from] PipelineCacheValidationError),
    #[error(transparent)]
    MissingFeatures(#[from] MissingFeatures),
}

impl WebGpuError for CreatePipelineCacheError {
    fn webgpu_error_type(&self) -> ErrorType {
        match self {
            Self::Device(e) => e.webgpu_error_type(),
            Self::Validation(e) => e.webgpu_error_type(),
            Self::MissingFeatures(e) => e.webgpu_error_type(),
        }
    }
}

#[derive(Debug)]
pub struct PipelineCache {
    pub(crate) raw: ManuallyDrop<Box<dyn crate::__hal::DynPipelineCache>>,
    pub(crate) device: Arc<Device>,
    /// The `label` from the descriptor used to create the resource.
    pub(crate) label: String,
}

impl Drop for PipelineCache {
    fn drop(&mut self) {
        resource_log!("Destroy raw {}", self.error_ident());
        // SAFETY: We are in the Drop impl and we don't use self.raw anymore after this point.
        let raw = unsafe { ManuallyDrop::take(&mut self.raw) };
        unsafe {
            self.device.raw().destroy_pipeline_cache(raw);
        }
    }
}

crate::__core::impl_resource_type!(PipelineCache);
crate::__core::impl_labeled!(PipelineCache);
crate::__core::impl_parent_device!(PipelineCache);
crate::__core::impl_storage_item!(PipelineCache);

impl PipelineCache {
    pub(crate) fn raw(&self) -> &dyn crate::__hal::DynPipelineCache {
        self.raw.as_ref()
    }
}

/// Describes how the vertex buffer is interpreted.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(jadix_wgpu_core_feature_serde, serde(rename_all = "camelCase"))]
pub struct VertexBufferLayout<'a> {
    /// The stride, in bytes, between elements of this buffer.
    pub array_stride: crate::__types::BufferAddress,
    /// How often this vertex buffer is "stepped" forward.
    pub step_mode: crate::__types::VertexStepMode,
    /// The list of attributes which comprise a single vertex.
    pub attributes: Cow<'a, [crate::__types::VertexAttribute]>,
}

/// A null vertex buffer layout that may be placed in unused slots.
impl Default for VertexBufferLayout<'_> {
    fn default() -> Self {
        Self {
            array_stride: Default::default(),
            step_mode: Default::default(),
            attributes: Cow::Borrowed(&[]),
        }
    }
}

/// Describes the vertex process in a render pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct VertexState<'a, SM = ShaderModuleId> {
    /// The compiled vertex stage and its entry point.
    pub stage: ProgrammableStageDescriptor<'a, SM>,
    /// The format of any vertex buffers used with this pipeline.
    pub buffers: Cow<'a, [Option<VertexBufferLayout<'a>>]>,
}

/// cbindgen:ignore
pub type ResolvedVertexState<'a> = VertexState<'a, Arc<ShaderModule>>;

/// Describes fragment processing in a render pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct FragmentState<'a, SM = ShaderModuleId> {
    /// The compiled fragment stage and its entry point.
    pub stage: ProgrammableStageDescriptor<'a, SM>,
    /// The effect of draw calls on the color aspect of the output target.
    pub targets: Cow<'a, [Option<crate::__types::ColorTargetState>]>,
}

/// cbindgen:ignore
pub type ResolvedFragmentState<'a> = FragmentState<'a, Arc<ShaderModule>>;

/// Describes the task shader in a mesh shader pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct TaskState<'a, SM = ShaderModuleId> {
    /// The compiled task stage and its entry point.
    pub stage: ProgrammableStageDescriptor<'a, SM>,
}

pub type ResolvedTaskState<'a> = TaskState<'a, Arc<ShaderModule>>;

/// Describes the mesh shader in a mesh shader pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct MeshState<'a, SM = ShaderModuleId> {
    /// The compiled mesh stage and its entry point.
    pub stage: ProgrammableStageDescriptor<'a, SM>,
}

pub type ResolvedMeshState<'a> = MeshState<'a, Arc<ShaderModule>>;

/// Describes a vertex processor for either a conventional or mesh shading
/// pipeline architecture.
///
/// This is not a public API. It is for use by `player` only. The public APIs
/// are [`VertexState`], [`TaskState`], and [`MeshState`].
///
/// cbindgen:ignore
#[doc(hidden)]
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum RenderPipelineVertexProcessor<'a, SM = ShaderModuleId> {
    Vertex(VertexState<'a, SM>),
    Mesh(Option<TaskState<'a, SM>>, MeshState<'a, SM>),
}

/// Describes a render (graphics) pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct RenderPipelineDescriptor<
    'a,
    PLL = PipelineLayoutId,
    SM = ShaderModuleId,
    PLC = PipelineCacheId,
> {
    pub label: Label<'a>,
    /// The layout of bind groups for this pipeline.
    pub layout: Option<PLL>,
    /// The vertex processing state for this pipeline.
    pub vertex: VertexState<'a, SM>,
    /// The properties of the pipeline at the primitive assembly and rasterization level.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub primitive: crate::__types::PrimitiveState,
    /// The effect of draw calls on the depth and stencil aspects of the output target, if any.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub depth_stencil: Option<crate::__types::DepthStencilState>,
    /// The multi-sampling properties of the pipeline.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub multisample: crate::__types::MultisampleState,
    /// The fragment processing state for this pipeline.
    pub fragment: Option<FragmentState<'a, SM>>,
    /// If the pipeline will be used with a multiview render pass, this indicates how many array
    /// layers the attachments will have.
    pub multiview_mask: Option<NonZeroU32>,
    /// The pipeline cache to use when creating this pipeline.
    pub cache: Option<PLC>,
}
/// Describes a mesh shader pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct MeshPipelineDescriptor<
    'a,
    PLL = PipelineLayoutId,
    SM = ShaderModuleId,
    PLC = PipelineCacheId,
> {
    pub label: Label<'a>,
    /// The layout of bind groups for this pipeline.
    pub layout: Option<PLL>,
    /// The task processing state for this pipeline.
    pub task: Option<TaskState<'a, SM>>,
    /// The mesh processing state for this pipeline
    pub mesh: MeshState<'a, SM>,
    /// The properties of the pipeline at the primitive assembly and rasterization level.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub primitive: crate::__types::PrimitiveState,
    /// The effect of draw calls on the depth and stencil aspects of the output target, if any.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub depth_stencil: Option<crate::__types::DepthStencilState>,
    /// The multi-sampling properties of the pipeline.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub multisample: crate::__types::MultisampleState,
    /// The fragment processing state for this pipeline.
    pub fragment: Option<FragmentState<'a, SM>>,
    /// If the pipeline will be used with a multiview render pass, this indicates how many array
    /// layers the attachments will have.
    pub multiview: Option<NonZeroU32>,
    /// The pipeline cache to use when creating this pipeline.
    pub cache: Option<PLC>,
}

/// Describes a render (graphics) pipeline, with either conventional or mesh
/// shading architecture.
///
/// This is not a public API. It is for use by `player` only. The public APIs
/// are [`RenderPipelineDescriptor`] and [`MeshPipelineDescriptor`].
///
/// cbindgen:ignore
#[doc(hidden)]
#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct GeneralRenderPipelineDescriptor<
    'a,
    PLL = PipelineLayoutId,
    SM = ShaderModuleId,
    PLC = PipelineCacheId,
> {
    pub label: Label<'a>,
    /// The layout of bind groups for this pipeline.
    pub layout: Option<PLL>,
    /// The vertex processing state for this pipeline.
    pub vertex: RenderPipelineVertexProcessor<'a, SM>,
    /// The properties of the pipeline at the primitive assembly and rasterization level.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub primitive: crate::__types::PrimitiveState,
    /// The effect of draw calls on the depth and stencil aspects of the output target, if any.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub depth_stencil: Option<crate::__types::DepthStencilState>,
    /// The multi-sampling properties of the pipeline.
    #[cfg_attr(jadix_wgpu_core_feature_serde, serde(default))]
    pub multisample: crate::__types::MultisampleState,
    /// The fragment processing state for this pipeline.
    pub fragment: Option<FragmentState<'a, SM>>,
    /// If the pipeline will be used with a multiview render pass, this indicates how many array
    /// layers the attachments will have.
    pub multiview_mask: Option<NonZeroU32>,
    /// The pipeline cache to use when creating this pipeline.
    pub cache: Option<PLC>,
}
impl<'a, PLL, SM, PLC> From<RenderPipelineDescriptor<'a, PLL, SM, PLC>>
    for GeneralRenderPipelineDescriptor<'a, PLL, SM, PLC>
{
    fn from(value: RenderPipelineDescriptor<'a, PLL, SM, PLC>) -> Self {
        Self {
            label: value.label,
            layout: value.layout,
            vertex: RenderPipelineVertexProcessor::Vertex(value.vertex),
            primitive: value.primitive,
            depth_stencil: value.depth_stencil,
            multisample: value.multisample,
            fragment: value.fragment,
            multiview_mask: value.multiview_mask,
            cache: value.cache,
        }
    }
}
impl<'a, PLL, SM, PLC> From<MeshPipelineDescriptor<'a, PLL, SM, PLC>>
    for GeneralRenderPipelineDescriptor<'a, PLL, SM, PLC>
{
    fn from(value: MeshPipelineDescriptor<'a, PLL, SM, PLC>) -> Self {
        Self {
            label: value.label,
            layout: value.layout,
            vertex: RenderPipelineVertexProcessor::Mesh(value.task, value.mesh),
            primitive: value.primitive,
            depth_stencil: value.depth_stencil,
            multisample: value.multisample,
            fragment: value.fragment,
            multiview_mask: value.multiview,
            cache: value.cache,
        }
    }
}

/// Not a public API. For use by `player` only.
///
/// cbindgen:ignore
pub type ResolvedGeneralRenderPipelineDescriptor<'a> =
    GeneralRenderPipelineDescriptor<'a, Arc<PipelineLayout>, Arc<ShaderModule>, Arc<PipelineCache>>;

#[derive(Clone, Debug)]
#[cfg_attr(
    jadix_wgpu_core_feature_serde,
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct PipelineCacheDescriptor<'a> {
    pub label: Label<'a>,
    pub data: Option<Cow<'a, [u8]>>,
    pub fallback: bool,
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum ColorStateError {
    #[error("Format {0:?} is not renderable")]
    FormatNotRenderable(crate::__types::TextureFormat),
    #[error("Format {0:?} is not blendable")]
    FormatNotBlendable(crate::__types::TextureFormat),
    #[error("Format {0:?} does not have a color aspect")]
    FormatNotColor(crate::__types::TextureFormat),
    #[error("Sample count {0} is not supported by format {1:?} on this device. The WebGPU spec guarantees {2:?} samples are supported by this format. With the TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES feature your device supports {3:?}.")]
    InvalidSampleCount(u32, crate::__types::TextureFormat, Vec<u32>, Vec<u32>),
    #[error("Output format {pipeline} is incompatible with the shader {shader}")]
    IncompatibleFormat {
        pipeline: validation::NumericType,
        shader: validation::NumericType,
    },
    #[error("Invalid write mask {0:?}")]
    InvalidWriteMask(crate::__types::ColorWrites),
    #[error("Using the blend factor {factor:?} for render target {target} is not possible. Only the first render target may be used when dual-source blending.")]
    BlendFactorOnUnsupportedTarget {
        factor: crate::__types::BlendFactor,
        target: u32,
    },
    #[error(
        "Blend factor {factor:?} for render target {target} is not valid. Blend factor must be `one` when using min/max blend operations."
    )]
    InvalidMinMaxBlendFactor {
        factor: crate::__types::BlendFactor,
        target: u32,
    },
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum DepthStencilStateError {
    #[error("Format {0:?} is not renderable")]
    FormatNotRenderable(crate::__types::TextureFormat),
    #[error("Format {0:?} is not a depth/stencil format")]
    FormatNotDepthOrStencil(crate::__types::TextureFormat),
    #[error("Format {0:?} does not have a depth aspect, but depth test/write is enabled")]
    FormatNotDepth(crate::__types::TextureFormat),
    #[error("Format {0:?} does not have a stencil aspect, but stencil test/write is enabled")]
    FormatNotStencil(crate::__types::TextureFormat),
    #[error("Sample count {0} is not supported by format {1:?} on this device. The WebGPU spec guarantees {2:?} samples are supported by this format. With the TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES feature your device supports {3:?}.")]
    InvalidSampleCount(u32, crate::__types::TextureFormat, Vec<u32>, Vec<u32>),
    #[error("Depth bias is not compatible with non-triangle topology {0:?}")]
    DepthBiasWithIncompatibleTopology(crate::__types::PrimitiveTopology),
    #[error("Depth compare function must be specified for depth format {0:?}")]
    MissingDepthCompare(crate::__types::TextureFormat),
    #[error("Depth write enabled must be specified for depth format {0:?}")]
    MissingDepthWriteEnabled(crate::__types::TextureFormat),
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum CreateRenderPipelineError {
    #[error(transparent)]
    ColorAttachment(#[from] ColorAttachmentError),
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("Unable to derive an implicit layout")]
    Implicit(#[from] ImplicitLayoutError),
    #[error("Color state [{0}] is invalid")]
    ColorState(u8, #[source] ColorStateError),
    #[error("Depth/stencil state is invalid")]
    DepthStencilState(#[from] DepthStencilStateError),
    #[error("Invalid sample count {0}")]
    InvalidSampleCount(u32),
    #[error("The number of vertex buffers {given} exceeds the limit {limit}")]
    TooManyVertexBuffers { given: u32, limit: u32 },
    #[error("The number of bind groups + vertex buffers {given} exceeds the limit {limit}")]
    TooManyBindGroupsPlusVertexBuffers { given: u32, limit: u32 },
    #[error("The number of vertex-stage buffers and acceleration structures {given} exceeds the limit {limit}")]
    TooManyBuffersAndAccelerationStructuresInVertexStage { given: u32, limit: u32 },
    #[error("The total number of vertex attributes {given} exceeds the limit {limit}")]
    TooManyVertexAttributes { given: u32, limit: u32 },
    #[error("Vertex attribute location {given} must be less than limit {limit}")]
    VertexAttributeLocationTooLarge { given: u32, limit: u32 },
    #[error("Vertex buffer {index} stride {given} exceeds the limit {limit}")]
    VertexStrideTooLarge { index: u32, given: u32, limit: u32 },
    #[error("Vertex attribute at location {location} stride {given} exceeds the limit {limit}")]
    VertexAttributeStrideTooLarge {
        location: crate::__types::ShaderLocation,
        given: u32,
        limit: u32,
    },
    #[error("Vertex buffer {index} stride {stride} does not respect `VERTEX_ALIGNMENT`")]
    UnalignedVertexStride {
        index: u32,
        stride: crate::__types::BufferAddress,
    },
    #[error("Vertex attribute at location {location} has invalid offset {offset}")]
    InvalidVertexAttributeOffset {
        location: crate::__types::ShaderLocation,
        offset: crate::__types::BufferAddress,
    },
    #[error("Two or more vertex attributes were assigned to the same location in the shader: {0}")]
    ShaderLocationClash(u32),
    #[error("Strip index format was not set to None but to {strip_index_format:?} while using the non-strip topology {topology:?}")]
    StripIndexFormatForNonStripTopology {
        strip_index_format: Option<crate::__types::IndexFormat>,
        topology: crate::__types::PrimitiveTopology,
    },
    #[error("Conservative Rasterization is only supported for wgt::PolygonMode::Fill")]
    ConservativeRasterizationNonFillPolygonMode,
    #[error(transparent)]
    MissingFeatures(#[from] MissingFeatures),
    #[error(transparent)]
    MissingDownlevelFlags(#[from] MissingDownlevelFlags),
    #[error("Error matching {stage:?} shader requirements against the pipeline")]
    Stage {
        stage: crate::__types::ShaderStages,
        #[source]
        error: validation::StageError,
    },
    #[error("Internal error in {stage:?} shader: {error}")]
    Internal {
        stage: crate::__types::ShaderStages,
        error: String,
    },
    #[error("Pipeline constant error in {stage:?} shader: {error}")]
    PipelineConstants {
        stage: crate::__types::ShaderStages,
        error: String,
    },
    #[error("In the provided shader, the type given for group {group} binding {binding} has a size of {size}. As the device does not support `DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED`, the type must have a size that is a multiple of 16 bytes.")]
    UnalignedShader { group: u32, binding: u32, size: u64 },
    #[error("Dual-source blending requires exactly one color target, but {count} color targets are present")]
    DualSourceBlendingWithMultipleColorTargets { count: usize },
    #[error("{}", concat!(
        "At least one color attachment or depth-stencil attachment was expected, ",
        "but no render target for the pipeline was specified."
    ))]
    NoTargetSpecified,
    #[error(transparent)]
    InvalidResource(#[from] InvalidResourceError),
}

impl WebGpuError for CreateRenderPipelineError {
    fn webgpu_error_type(&self) -> ErrorType {
        match self {
            Self::Device(e) => e.webgpu_error_type(),
            Self::InvalidResource(e) => e.webgpu_error_type(),
            Self::MissingFeatures(e) => e.webgpu_error_type(),
            Self::MissingDownlevelFlags(e) => e.webgpu_error_type(),

            Self::Internal { .. } => ErrorType::Internal,

            Self::ColorAttachment(_)
            | Self::Implicit(_)
            | Self::ColorState(_, _)
            | Self::DepthStencilState(_)
            | Self::InvalidSampleCount(_)
            | Self::TooManyVertexBuffers { .. }
            | Self::TooManyBindGroupsPlusVertexBuffers { .. }
            | Self::TooManyBuffersAndAccelerationStructuresInVertexStage { .. }
            | Self::TooManyVertexAttributes { .. }
            | Self::VertexAttributeLocationTooLarge { .. }
            | Self::VertexStrideTooLarge { .. }
            | Self::UnalignedVertexStride { .. }
            | Self::InvalidVertexAttributeOffset { .. }
            | Self::ShaderLocationClash(_)
            | Self::StripIndexFormatForNonStripTopology { .. }
            | Self::ConservativeRasterizationNonFillPolygonMode
            | Self::Stage { .. }
            | Self::UnalignedShader { .. }
            | Self::DualSourceBlendingWithMultipleColorTargets { .. }
            | Self::NoTargetSpecified
            | Self::PipelineConstants { .. }
            | Self::VertexAttributeStrideTooLarge { .. } => ErrorType::Validation,
        }
    }
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct PipelineFlags: u32 {
        const BLEND_CONSTANT = 1 << 0;
        const STENCIL_REFERENCE = 1 << 1;
        const WRITES_DEPTH = 1 << 2;
        const WRITES_STENCIL = 1 << 3;
    }
}

/// How a render pipeline will retrieve attributes from a particular vertex buffer.
#[derive(Clone, Copy, Debug)]
pub struct VertexStep {
    /// The byte stride in the buffer between one attribute value and the next.
    pub stride: crate::__types::BufferAddress,

    /// The byte size required to fit the last vertex in the stream.
    pub last_stride: crate::__types::BufferAddress,

    /// Whether the buffer is indexed by vertex number or instance number.
    pub mode: crate::__types::VertexStepMode,
}

impl Default for VertexStep {
    fn default() -> Self {
        Self {
            stride: 0,
            last_stride: 0,
            mode: crate::__types::VertexStepMode::Vertex,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RenderPipelineState {
    pub(crate) raw: ManuallyDrop<Box<dyn crate::__hal::DynRenderPipeline>>,
    pub(crate) layout: Arc<PipelineLayout>,
}

#[derive(Debug)]
pub struct RenderPipeline {
    pub(crate) state: ResourceState<RenderPipelineState>,
    pub(crate) device: Arc<Device>,
    pub(crate) _shader_modules:
        ArrayVec<Arc<ShaderModule>, { crate::__hal::MAX_CONCURRENT_SHADER_STAGES }>,
    pub(crate) pass_context: RenderPassContext,
    pub(crate) flags: PipelineFlags,
    pub(crate) topology: crate::__types::PrimitiveTopology,
    pub(crate) strip_index_format: Option<crate::__types::IndexFormat>,
    pub(crate) vertex_steps: Vec<Option<VertexStep>>,
    pub(crate) late_sized_buffer_groups:
        ArrayVec<LateSizedBufferGroup, { crate::__hal::MAX_BIND_GROUPS }>,
    pub(crate) immediate_slots_required: crate::__naga::valid::ImmediateSlots,
    /// The `label` from the descriptor used to create the resource.
    pub(crate) label: String,
    pub(crate) tracking_data: TrackingData,
    /// Whether this is a mesh shader pipeline
    pub(crate) is_mesh: bool,
    pub(crate) has_task_shader: bool,
}

impl Drop for RenderPipeline {
    fn drop(&mut self) {
        resource_log!("Destroy raw {}", self.error_ident());
        #[cfg(jadix_wgpu_core_feature_trace)]
        {
            use crate::__core::device::trace;
            if let Some(t) = self.device.trace.lock().as_mut() {
                t.add(trace::Action::DropRenderPipeline(unsafe {
                    trace::to_trace(self)
                }));
            }
        }
        let ResourceState::Valid(state) = &mut self.state else {
            return;
        };
        // SAFETY: We are in the Drop impl and we don't use self.raw anymore after this point.
        let raw = unsafe { ManuallyDrop::take(&mut state.raw) };
        unsafe {
            self.device.raw().destroy_render_pipeline(raw);
        }
    }
}

crate::__core::impl_resource_type!(RenderPipeline);
crate::__core::impl_labeled!(RenderPipeline);
crate::__core::impl_parent_device!(RenderPipeline);
crate::__core::impl_storage_item!(RenderPipeline);
crate::__core::impl_trackable!(RenderPipeline);

impl RenderPipeline {
    pub(crate) fn raw(&self) -> Result<&dyn crate::__hal::DynRenderPipeline, InvalidResourceError> {
        let ResourceState::Valid(state) = &self.state else {
            return Err(InvalidResourceError(self.error_ident()));
        };
        Ok(state.raw.as_ref())
    }

    pub(crate) fn layout(&self) -> Result<&Arc<PipelineLayout>, InvalidResourceError> {
        let ResourceState::Valid(state) = &self.state else {
            return Err(InvalidResourceError(self.error_ident()));
        };
        Ok(&state.layout)
    }

    pub(crate) fn check_valid(&self) -> Result<(), InvalidResourceError> {
        let ResourceState::Valid(_) = &self.state else {
            return Err(InvalidResourceError(self.error_ident()));
        };
        Ok(())
    }

    pub(crate) fn invalid(device: Arc<Device>, label: String) -> Arc<Self> {
        Arc::new(Self {
            tracking_data: TrackingData::new(device.tracker_indices.render_pipelines.clone()),
            state: ResourceState::Invalid,
            device,
            _shader_modules: ArrayVec::new(),
            pass_context: RenderPassContext {
                attachments: AttachmentData {
                    colors: ArrayVec::new(),
                    resolves: ArrayVec::new(),
                    depth_stencil: None,
                },
                sample_count: 0,
                multiview_mask: None,
            },
            flags: PipelineFlags::empty(),
            topology: crate::__types::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            vertex_steps: Vec::new(),
            late_sized_buffer_groups: ArrayVec::new(),
            immediate_slots_required: crate::__naga::valid::ImmediateSlots::default(),
            label,
            is_mesh: false,
            has_task_shader: false,
        })
    }

    pub fn get_bind_group_layout_inner(
        self: &Arc<Self>,
        index: u32,
    ) -> Result<Arc<BindGroupLayout>, GetBindGroupLayoutError> {
        self.layout()?.get_bind_group_layout(index, self.into())
    }

    pub fn get_bind_group_layout(
        self: &Arc<Self>,
        index: u32,
    ) -> (Arc<BindGroupLayout>, Option<GetBindGroupLayoutError>) {
        let (bgl, error) = match self.get_bind_group_layout_inner(index) {
            Ok(bgl) => (bgl, None),
            Err(e) => (
                BindGroupLayout::invalid(&self.device, String::new()),
                Some(e),
            ),
        };
        #[cfg(jadix_wgpu_core_feature_trace)]
        if let Some(ref mut trace) = *self.device.trace.lock() {
            use crate::__core::device::trace;
            use trace::IntoTrace;
            trace.add(trace::Action::GetRenderPipelineBindGroupLayout {
                id: bgl.to_trace(),
                pipeline: self.to_trace(),
                index,
            });
        };
        (bgl, error)
    }
}
