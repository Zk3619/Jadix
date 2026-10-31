#![allow(unused_variables)]

use alloc::{string::String, sync::Arc, vec, vec::Vec};
use core::{ptr, sync::atomic::Ordering, time::Duration};

#[cfg(jadix_wgpu_hal_supports_64bit_atomics)]
use core::sync::atomic::AtomicU64;
#[cfg(not(jadix_wgpu_hal_supports_64bit_atomics))]
use portable_atomic::AtomicU64;

use crate::__hal::TlasInstance;

mod buffer;
pub use buffer::Buffer;
mod command;
pub use command::CommandBuffer;

#[derive(Clone, Debug)]
pub struct Api;

#[derive(Debug)]
pub struct Context {
    options: Arc<crate::__types::NoopBackendOptions>,
}
#[derive(Debug)]
pub struct Encoder;
#[derive(Debug)]
pub struct Resource;

#[derive(Debug)]
pub struct Fence {
    value: AtomicU64,
}

type DeviceResult<T> = Result<T, crate::__hal::DeviceError>;

impl crate::__hal::Api for Api {
    const VARIANT: crate::__types::Backend = crate::__types::Backend::Noop;

    type Instance = Context;
    type Surface = Context;
    type Adapter = Context;
    type Device = Context;

    type Queue = Context;
    type CommandEncoder = CommandBuffer;
    type CommandBuffer = CommandBuffer;

    type Buffer = Buffer;
    type Texture = Resource;
    type SurfaceTexture = Resource;
    type TextureView = Resource;
    type Sampler = Resource;
    type QuerySet = Resource;
    type Fence = Fence;
    type AccelerationStructure = Resource;
    type PipelineCache = Resource;

    type BindGroupLayout = Resource;
    type BindGroup = Resource;
    type PipelineLayout = Resource;
    type ShaderModule = Resource;
    type RenderPipeline = Resource;
    type RayTracingPipeline = Resource;
    type ComputePipeline = Resource;
}

crate::__hal::impl_dyn_resource!(Buffer, CommandBuffer, Context, Fence, Resource);

impl crate::__hal::DynAccelerationStructure for Resource {}
impl crate::__hal::DynBindGroup for Resource {}
impl crate::__hal::DynBindGroupLayout for Resource {}
impl crate::__hal::DynBuffer for Buffer {}
impl crate::__hal::DynCommandBuffer for CommandBuffer {}
impl crate::__hal::DynComputePipeline for Resource {}
impl crate::__hal::DynFence for Fence {}
impl crate::__hal::DynPipelineCache for Resource {}
impl crate::__hal::DynPipelineLayout for Resource {}
impl crate::__hal::DynQuerySet for Resource {}
impl crate::__hal::DynRenderPipeline for Resource {}
impl crate::__hal::DynRayTracingPipeline for Resource {}
impl crate::__hal::DynSampler for Resource {}
impl crate::__hal::DynShaderModule for Resource {}
impl crate::__hal::DynSurfaceTexture for Resource {}
impl crate::__hal::DynTexture for Resource {}
impl crate::__hal::DynTextureView for Resource {}

impl core::borrow::Borrow<dyn crate::__hal::DynTexture> for Resource {
    fn borrow(&self) -> &dyn crate::__hal::DynTexture {
        self
    }
}

impl crate::__hal::Instance for Context {
    type A = Api;

