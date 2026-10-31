#![allow(unused_variables, unused_unsafe)]
type ResultDevice<T> = Result<T, crate::__hal::DeviceError>;
use super::*;
impl crate::__hal::Device for HalDevice {
    type A = Api;
    unsafe fn create_buffer(&self, desc: &crate::__hal::BufferDescriptor) -> ResultDevice<Buffer> {
        unsafe {
            let uniform = desc.usage.contains(crate::__types::BufferUses::UNIFORM);
            let size =
                usize::try_from(desc.size).map_err(|_| crate::__hal::DeviceError::OutOfMemory)?;
            let allocated = if uniform {
                size.div_ceil(256) * 256
            } else {
                size.max(4).div_ceil(4) * 4
            };
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(allocated)
                .map_err(|_| crate::__hal::DeviceError::OutOfMemory)?;
            bytes.resize(allocated, 0u8);
            let mut flags = 0;
            if uniform {
                flags = D3D11_BIND_CONSTANT_BUFFER.0 as u32;
            } else {
                if desc.usage.contains(crate::__types::BufferUses::VERTEX) {
                    flags |= D3D11_BIND_VERTEX_BUFFER.0 as u32;
                }
                if desc.usage.contains(crate::__types::BufferUses::INDEX) {
                    flags |= D3D11_BIND_INDEX_BUFFER.0 as u32;
                }
                if desc.usage.intersects(
                    crate::__types::BufferUses::STORAGE_READ_ONLY
                        | crate::__types::BufferUses::STORAGE_READ_WRITE,
                ) {
                    flags |= D3D11_BIND_SHADER_RESOURCE.0 as u32;
                }
            }
            let mut raw = None;
            if flags != 0 {
                self.0
                    .raw
                    .CreateBuffer(
                        &D3D11_BUFFER_DESC {
                            ByteWidth: allocated
                                .try_into()
                                .map_err(|_| crate::__hal::DeviceError::OutOfMemory)?,
                            Usage: if uniform {
                                D3D11_USAGE_DYNAMIC
                            } else {
                                D3D11_USAGE_DEFAULT
                            },
                            BindFlags: flags,
                            CPUAccessFlags: if uniform {
                                D3D11_CPU_ACCESS_WRITE.0 as u32
                            } else {
                                0
                            },
                            MiscFlags: if flags & D3D11_BIND_SHADER_RESOURCE.0 as u32 != 0 {
                                D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS.0 as u32
                            } else {
                                0
                            },
                            ..Default::default()
                        },
                        Some(&D3D11_SUBRESOURCE_DATA {
                            pSysMem: bytes.as_ptr().cast(),
                            ..Default::default()
                        }),
                        Some(&mut raw),
                    )
                    .map_err(device_error)?;
            }
            Ok(Buffer(Arc::new(BufferInner {
                raw,
                bytes: UnsafeCell::new(bytes.into_boxed_slice()),
                uniform,
            })))
        }
    }
    unsafe fn destroy_buffer(&self, buffer: Buffer) {
        unsafe {}
    }
    unsafe fn add_raw_buffer(&self, _buffer: &Buffer) {
        unsafe {}
    }
    unsafe fn map_buffer(
        &self,
        buffer: &Buffer,
        range: crate::__hal::MemoryRange,
    ) -> ResultDevice<crate::__hal::BufferMapping> {
        unsafe {
            let size = buffer.bytes().len();
            if range.start > range.end || range.end > size as u64 {
                return unsupported();
            }
            Ok(crate::__hal::BufferMapping {
                ptr: NonNull::new(
                    (*buffer.0.bytes.get())
                        .as_mut_ptr()
                        .add(range.start as usize),
                )
                .unwrap(),
                is_coherent: true,
            })
        }
    }
    unsafe fn unmap_buffer(&self, buffer: &Buffer) {
        unsafe {}
    }
    unsafe fn flush_mapped_ranges<I>(&self, buffer: &Buffer, ranges: I) {
        unsafe {}
    }
    unsafe fn invalidate_mapped_ranges<I>(&self, buffer: &Buffer, ranges: I) {
        unsafe {}
    }
    unsafe fn create_texture(
        &self,
        desc: &crate::__hal::TextureDescriptor,
    ) -> ResultDevice<Texture> {
        unsafe {
            if desc.dimension != crate::__types::TextureDimension::D2
                || desc.sample_count != 1
                || desc.usage.intersects(
                    crate::__types::TextureUses::STORAGE_READ_WRITE
                        | crate::__types::TextureUses::DEPTH_STENCIL_WRITE,
                )
            {
                return unsupported();
            }
            let (mut f, _) = format(desc.format).ok_or(crate::__hal::DeviceError::Unexpected)?;
            if matches!(
                desc.format,
                crate::__types::TextureFormat::Rgba8Unorm
                    | crate::__types::TextureFormat::Rgba8UnormSrgb
            ) {
                f = DXGI_FORMAT_R8G8B8A8_TYPELESS;
            }
            if matches!(
                desc.format,
                crate::__types::TextureFormat::Bgra8Unorm
                    | crate::__types::TextureFormat::Bgra8UnormSrgb
            ) {
                f = DXGI_FORMAT_B8G8R8A8_TYPELESS;
            }
            let mut flags = 0;
            if desc.usage.contains(crate::__types::TextureUses::RESOURCE) {
                flags |= D3D11_BIND_SHADER_RESOURCE.0 as u32;
            }
            if desc
                .usage
                .contains(crate::__types::TextureUses::COLOR_TARGET)
            {
                flags |= D3D11_BIND_RENDER_TARGET.0 as u32;
            }
            let mut raw = None;
            self.0
                .raw
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: desc.size.width,
                        Height: desc.size.height,
                        ArraySize: desc.size.depth_or_array_layers,
                        MipLevels: desc.mip_level_count,
                        Format: f,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: flags,
                        ..Default::default()
                    },
                    None,
                    Some(&mut raw),
                )
                .map_err(device_error)?;
            Ok(Texture {
                raw: raw.unwrap(),
                format: desc.format,
                size: desc.size,
                mips: desc.mip_level_count,
            })
        }
    }
    unsafe fn destroy_texture(&self, texture: Texture) {
        unsafe {}
    }
    unsafe fn add_raw_texture(&self, _texture: &Texture) {
        unsafe {}
    }
    unsafe fn create_texture_view(
        &self,
        texture: &Texture,
        desc: &crate::__hal::TextureViewDescriptor,
    ) -> ResultDevice<TextureView> {
        unsafe {
            let (f, _) = format(desc.format).ok_or(crate::__hal::DeviceError::Unexpected)?;
            let mut srv = None;
            let mut rtv = None;
            let r = &desc.range;
            let mips = r.mip_level_count.unwrap_or(texture.mips - r.base_mip_level);
            let layers = r
                .array_layer_count
                .unwrap_or(texture.size.depth_or_array_layers - r.base_array_layer);
            if !matches!(
                desc.dimension,
                crate::__types::TextureViewDimension::D2
                    | crate::__types::TextureViewDimension::D2Array
            ) {
                return unsupported();
            }
            if desc.usage.contains(crate::__types::TextureUses::RESOURCE) {
                let d = if desc.dimension == crate::__types::TextureViewDimension::D2 {
                    D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: f,
                        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_SRV {
                                MostDetailedMip: r.base_mip_level,
                                MipLevels: mips,
                            },
                        },
                    }
                } else {
                    D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: f,
                        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2DARRAY,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2DArray: D3D11_TEX2D_ARRAY_SRV {
                                MostDetailedMip: r.base_mip_level,
                                MipLevels: mips,
                                FirstArraySlice: r.base_array_layer,
                                ArraySize: layers,
                            },
                        },
                    }
                };
                self.0
                    .raw
                    .CreateShaderResourceView(&texture.raw, Some(&d), Some(&mut srv))
                    .map_err(device_error)?;
            }
            if desc
                .usage
                .contains(crate::__types::TextureUses::COLOR_TARGET)
            {
                let d = if desc.dimension == crate::__types::TextureViewDimension::D2 {
                    D3D11_RENDER_TARGET_VIEW_DESC {
                        Format: f,
                        ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_RTV {
                                MipSlice: r.base_mip_level,
                            },
                        },
                    }
                } else {
                    D3D11_RENDER_TARGET_VIEW_DESC {
                        Format: f,
                        ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2DARRAY,
                        Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                            Texture2DArray: D3D11_TEX2D_ARRAY_RTV {
                                MipSlice: r.base_mip_level,
                                FirstArraySlice: r.base_array_layer,
                                ArraySize: layers,
                            },
                        },
                    }
                };
                self.0
                    .raw
                    .CreateRenderTargetView(&texture.raw, Some(&d), Some(&mut rtv))
                    .map_err(device_error)?;
            }
            Ok(TextureView { srv, rtv })
        }
    }
    unsafe fn destroy_texture_view(&self, view: TextureView) {
        unsafe {}
    }
    unsafe fn create_sampler(
        &self,
        desc: &crate::__hal::SamplerDescriptor,
    ) -> ResultDevice<Sampler> {
        unsafe {
            let address = |a| match a {
                crate::__types::AddressMode::ClampToEdge => D3D11_TEXTURE_ADDRESS_CLAMP,
                crate::__types::AddressMode::Repeat => D3D11_TEXTURE_ADDRESS_WRAP,
                crate::__types::AddressMode::MirrorRepeat => D3D11_TEXTURE_ADDRESS_MIRROR,
                _ => D3D11_TEXTURE_ADDRESS_BORDER,
            };
            let filter = if desc.anisotropy_clamp > 1 {
                D3D11_FILTER_ANISOTROPIC.0
            } else {
                ((desc.min_filter == crate::__types::FilterMode::Linear) as i32) << 4
                    | ((desc.mag_filter == crate::__types::FilterMode::Linear) as i32) << 2
                    | ((desc.mipmap_filter == crate::__types::MipmapFilterMode::Linear) as i32)
            } | if desc.compare.is_some() { 0x80 } else { 0 };
            let mut raw = None;
            self.0
                .raw
                .CreateSamplerState(
                    &D3D11_SAMPLER_DESC {
                        Filter: D3D11_FILTER(filter),
                        AddressU: address(desc.address_modes[0]),
                        AddressV: address(desc.address_modes[1]),
                        AddressW: address(desc.address_modes[2]),
                        MinLOD: desc.lod_clamp.start,
                        MaxLOD: desc.lod_clamp.end,
                        MaxAnisotropy: desc.anisotropy_clamp as u32,
                        ComparisonFunc: D3D11_COMPARISON_FUNC(desc.compare.map_or(8, |c| c as i32)),
                        ..Default::default()
                    },
                    Some(&mut raw),
                )
                .map_err(device_error)?;
            Ok(Sampler(raw.unwrap()))
        }
    }
    unsafe fn destroy_sampler(&self, sampler: Sampler) {
        unsafe {}
    }
    unsafe fn create_command_encoder(
        &self,
        desc: &crate::__hal::CommandEncoderDescriptor<HalDevice>,
    ) -> ResultDevice<CommandBuffer> {
        unsafe { Ok(CommandBuffer::new(self.clone())) }
    }
    unsafe fn create_bind_group_layout(
        &self,
        desc: &crate::__hal::BindGroupLayoutDescriptor,
    ) -> ResultDevice<BindGroupLayout> {
        unsafe {
            if desc.entries.iter().any(|e| e.count.is_some()) {
                return unsupported();
            }
            Ok(BindGroupLayout(desc.entries.to_vec()))
        }
    }
    unsafe fn destroy_bind_group_layout(&self, bg_layout: BindGroupLayout) {
        unsafe {}
    }
    unsafe fn create_pipeline_layout(
        &self,
        desc: &crate::__hal::PipelineLayoutDescriptor<BindGroupLayout>,
    ) -> ResultDevice<PipelineLayout> {
        unsafe { self.layout(desc) }
    }
    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: PipelineLayout) {
        unsafe {}
    }
    unsafe fn create_bind_group(
        &self,
        desc: &crate::__hal::BindGroupDescriptor<
            BindGroupLayout,
            Buffer,
            Sampler,
            TextureView,
            Unsupported,
        >,
    ) -> ResultDevice<BindGroup> {
        unsafe { self.group(desc) }
    }
    unsafe fn destroy_bind_group(&self, group: BindGroup) {
        unsafe {}
    }
    unsafe fn create_shader_module(
        &self,
        desc: &crate::__hal::ShaderModuleDescriptor,
        shader: crate::__hal::ShaderInput,
    ) -> Result<ShaderModule, crate::__hal::ShaderError> {
        unsafe {
            match shader {
                crate::__hal::ShaderInput::Naga(n) => Ok(ShaderModule(n)),
                _ => Err(crate::__hal::ShaderError::Compilation(
                    "native DX11 requires validated Naga IR".into(),
                )),
            }
        }
    }
    unsafe fn destroy_shader_module(&self, module: ShaderModule) {
        unsafe {}
    }
    unsafe fn create_render_pipeline(
        &self,
        desc: &crate::__hal::RenderPipelineDescriptor<PipelineLayout, ShaderModule, Unsupported>,
    ) -> Result<Pipeline, crate::__hal::PipelineError> {
        unsafe { self.pipeline(desc) }
    }
    unsafe fn destroy_render_pipeline(&self, pipeline: Pipeline) {
        unsafe {}
    }
    unsafe fn create_compute_pipeline(
        &self,
        desc: &crate::__hal::ComputePipelineDescriptor<PipelineLayout, ShaderModule, Unsupported>,
    ) -> Result<Unsupported, crate::__hal::PipelineError> {
        unsafe {
            Err(crate::__hal::PipelineError::Linkage(
                crate::__types::ShaderStages::COMPUTE,
                "DX11 graphics HAL does not advertise compute".into(),
            ))
        }
    }
    unsafe fn destroy_compute_pipeline(&self, pipeline: Unsupported) {
        unsafe {}
    }
    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &crate::__hal::RayTracingPipelineDescriptor<
            PipelineLayout,
            ShaderModule,
            Unsupported,
        >,
    ) -> Result<Unsupported, crate::__hal::PipelineError> {
        unsafe {
            Err(crate::__hal::PipelineError::Device(
                crate::__hal::DeviceError::Unexpected,
            ))
        }
    }
    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: Unsupported) {
        unsafe {}
    }
    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &Unsupported,
        groups: core::ops::Range<u32>,
    ) -> Result<Vec<u8>, crate::__hal::DeviceError> {
        unsafe { unsupported() }
    }
    unsafe fn create_pipeline_cache(
        &self,
        desc: &crate::__hal::PipelineCacheDescriptor<'_>,
    ) -> Result<Unsupported, crate::__hal::PipelineCacheError> {
        unsafe {
            Err(crate::__hal::PipelineCacheError::Device(
                crate::__hal::DeviceError::Unexpected,
            ))
        }
    }
    unsafe fn destroy_pipeline_cache(&self, cache: Unsupported) {
        unsafe {}
    }
    unsafe fn create_query_set(
        &self,
        desc: &crate::__types::QuerySetDescriptor<crate::__hal::Label>,
    ) -> ResultDevice<Unsupported> {
        unsafe { unsupported() }
    }
    unsafe fn destroy_query_set(&self, set: Unsupported) {
        unsafe {}
    }
    unsafe fn create_fence(&self) -> ResultDevice<Fence> {
        unsafe {
            Ok(Fence {
                queries: Mutex::new(FenceQueries::default()),
                completed: crate::__hal::AtomicFenceValue::new(0),
            })
        }
    }
    unsafe fn destroy_fence(&self, fence: Fence) {
        unsafe {}
    }
    unsafe fn get_fence_value(&self, fence: &Fence) -> ResultDevice<crate::__hal::FenceValue> {
        unsafe { self.poll_fence(fence) }
    }
    unsafe fn wait(
        &self,
        fence: &Fence,
        value: crate::__hal::FenceValue,
        timeout: Option<Duration>,
    ) -> ResultDevice<bool> {
        unsafe {
            let start = std::time::Instant::now();
            self.0.context.lock().Flush();
            loop {
                if self.poll_fence(fence)? >= value {
                    return Ok(true);
                }
                if timeout.is_some_and(|t| start.elapsed() >= t) {
                    return Ok(false);
                }
                std::thread::yield_now();
            }
        }
    }
    unsafe fn start_graphics_debugger_capture(&self) -> bool {
        unsafe { false }
    }
    unsafe fn stop_graphics_debugger_capture(&self) {
        unsafe {}
    }
    unsafe fn create_acceleration_structure(
        &self,
        desc: &crate::__hal::AccelerationStructureDescriptor,
    ) -> ResultDevice<Unsupported> {
        unsafe { unsupported() }
    }
    unsafe fn get_acceleration_structure_build_sizes<'a>(
        &self,
        _desc: &crate::__hal::GetAccelerationStructureBuildSizesDescriptor<'a, Buffer>,
    ) -> crate::__hal::AccelerationStructureBuildSizes {
        unsafe { panic!("DX11 ray tracing is not advertised") }
    }
    unsafe fn get_acceleration_structure_device_address(
        &self,
        _acceleration_structure: &Unsupported,
    ) -> crate::__types::BufferAddress {
        unsafe { panic!("DX11 ray tracing is not advertised") }
    }
    unsafe fn destroy_acceleration_structure(&self, _acceleration_structure: Unsupported) {
        unsafe {}
    }
    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8> {
        unsafe { panic!("DX11 ray tracing is not advertised") }
    }
    fn get_internal_counters(&self) -> crate::__types::HalCounters {
        unsafe { Default::default() }
    }
    fn check_if_oom(&self) -> ResultDevice<()> {
        unsafe { self.0.raw.GetDeviceRemovedReason().map_err(device_error) }
    }
}
