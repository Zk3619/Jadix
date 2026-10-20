//! Experimental SM5 graphics HAL. Unsupported features are deliberately not advertised.
use crate::TlasInstance;
use alloc::{boxed::Box, collections::VecDeque, format, string::String, sync::Arc, vec, vec::Vec};
use core::{cell::UnsafeCell, ptr::NonNull, time::Duration};
use parking_lot::Mutex;
use windows::{
    core::{Interface, PCSTR},
    Win32::{
        Foundation::{DXGI_STATUS_OCCLUDED, HWND, RECT},
        Graphics::{
            Direct3D::*,
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
};
mod command;
mod device;
mod pipeline;
pub use command::CommandBuffer;
use pipeline::*;

#[derive(Clone, Debug)]
pub struct Api;
#[derive(Debug)]
pub struct Instance;
impl Instance {
    pub fn new() -> Result<Self, crate::InstanceError> {
        Ok(Self)
    }
}
#[derive(Debug)]
pub struct HalAdapter(super::Adapter);
impl HalAdapter {
    pub fn raw_adapter(&self) -> &IDXGIAdapter1 {
        &self.0.raw
    }
}
#[derive(Clone, Debug)]
pub struct HalDevice(Arc<super::Device>);
#[derive(Debug)]
pub struct Surface {
    hwnd: isize,
    state: Mutex<Option<Swapchain>>,
}
#[derive(Debug)]
struct Swapchain {
    raw: IDXGISwapChain1,
    owner: ID3D11Device,
    config: crate::SurfaceConfiguration,
}
#[derive(Clone, Debug)]
pub struct Texture {
    raw: ID3D11Texture2D,
    format: wgt::TextureFormat,
    size: wgt::Extent3d,
    mips: u32,
}
#[derive(Clone, Debug)]
pub struct TextureView {
    srv: Option<ID3D11ShaderResourceView>,
    rtv: Option<ID3D11RenderTargetView>,
}
#[derive(Clone, Debug)]
pub struct Sampler(ID3D11SamplerState);
#[derive(Clone, Debug)]
pub struct Buffer(Arc<BufferInner>);
#[derive(Debug)]
struct BufferInner {
    raw: Option<ID3D11Buffer>,
    bytes: UnsafeCell<Box<[u8]>>,
    uniform: bool,
}
// HAL callers must synchronize host mappings and submission. The backing allocation never moves;
// only the mapped range may be accessed by the caller until unmap. Queue execution is serialized.
unsafe impl Send for BufferInner {}
unsafe impl Sync for BufferInner {}
impl Buffer {
    unsafe fn bytes(&self) -> &mut [u8] {
        unsafe { &mut *self.0.bytes.get() }
    }
}
#[derive(Debug, Default)]
struct FenceQueries {
    pending: VecDeque<(u64, ID3D11Query)>,
    recycled: Option<ID3D11Query>,
}
#[derive(Debug)]
pub struct Fence {
    queries: Mutex<FenceQueries>,
    completed: crate::AtomicFenceValue,
}
#[derive(Debug)]
pub struct Unsupported;
impl crate::Api for Api {
    const VARIANT: wgt::Backend = wgt::Backend::Dx11;
    type Instance = Instance;
    type Adapter = HalAdapter;
    type Surface = Surface;
    type Device = HalDevice;
    type Queue = HalDevice;
    type CommandEncoder = CommandBuffer;
    type CommandBuffer = CommandBuffer;
    type Buffer = Buffer;
    type Texture = Texture;
    type SurfaceTexture = Texture;
    type TextureView = TextureView;
    type Sampler = Sampler;
    type Fence = Fence;
    type QuerySet = Unsupported;
    type PipelineCache = Unsupported;
    type AccelerationStructure = Unsupported;
    type BindGroupLayout = BindGroupLayout;
    type BindGroup = BindGroup;
    type PipelineLayout = PipelineLayout;
    type ShaderModule = ShaderModule;
    type RenderPipeline = Pipeline;
    type ComputePipeline = Unsupported;
    type RayTracingPipeline = Unsupported;
}
crate::impl_dyn_resource!(
    Instance,
    HalAdapter,
    Surface,
    HalDevice,
    Buffer,
    Texture,
    TextureView,
    Sampler,
    Fence,
    Unsupported,
    BindGroupLayout,
    BindGroup,
    PipelineLayout,
    ShaderModule,
    Pipeline,
    CommandBuffer
);
impl crate::DynBuffer for Buffer {}
impl crate::DynTexture for Texture {}
impl crate::DynSurfaceTexture for Texture {}
impl crate::DynTextureView for TextureView {}
impl crate::DynSampler for Sampler {}
impl crate::DynFence for Fence {}
impl crate::DynQuerySet for Unsupported {}
impl crate::DynPipelineCache for Unsupported {}
impl crate::DynAccelerationStructure for Unsupported {}
impl crate::DynComputePipeline for Unsupported {}
impl crate::DynRayTracingPipeline for Unsupported {}
impl crate::DynBindGroupLayout for BindGroupLayout {}
impl crate::DynBindGroup for BindGroup {}
impl crate::DynPipelineLayout for PipelineLayout {}
impl crate::DynShaderModule for ShaderModule {}
impl crate::DynRenderPipeline for Pipeline {}
impl crate::DynCommandBuffer for CommandBuffer {}
impl core::borrow::Borrow<dyn crate::DynTexture> for Texture {
    fn borrow(&self) -> &dyn crate::DynTexture {
        self
    }
}

fn device_error(e: windows::core::Error) -> crate::DeviceError {
    log::error!("native D3D11: {e}");
    if e.code() == windows::Win32::Foundation::E_OUTOFMEMORY {
        crate::DeviceError::OutOfMemory
    } else {
        crate::DeviceError::Lost
    }
}
fn unsupported<T>() -> Result<T, crate::DeviceError> {
    Err(crate::DeviceError::Unexpected)
}
fn format(f: wgt::TextureFormat) -> Option<(DXGI_FORMAT, u32)> {
    use wgt::TextureFormat::*;
    Some(match f {
        R8Unorm => (DXGI_FORMAT_R8_UNORM, 1),
        Rg8Unorm => (DXGI_FORMAT_R8G8_UNORM, 2),
        Rgba8Unorm => (DXGI_FORMAT_R8G8B8A8_UNORM, 4),
        Rgba8UnormSrgb => (DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, 4),
        Bgra8Unorm => (DXGI_FORMAT_B8G8R8A8_UNORM, 4),
        Bgra8UnormSrgb => (DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, 4),
        Rgba16Float => (DXGI_FORMAT_R16G16B16A16_FLOAT, 8),
        R32Float => (DXGI_FORMAT_R32_FLOAT, 4),
        Rgba32Float => (DXGI_FORMAT_R32G32B32A32_FLOAT, 16),
        _ => return None,
    })
}
impl crate::Instance for Instance {
    type A = Api;
    unsafe fn init(_: &crate::InstanceDescriptor<'_>) -> Result<Self, crate::InstanceError> {
        Self::new()
    }
    unsafe fn create_surface(
        &self,
        _: raw_window_handle::RawDisplayHandle,
        w: raw_window_handle::RawWindowHandle,
    ) -> Result<Surface, crate::InstanceError> {
        match w {
            raw_window_handle::RawWindowHandle::Win32(w) => Ok(Surface {
                hwnd: w.hwnd.get(),
                state: Mutex::new(None),
            }),
            _ => Err(crate::InstanceError::new(
                "D3D11 requires Win32 HWND".into(),
            )),
        }
    }
    unsafe fn enumerate_adapters(&self, _: Option<&Surface>) -> Vec<crate::ExposedAdapter<Api>> {
        super::enumerate_adapters()
            .unwrap_or_else(|e| {
                log::error!("DX11 enumeration: {e}");
                vec![]
            })
            .into_iter()
            .map(|a| {
                let mut limits = wgt::Limits::downlevel_defaults();
                limits.max_texture_dimension_1d = 16384;
                limits.max_texture_dimension_2d = 16384;
                limits.max_storage_buffers_per_shader_stage = 8;
                limits.max_storage_buffer_binding_size = 128 << 20;
                limits.max_uniform_buffer_binding_size = 65536;
                limits.max_vertex_attributes = 32;
                limits.max_compute_workgroups_per_dimension = 0;
                limits.max_compute_invocations_per_workgroup = 0;
                limits.max_compute_workgroup_size_x = 0;
                limits.max_compute_workgroup_size_y = 0;
                limits.max_compute_workgroup_size_z = 0;
                let mut caps = crate::noop::CAPABILITIES.clone();
                caps.limits = limits;
                caps.alignments.buffer_copy_offset = core::num::NonZeroU64::new(4).unwrap();
                caps.alignments.buffer_copy_pitch = core::num::NonZeroU64::new(256).unwrap();
                caps.alignments.uniform_bounds_check_alignment =
                    core::num::NonZeroU64::new(256).unwrap();
                caps.downlevel = wgt::DownlevelCapabilities {
                    flags: wgt::DownlevelFlags::BASE_VERTEX
                        | wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES
                        | wgt::DownlevelFlags::COMPARISON_SAMPLERS
                        | wgt::DownlevelFlags::INDEPENDENT_BLEND
                        | wgt::DownlevelFlags::VERTEX_STORAGE
                        | wgt::DownlevelFlags::FRAGMENT_STORAGE
                        | wgt::DownlevelFlags::ANISOTROPIC_FILTERING
                        | wgt::DownlevelFlags::UNRESTRICTED_INDEX_BUFFER
                        | wgt::DownlevelFlags::VIEW_FORMATS
                        | wgt::DownlevelFlags::SURFACE_VIEW_FORMATS,
                    limits: wgt::DownlevelLimits {},
                    shader_model: wgt::ShaderModel::Sm5,
                };
                let d = &a.description;
                let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
                info.name = String::from_utf16_lossy(
                    &d.Description[..d
                        .Description
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(d.Description.len())],
                );
                info.vendor = d.VendorId;
                info.device = d.DeviceId;
                info.driver = "native D3D11 HAL / FXC SM5".into();
                info.driver_info =
                    format!("LUID {}:{}", d.AdapterLuid.HighPart, d.AdapterLuid.LowPart);
                crate::ExposedAdapter {
                    adapter: HalAdapter(a),
                    info,
                    features: wgt::Features::empty(),
                    capabilities: caps,
                }
            })
            .collect()
    }
}
impl crate::Adapter for HalAdapter {
    type A = Api;
    unsafe fn open(
        &self,
        features: wgt::Features,
        _: &wgt::Limits,
        _: &wgt::MemoryHints,
    ) -> Result<crate::OpenDevice<Api>, crate::DeviceError> {
        if !features.is_empty() {
            return unsupported();
        }
        let device = HalDevice(Arc::new(self.0.open().map_err(|e| {
            log::error!("{e}");
            crate::DeviceError::Lost
        })?));
        let _: ID3D11DeviceContext1 = device.0.context.lock().cast().map_err(device_error)?;
        Ok(crate::OpenDevice {
            queue: device.clone(),
            device,
        })
    }
    unsafe fn texture_format_capabilities(
        &self,
        f: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        use crate::TextureFormatCapabilities as C;
        if format(f).is_none() {
            return C::empty();
        }
        C::SAMPLED
            | C::SAMPLED_LINEAR
            | C::COLOR_ATTACHMENT
            | C::COLOR_ATTACHMENT_BLEND
            | C::COPY_SRC
            | C::COPY_DST
    }
    unsafe fn surface_capabilities(&self, _: &Surface) -> Option<crate::SurfaceCapabilities> {
        Some(crate::SurfaceCapabilities {
            formats: [
                wgt::TextureFormat::Bgra8Unorm,
                wgt::TextureFormat::Bgra8UnormSrgb,
            ]
            .into_iter()
            .map(|format| wgt::SurfaceFormatCapabilities {
                format,
                color_spaces: wgt::SurfaceColorSpaces::SRGB,
            })
            .collect(),
            maximum_frame_latency: 1..=3,
            current_extent: None,
            usage: wgt::TextureUses::COLOR_TARGET
                | wgt::TextureUses::COPY_SRC
                | wgt::TextureUses::COPY_DST,
            present_modes: vec![wgt::PresentMode::Fifo, wgt::PresentMode::Immediate],
            composite_alpha_modes: vec![wgt::CompositeAlphaMode::Opaque],
        })
    }
    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp {
        wgt::PresentationTimestamp::INVALID_TIMESTAMP
    }
    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses {
        wgt::BufferUses::INCLUSIVE | wgt::BufferUses::MAP_WRITE
    }
    fn get_ordered_texture_usages(&self) -> wgt::TextureUses {
        wgt::TextureUses::INCLUSIVE | wgt::TextureUses::COLOR_TARGET
    }
}
impl crate::Surface for Surface {
    type A = Api;
    unsafe fn configure(
        &self,
        d: &HalDevice,
        c: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {
        if c.composite_alpha_mode != wgt::CompositeAlphaMode::Opaque
            || !matches!(
                c.format,
                wgt::TextureFormat::Bgra8Unorm | wgt::TextureFormat::Bgra8UnormSrgb
            )
        {
            return Err(crate::SurfaceError::Other(
                "unsupported DX11 surface configuration",
            ));
        }
        let mut state = self.state.lock();
        let context = d.0.context.lock();
        unsafe {
            context.ClearState();
            context.Flush();
        }
        if let Some(old) = state.as_mut() {
            if old.owner == d.0.raw {
                unsafe {
                    old.raw.ResizeBuffers(
                        0,
                        c.extent.width,
                        c.extent.height,
                        DXGI_FORMAT_UNKNOWN,
                        DXGI_SWAP_CHAIN_FLAG(0),
                    )
                }
                .map_err(|e| crate::SurfaceError::Device(device_error(e)))?;
                old.config = c.clone();
                return Ok(());
            }
        }
        *state = None;
        let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1() }
            .map_err(|e| crate::SurfaceError::Device(device_error(e)))?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: c.extent.width,
            Height: c.extent.height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };
        let raw = unsafe {
            factory.CreateSwapChainForHwnd(&d.0.raw, HWND(self.hwnd as *mut _), &desc, None, None)
        }
        .map_err(|e| crate::SurfaceError::Device(device_error(e)))?;
        unsafe { factory.MakeWindowAssociation(HWND(self.hwnd as *mut _), DXGI_MWA_NO_ALT_ENTER) }
            .map_err(|e| crate::SurfaceError::Device(device_error(e)))?;
        *state = Some(Swapchain {
            raw,
            owner: d.0.raw.clone(),
            config: c.clone(),
        });
        Ok(())
    }
    unsafe fn unconfigure(&self, d: &HalDevice) {
        unsafe {
            d.0.context.lock().ClearState();
        }
        *self.state.lock() = None;
    }
    unsafe fn acquire_texture(
        &self,
        _: Option<Duration>,
        _: &Fence,
    ) -> Result<crate::AcquiredSurfaceTexture<Api>, crate::SurfaceError> {
        let state = self.state.lock();
        let s = state.as_ref().ok_or(crate::SurfaceError::Outdated)?;
        let raw = unsafe { s.raw.GetBuffer::<ID3D11Texture2D>(0) }
            .map_err(|e| crate::SurfaceError::Device(device_error(e)))?;
        Ok(crate::AcquiredSurfaceTexture {
            texture: Texture {
                raw,
                format: s.config.format,
                size: s.config.extent,
                mips: 1,
            },
            suboptimal: false,
        })
    }
    unsafe fn discard_texture(&self, _: Texture) {}
}
impl HalDevice {
    fn poll_fence(&self, f: &Fence) -> Result<u64, crate::DeviceError> {
        use core::sync::atomic::Ordering;
        let context = self.0.context.lock();
        let mut queries = f.queries.lock();
        while let Some((value, q)) = queries.pending.front() {
            let mut done = 0i32;
            let hr = unsafe {
                context.GetData(
                    q,
                    Some(core::ptr::from_mut(&mut done).cast()),
                    4,
                    D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
                )
            };
            if let Err(e) = hr {
                return Err(device_error(e));
            }
            if done == 0 {
                break;
            }
            f.completed.store(*value, Ordering::Release);
            // Reuse only an observed-complete event, retaining at most one spare.
            // Pending and spare queries share the existing fence lock.
            queries.recycled = queries.pending.pop_front().map(|(_, query)| query);
        }
        Ok(f.completed.load(Ordering::Acquire))
    }
}
impl crate::Queue for HalDevice {
    type A = Api;
    unsafe fn submit(
        &self,
        cbs: &[&CommandBuffer],
        _: &[&Texture],
        (f, value): (&Fence, u64),
    ) -> Result<(), crate::DeviceError> {
        let context = self.0.context.lock();
        for cb in cbs {
            for command in &cb.commands {
                (command)(&context)?;
            }
        }
        let mut queries = f.queries.lock();
        let q = if let Some(query) = queries.recycled.take() {
            query
        } else {
            let mut query = None;
            unsafe {
                self.0.raw.CreateQuery(
                    &D3D11_QUERY_DESC {
                        Query: D3D11_QUERY_EVENT,
                        MiscFlags: 0,
                    },
                    Some(&mut query),
                )
            }
            .map_err(device_error)?;
            query.unwrap()
        };
        unsafe {
            context.End(&q);
            context.Flush();
        }
        queries.pending.push_back((value, q));
        Ok(())
    }
    unsafe fn present(&self, s: &Surface, _: Texture) -> Result<(), crate::SurfaceError> {
        let state = s.state.lock();
        let s = state.as_ref().ok_or(crate::SurfaceError::Outdated)?;
        let hr = unsafe {
            s.raw.Present(
                u32::from(s.config.present_mode == wgt::PresentMode::Fifo),
                DXGI_PRESENT(0),
            )
        };
        if hr == DXGI_STATUS_OCCLUDED {
            return Err(crate::SurfaceError::Occluded);
        }
        hr.ok()
            .map_err(|e| crate::SurfaceError::Device(device_error(e)))
    }
    unsafe fn wait_for_idle(&self) -> Result<(), crate::DeviceError> {
        let f = Fence {
            queries: Mutex::new(FenceQueries::default()),
            completed: crate::AtomicFenceValue::new(0),
        };
        unsafe {
            crate::Queue::submit(self, &[], &[], (&f, 1))?;
            crate::Device::wait(self, &f, 1, None)?;
        }
        Ok(())
    }
    unsafe fn get_timestamp_period(&self) -> f32 {
        1.0
    }
}