    unsafe fn init(
        desc: &crate::__hal::InstanceDescriptor<'_>,
    ) -> Result<Self, crate::__hal::InstanceError> {
        let options = Arc::new(desc.backend_options.noop.clone());
        if options.enable {
            Ok(Context { options })
        } else {
            Err(crate::__hal::InstanceError::new(String::from(
                "noop backend disabled because NoopBackendOptions::enable is false",
            )))
        }
    }
    unsafe fn create_surface(
        &self,
        _display_handle: raw_window_handle::RawDisplayHandle,
        _window_handle: raw_window_handle::RawWindowHandle,
    ) -> Result<Context, crate::__hal::InstanceError> {
        Ok(Context {
            options: Arc::clone(&self.options),
        })
    }
    unsafe fn enumerate_adapters(
        &self,
        _surface_hint: Option<&Context>,
    ) -> Vec<crate::__hal::ExposedAdapter<Api>> {
        let device_type = self
            .options
            .device_type
            .unwrap_or(crate::__types::DeviceType::Other);
        let subgroup_min_size = self
            .options
            .subgroup_min_size
            .unwrap_or(crate::__types::MINIMUM_SUBGROUP_MIN_SIZE);
        let subgroup_max_size = self
            .options
            .subgroup_max_size
            .unwrap_or(crate::__types::MAXIMUM_SUBGROUP_MAX_SIZE);
        let features = self
            .options
            .features
            .unwrap_or(crate::__types::Features::all());
        let limits = self
            .options
            .limits
            .clone()
            .unwrap_or(CAPABILITIES.limits.clone());

        let info = crate::__types::AdapterInfo {
            subgroup_min_size,
            subgroup_max_size,
            ..adapter_info()
        };

        let capabilities = crate::__hal::Capabilities {
            limits,
            ..CAPABILITIES
        };

        vec![crate::__hal::ExposedAdapter {
            adapter: Context {
                options: Arc::clone(&self.options),
            },
            info,
            features,
            capabilities,
        }]
    }
}

/// Returns the adapter info for the noop backend.
///
/// This is used in the test harness to construct info about
/// the noop backend adapter without actually initializing wgpu.
pub fn adapter_info() -> crate::__types::AdapterInfo {
    crate::__types::AdapterInfo {
        name: String::from("noop wgpu backend"),
        driver: String::from("wgpu"),
        ..crate::__types::AdapterInfo::new(
            crate::__types::DeviceType::Cpu,
            crate::__types::Backend::Noop,
        )
    }
}

/// The capabilities of the noop backend.
///
/// This is used in the test harness to construct capabilities
/// of the noop backend without actually initializing wgpu.
pub const CAPABILITIES: crate::__hal::Capabilities = {
    crate::__hal::Capabilities {
        limits: crate::__types::Limits::unlimited(),
        alignments: crate::__hal::Alignments {
            // All maximally permissive
            buffer_copy_offset: crate::__types::BufferSize::MIN,
            buffer_copy_pitch: crate::__types::BufferSize::MIN,
            uniform_bounds_check_alignment: crate::__types::BufferSize::MIN,
            raw_tlas_instance_size: 0,
            ray_tracing_scratch_buffer_alignment: 1,
            ray_tracing_pipeline_group_data_size: 1,
            ray_tracing_pipeline_group_data_alignment: 1,
            ray_tracing_pipeline_data_offset_alignment: 1,
        },
        downlevel: crate::__types::DownlevelCapabilities {
            flags: crate::__types::DownlevelFlags::all(),
            limits: crate::__types::DownlevelLimits {},
            shader_model: crate::__types::ShaderModel::Sm5,
        },
        cooperative_matrix_properties: Vec::new(),
    }
};

impl crate::__hal::Surface for Context {
    type A = Api;

    unsafe fn configure(
        &self,
        device: &Context,
        config: &crate::__hal::SurfaceConfiguration,
    ) -> Result<(), crate::__hal::SurfaceError> {
        Ok(())
    }

    unsafe fn unconfigure(&self, device: &Context) {}

    unsafe fn acquire_texture(
        &self,
        _timeout: Option<Duration>,
        _fence: &Fence,
    ) -> Result<crate::__hal::AcquiredSurfaceTexture<Api>, crate::__hal::SurfaceError> {
        Err(crate::__hal::SurfaceError::Timeout)
    }
    unsafe fn discard_texture(&self, texture: Resource) {}
}

impl crate::__hal::Adapter for Context {
    type A = Api;

    unsafe fn open(
        &self,
        features: crate::__types::Features,
        _limits: &crate::__types::Limits,
        _memory_hints: &crate::__types::MemoryHints,
    ) -> DeviceResult<crate::__hal::OpenDevice<Api>> {
        Ok(crate::__hal::OpenDevice {
            device: Context {
                options: Arc::clone(&self.options),
            },
            queue: Context {
                options: Arc::clone(&self.options),
            },
        })
    }
    unsafe fn texture_format_capabilities(
        &self,
        format: crate::__types::TextureFormat,
    ) -> crate::__hal::TextureFormatCapabilities {
        crate::__hal::TextureFormatCapabilities::empty()
    }

    unsafe fn surface_capabilities(
        &self,
        surface: &Context,
    ) -> Option<crate::__hal::SurfaceCapabilities> {
        None
    }

    unsafe fn get_presentation_timestamp(&self) -> crate::__types::PresentationTimestamp {
        crate::__types::PresentationTimestamp::INVALID_TIMESTAMP
    }

    fn get_ordered_buffer_usages(&self) -> crate::__types::BufferUses {
        crate::__types::BufferUses::INCLUSIVE | crate::__types::BufferUses::MAP_WRITE
    }

    fn get_ordered_texture_usages(&self) -> crate::__types::TextureUses {
        crate::__types::TextureUses::INCLUSIVE
            | crate::__types::TextureUses::COLOR_TARGET
            | crate::__types::TextureUses::DEPTH_STENCIL_WRITE
    }
}

impl crate::__hal::Queue for Context {
    type A = Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&CommandBuffer],
        surface_textures: &[&Resource],
        (fence, fence_value): (&Fence, crate::__hal::FenceValue),
    ) -> DeviceResult<()> {
        // All commands are executed synchronously.
        for cb in command_buffers {
            // SAFETY: Caller is responsible for ensuring synchronization between commands and
            // other mutations.
            unsafe {
                cb.execute();
            }
        }
        fence.value.store(fence_value, Ordering::Release);
        Ok(())
    }
    unsafe fn present(
        &self,
        surface: &Context,
        texture: Resource,
    ) -> Result<(), crate::__hal::SurfaceError> {
        Ok(())
    }

    unsafe fn get_timestamp_period(&self) -> f32 {
        1.0
    }

    unsafe fn wait_for_idle(&self) -> Result<(), crate::__hal::DeviceError> {
        Ok(())
    }
}

impl crate::__hal::Device for Context {
    type A = Api;

    unsafe fn create_buffer(&self, desc: &crate::__hal::BufferDescriptor) -> DeviceResult<Buffer> {
        Buffer::new(desc)
    }

    unsafe fn destroy_buffer(&self, buffer: Buffer) {}
    unsafe fn add_raw_buffer(&self, _buffer: &Buffer) {}

    unsafe fn map_buffer(
        &self,
        buffer: &Buffer,
        range: crate::__hal::MemoryRange,
    ) -> DeviceResult<crate::__hal::BufferMapping> {
        // Safety: the `wgpu-core` validation layer will prevent any user-accessible aliasing
        // mappings from being created, so we don’t need to perform any checks here, except for
        // bounds checks on the range which are built into `get_slice_ptr()`.
        Ok(crate::__hal::BufferMapping {
            ptr: ptr::NonNull::new(buffer.get_slice_ptr(range).cast::<u8>()).unwrap(),
            is_coherent: true,
        })
    }
    unsafe fn unmap_buffer(&self, buffer: &Buffer) {}
    unsafe fn flush_mapped_ranges<I>(&self, buffer: &Buffer, ranges: I) {}
    unsafe fn invalidate_mapped_ranges<I>(&self, buffer: &Buffer, ranges: I) {}

    unsafe fn create_texture(
        &self,
        desc: &crate::__hal::TextureDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_texture(&self, texture: Resource) {}
    unsafe fn add_raw_texture(&self, _texture: &Resource) {}

    unsafe fn create_texture_view(
        &self,
        texture: &Resource,
        desc: &crate::__hal::TextureViewDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_texture_view(&self, view: Resource) {}
    unsafe fn create_sampler(
        &self,
        desc: &crate::__hal::SamplerDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_sampler(&self, sampler: Resource) {}

    unsafe fn create_command_encoder(
        &self,
        desc: &crate::__hal::CommandEncoderDescriptor<Context>,
    ) -> DeviceResult<CommandBuffer> {
        Ok(CommandBuffer::new())
    }

    unsafe fn create_bind_group_layout(
        &self,
        desc: &crate::__hal::BindGroupLayoutDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_bind_group_layout(&self, bg_layout: Resource) {}
    unsafe fn create_pipeline_layout(
        &self,
        desc: &crate::__hal::PipelineLayoutDescriptor<Resource>,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: Resource) {}
    unsafe fn create_bind_group(
        &self,
        desc: &crate::__hal::BindGroupDescriptor<Resource, Buffer, Resource, Resource, Resource>,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_bind_group(&self, group: Resource) {}

    unsafe fn create_shader_module(
        &self,
        desc: &crate::__hal::ShaderModuleDescriptor,
        shader: crate::__hal::ShaderInput,
    ) -> Result<Resource, crate::__hal::ShaderError> {
        Ok(Resource)
    }
    unsafe fn destroy_shader_module(&self, module: Resource) {}
    unsafe fn create_render_pipeline(
        &self,
        desc: &crate::__hal::RenderPipelineDescriptor<Resource, Resource, Resource>,
    ) -> Result<Resource, crate::__hal::PipelineError> {
        Ok(Resource)
    }
    unsafe fn destroy_render_pipeline(&self, pipeline: Resource) {}
    unsafe fn create_compute_pipeline(
        &self,
        desc: &crate::__hal::ComputePipelineDescriptor<Resource, Resource, Resource>,
    ) -> Result<Resource, crate::__hal::PipelineError> {
        Ok(Resource)
    }
    unsafe fn destroy_compute_pipeline(&self, pipeline: Resource) {}
    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &crate::__hal::RayTracingPipelineDescriptor<Resource, Resource, Resource>,
    ) -> Result<Resource, crate::__hal::PipelineError> {
        Ok(Resource)
    }
    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: Resource) {}
    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &Resource,
        groups: core::ops::Range<u32>,
    ) -> Result<Vec<u8>, crate::__hal::DeviceError> {
        Ok(vec![0; groups.count()])
    }
    unsafe fn create_pipeline_cache(
        &self,
        desc: &crate::__hal::PipelineCacheDescriptor<'_>,
    ) -> Result<Resource, crate::__hal::PipelineCacheError> {
        Ok(Resource)
    }
    unsafe fn destroy_pipeline_cache(&self, cache: Resource) {}

    unsafe fn create_query_set(
        &self,
        desc: &crate::__types::QuerySetDescriptor<crate::__hal::Label>,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_query_set(&self, set: Resource) {}
    unsafe fn create_fence(&self) -> DeviceResult<Fence> {
        Ok(Fence {
            value: AtomicU64::new(0),
        })
    }
    unsafe fn destroy_fence(&self, fence: Fence) {}
    unsafe fn get_fence_value(&self, fence: &Fence) -> DeviceResult<crate::__hal::FenceValue> {
        Ok(fence.value.load(Ordering::Acquire))
    }
    unsafe fn wait(
        &self,
        fence: &Fence,
        value: crate::__hal::FenceValue,
        timeout: Option<Duration>,
    ) -> DeviceResult<bool> {
        // The relevant commands must have already been submitted, and noop-backend commands are
        // executed synchronously, so there is no waiting — either it is already done,
        // or this method was called incorrectly.
        assert!(
            fence.value.load(Ordering::Acquire) >= value,
            "submission must have already been done"
        );
        Ok(true)
    }

    unsafe fn start_graphics_debugger_capture(&self) -> bool {
        false
    }
    unsafe fn stop_graphics_debugger_capture(&self) {}
    unsafe fn create_acceleration_structure(
        &self,
        desc: &crate::__hal::AccelerationStructureDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn get_acceleration_structure_build_sizes<'a>(
        &self,
        _desc: &crate::__hal::GetAccelerationStructureBuildSizesDescriptor<'a, Buffer>,
    ) -> crate::__hal::AccelerationStructureBuildSizes {
        Default::default()
    }
    unsafe fn get_acceleration_structure_device_address(
        &self,
        _acceleration_structure: &Resource,
    ) -> crate::__types::BufferAddress {
        Default::default()
    }
    unsafe fn destroy_acceleration_structure(&self, _acceleration_structure: Resource) {}

    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8> {
        vec![]
    }

    fn get_internal_counters(&self) -> crate::__types::HalCounters {
        Default::default()
    }

    fn check_if_oom(&self) -> DeviceResult<()> {
        Ok(())
    }
}
