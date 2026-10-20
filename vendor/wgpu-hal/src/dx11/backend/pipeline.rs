use super::*;
#[derive(Clone, Debug)]
pub struct BindGroupLayout(pub Vec<wgt::BindGroupLayoutEntry>);
#[derive(Clone, Debug)]
pub struct PipelineLayout {
    pub map: Arc<naga::back::hlsl::BindingMap>,
}
#[derive(Clone, Debug)]
pub struct BindGroup {
    pub entries: Arc<[(u32, wgt::ShaderStages, Binding)]>,
}
#[derive(Clone, Debug)]
pub enum Binding {
    Uniform {
        buffer: Buffer,
        offset: u64,
        size: u64,
        dynamic: bool,
    },
    Storage {
        buffer: Buffer,
        offset: u64,
        size: u64,
        srv: ID3D11ShaderResourceView,
    },
    Texture(Option<ID3D11ShaderResourceView>),
    Sampler(ID3D11SamplerState),
}
#[derive(Debug)]
pub struct ShaderModule(pub crate::NagaShader);
#[derive(Clone, Debug)]
pub struct Pipeline {
    vs: ID3D11VertexShader,
    ps: Option<ID3D11PixelShader>,
    input: Option<ID3D11InputLayout>,
    raster: ID3D11RasterizerState,
    pub blend: ID3D11BlendState,
    depth: ID3D11DepthStencilState,
    special: Option<Arc<(ID3D11Buffer, Mutex<Option<[u32; 4]>>)>>,
    pub strides: Arc<[u32]>,
    topology: D3D_PRIMITIVE_TOPOLOGY,
    pub sample_mask: u32,
}
impl Pipeline {
    pub unsafe fn draw_constants(
        &self,
        c: &ID3D11DeviceContext,
        vertex: i32,
        instance: u32,
    ) -> Result<(), crate::DeviceError> {
        if let Some(special) = &self.special {
            let (b, last) = &**special;
            let values = [vertex as u32, instance, 0, 0];
            let mut last = last.lock();
            if *last == Some(values) {
                return Ok(());
            }
            unsafe {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                c.Map(b, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
                    .map_err(device_error)?;
                core::ptr::copy_nonoverlapping(
                    values.as_ptr(),
                    mapped.pData.cast(),
                    4,
                );
                c.Unmap(b, 0);
            }
            *last = Some(values);
        }
        Ok(())
    }
    pub unsafe fn bind(&self, c: &ID3D11DeviceContext) {
        unsafe {
            c.VSSetShader(&self.vs, None);
            c.PSSetShader(self.ps.as_ref(), None);
            c.IASetInputLayout(self.input.as_ref());
            c.IASetPrimitiveTopology(self.topology);
            c.RSSetState(&self.raster);
            c.OMSetBlendState(&self.blend, None, self.sample_mask);
            c.OMSetDepthStencilState(&self.depth, 0);
            if let Some(special) = &self.special {
                c.VSSetConstantBuffers(13, Some(&[Some(special.0.clone())]));
            }
        }
    }
}
impl HalDevice {
    pub fn layout(
        &self,
        d: &crate::PipelineLayoutDescriptor<BindGroupLayout>,
    ) -> Result<PipelineLayout, crate::DeviceError> {
        if d.immediate_size != 0 {
            return unsupported();
        }
        let mut map = naga::back::hlsl::BindingMap::default();
        let (mut b, mut t, mut s) = (0, 0, 0);
        for (group, g) in d.bind_group_layouts.iter().enumerate() {
            if let Some(g) = g {
                for e in &g.0 {
                    if e.count.is_some() {
                        return unsupported();
                    }
                    let slot = match e.ty {
                        wgt::BindingType::Buffer {
                            ty: wgt::BufferBindingType::Uniform,
                            ..
                        } => {
                            let n = b;
                            b += 1;
                            if b > 13 {
                                return unsupported();
                            }
                            n
                        }
                        wgt::BindingType::Buffer {
                            ty: wgt::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            ..
                        }
                        | wgt::BindingType::Texture { .. } => {
                            let n = t;
                            t += 1;
                            if t > 128 {
                                return unsupported();
                            }
                            n
                        }
                        wgt::BindingType::Sampler(_) => {
                            let n = s;
                            s += 1;
                            if s > 16 {
                                return unsupported();
                            }
                            n
                        }
                        _ => return unsupported(),
                    };
                    map.insert(
                        naga::ResourceBinding {
                            group: group as u32,
                            binding: e.binding,
                        },
                        naga::back::hlsl::BindTarget {
                            register: slot,
                            restrict_indexing: true,
                            ..Default::default()
                        },
                    );
                }
            }
        }
        Ok(PipelineLayout { map: Arc::new(map) })
    }
    pub fn group(
        &self,
        d: &crate::BindGroupDescriptor<BindGroupLayout, Buffer, Sampler, TextureView, Unsupported>,
    ) -> Result<BindGroup, crate::DeviceError> {
        let mut out = Vec::new();
        for e in d.entries {
            let layout = d
                .layout
                .0
                .iter()
                .find(|l| l.binding == e.binding)
                .ok_or(crate::DeviceError::Unexpected)?;
            if e.count != 1 {
                return unsupported();
            }
            let i = e.resource_index as usize;
            let binding = match layout.ty {
                wgt::BindingType::Buffer {
                    ty: wgt::BufferBindingType::Uniform,
                    has_dynamic_offset,
                    ..
                } => {
                    let b = &d.buffers[i];
                    Binding::Uniform {
                        buffer: b.buffer.clone(),
                        offset: b.offset,
                        size: b
                            .size
                            .map_or(unsafe { b.buffer.bytes().len() as u64 } - b.offset, |s| {
                                s.get()
                            }),
                        dynamic: has_dynamic_offset,
                    }
                }
                wgt::BindingType::Buffer {
                    ty: wgt::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    ..
                } => {
                    let b = &d.buffers[i];
                    let size = b
                        .size
                        .map_or(unsafe { b.buffer.bytes().len() as u64 } - b.offset, |s| {
                            s.get()
                        });
                    let mut srv = None;
                    let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: DXGI_FORMAT_R32_TYPELESS,
                        ViewDimension: D3D_SRV_DIMENSION_BUFFEREX,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            BufferEx: D3D11_BUFFEREX_SRV {
                                FirstElement: (b.offset / 4) as u32,
                                NumElements: (size / 4) as u32,
                                Flags: D3D11_BUFFEREX_SRV_FLAG_RAW.0 as u32,
                            },
                        },
                    };
                    unsafe {
                        self.0.raw.CreateShaderResourceView(
                            b.buffer.0.raw.as_ref().unwrap(),
                            Some(&desc),
                            Some(&mut srv),
                        )
                    }
                    .map_err(device_error)?;
                    Binding::Storage {
                        buffer: b.buffer.clone(),
                        offset: b.offset,
                        size,
                        srv: srv.unwrap(),
                    }
                }
                wgt::BindingType::Texture { .. } => {
                    Binding::Texture(d.textures[i].view.srv.clone())
                }
                wgt::BindingType::Sampler(_) => Binding::Sampler(d.samplers[i].0.clone()),
                _ => return unsupported(),
            };
            out.push((e.binding, layout.visibility, binding));
        }
        Ok(BindGroup { entries: out.into() })
    }
}
impl BindGroup {
    pub unsafe fn bind(
        &self,
        c: &ID3D11DeviceContext,
        l: &PipelineLayout,
        index: u32,
        offsets: &[u32],
    ) -> Result<(), crate::DeviceError> {
        let c1: ID3D11DeviceContext1 = c.cast().map_err(device_error)?;
        let mut dynamic = 0;
        for (binding, visibility, value) in self.entries.iter() {
            let slot = l.map[&naga::ResourceBinding {
                group: index,
                binding: *binding,
            }]
                .register;
            unsafe {
                match value {
                    Binding::Uniform {
                        buffer,
                        offset,
                        size,
                        dynamic: has_dynamic,
                    } => {
                        let start = *offset
                            + if *has_dynamic {
                                let o = offsets[dynamic] as u64;
                                dynamic += 1;
                                o
                            } else {
                                0
                            };
                        let first = (start / 16) as u32;
                        let count = size.div_ceil(256) as u32 * 16;
                        if visibility.contains(wgt::ShaderStages::VERTEX) {
                            c1.VSSetConstantBuffers1(
                                slot,
                                1,
                                Some(&buffer.0.raw),
                                Some(&first),
                                Some(&count),
                            );
                        }
                        if visibility.contains(wgt::ShaderStages::FRAGMENT) {
                            c1.PSSetConstantBuffers1(
                                slot,
                                1,
                                Some(&buffer.0.raw),
                                Some(&first),
                                Some(&count),
                            );
                        }
                    }
                    Binding::Storage { srv, .. } => {
                        if visibility.contains(wgt::ShaderStages::VERTEX) {
                            c.VSSetShaderResources(slot, Some(&[Some(srv.clone())]));
                        }
                        if visibility.contains(wgt::ShaderStages::FRAGMENT) {
                            c.PSSetShaderResources(slot, Some(&[Some(srv.clone())]));
                        }
                    }
                    Binding::Texture(srv) => {
                        if visibility.contains(wgt::ShaderStages::VERTEX) {
                            c.VSSetShaderResources(slot, Some(&[srv.clone()]));
                        }
                        if visibility.contains(wgt::ShaderStages::FRAGMENT) {
                            c.PSSetShaderResources(slot, Some(&[srv.clone()]));
                        }
                    }
                    Binding::Sampler(s) => {
                        if visibility.contains(wgt::ShaderStages::VERTEX) {
                            c.VSSetSamplers(slot, Some(&[Some(s.clone())]));
                        }
                        if visibility.contains(wgt::ShaderStages::FRAGMENT) {
                            c.PSSetSamplers(slot, Some(&[Some(s.clone())]));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn compile(
    stage: &crate::ProgrammableStage<ShaderModule>,
    kind: naga::ShaderStage,
    l: &PipelineLayout,
    special: bool,
) -> Result<Vec<u8>, crate::PipelineError> {
    let bits = if kind == naga::ShaderStage::Vertex {
        wgt::ShaderStages::VERTEX
    } else {
        wgt::ShaderStages::FRAGMENT
    };
    let fail = |e: String| crate::PipelineError::Linkage(bits, e);
    let n = &stage.module.0;
    let (module, info) = naga::back::pipeline_constants::process_overrides(
        &n.module,
        &n.info,
        Some((kind, stage.entry_point)),
        stage.constants,
    )
    .map_err(|e| fail(format!("{e}")))?;
    let options = naga::back::hlsl::Options {
        shader_model: naga::back::hlsl::ShaderModel::V5_0,
        binding_map: (*l.map).clone(),
        special_constants_binding: if special {
            Some(naga::back::hlsl::BindTarget {
                register: 13,
                ..Default::default()
            })
        } else {
            None
        },
        fake_missing_bindings: false,
        restrict_indexing: true,
        force_loop_bounding: true,
        ..Default::default()
    };
    let pipeline = naga::back::hlsl::PipelineOptions {
        entry_point: Some((kind, stage.entry_point.into())),
    };
    let mut source = String::new();
    let reflection = naga::back::hlsl::Writer::new(&mut source, &options, &pipeline)
        .write(&module, &info, None)
        .map_err(|e| fail(format!("{e}")))?;
    let entry = reflection
        .entry_point_names
        .first()
        .ok_or_else(|| fail("missing entry point".into()))?
        .as_ref()
        .map_err(|e| fail(format!("{e:?}")))?;
    let entry = std::ffi::CString::new(entry.as_bytes()).unwrap();
    let profile = if kind == naga::ShaderStage::Vertex {
        windows::core::s!("vs_5_0")
    } else {
        windows::core::s!("ps_5_0")
    };
    let (mut blob, mut errors) = (None, None);
    unsafe {
        if let Err(e) = Fxc::D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            profile,
            Fxc::D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut blob,
            Some(&mut errors),
        ) {
            let text = errors
                .as_ref()
                .map(|b| {
                    String::from_utf8_lossy(core::slice::from_raw_parts(
                        b.GetBufferPointer().cast(),
                        b.GetBufferSize(),
                    ))
                    .into_owned()
                })
                .unwrap_or_else(|| format!("{e}"));
            return Err(fail(text));
        }
        let blob = blob.unwrap();
        Ok(
            core::slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize())
                .to_vec(),
        )
    }
}
fn vertex_format(f: wgt::VertexFormat) -> Option<DXGI_FORMAT> {
    use wgt::VertexFormat::*;
    Some(match f {
        Float32 => DXGI_FORMAT_R32_FLOAT,
        Float32x2 => DXGI_FORMAT_R32G32_FLOAT,
        Float32x3 => DXGI_FORMAT_R32G32B32_FLOAT,
        Float32x4 => DXGI_FORMAT_R32G32B32A32_FLOAT,
        Uint32 => DXGI_FORMAT_R32_UINT,
        Uint32x2 => DXGI_FORMAT_R32G32_UINT,
        Uint32x3 => DXGI_FORMAT_R32G32B32_UINT,
        Uint32x4 => DXGI_FORMAT_R32G32B32A32_UINT,
        Sint32 => DXGI_FORMAT_R32_SINT,
        Sint32x2 => DXGI_FORMAT_R32G32_SINT,
        Sint32x3 => DXGI_FORMAT_R32G32B32_SINT,
        Sint32x4 => DXGI_FORMAT_R32G32B32A32_SINT,
        Unorm8x4 => DXGI_FORMAT_R8G8B8A8_UNORM,
        _ => return None,
    })
}
fn factor(f: wgt::BlendFactor) -> D3D11_BLEND {
    use wgt::BlendFactor::*;
    match f {
        Zero => D3D11_BLEND_ZERO,
        One => D3D11_BLEND_ONE,
        Src => D3D11_BLEND_SRC_COLOR,
        OneMinusSrc => D3D11_BLEND_INV_SRC_COLOR,
        SrcAlpha => D3D11_BLEND_SRC_ALPHA,
        OneMinusSrcAlpha => D3D11_BLEND_INV_SRC_ALPHA,
        Dst => D3D11_BLEND_DEST_COLOR,
        OneMinusDst => D3D11_BLEND_INV_DEST_COLOR,
        DstAlpha => D3D11_BLEND_DEST_ALPHA,
        OneMinusDstAlpha => D3D11_BLEND_INV_DEST_ALPHA,
        SrcAlphaSaturated => D3D11_BLEND_SRC_ALPHA_SAT,
        Constant => D3D11_BLEND_BLEND_FACTOR,
        OneMinusConstant => D3D11_BLEND_INV_BLEND_FACTOR,
        _ => D3D11_BLEND_ZERO,
    }
}
fn blend_op(op: wgt::BlendOperation) -> D3D11_BLEND_OP {
    use wgt::BlendOperation::*;
    match op {
        Add => D3D11_BLEND_OP_ADD,
        Subtract => D3D11_BLEND_OP_SUBTRACT,
        ReverseSubtract => D3D11_BLEND_OP_REV_SUBTRACT,
        Min => D3D11_BLEND_OP_MIN,
        Max => D3D11_BLEND_OP_MAX,
    }
}
impl HalDevice {
    pub fn pipeline(
        &self,
        d: &crate::RenderPipelineDescriptor<PipelineLayout, ShaderModule, Unsupported>,
    ) -> Result<Pipeline, crate::PipelineError> {
        let fail = || {
            crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX_FRAGMENT,
                "unsupported native DX11 pipeline state".into(),
            )
        };
        if d.depth_stencil.is_some()
            || d.multisample.count != 1
            || d.multiview_mask.is_some()
            || d.primitive.polygon_mode != wgt::PolygonMode::Fill
            || d.primitive.unclipped_depth
            || d.primitive.conservative
        {
            return Err(fail());
        }
        let (buffers, vertex) = match &d.vertex_processor {
            crate::VertexProcessor::Standard {
                vertex_buffers,
                vertex_stage,
            } => (*vertex_buffers, vertex_stage),
            _ => return Err(fail()),
        };
        let uses_special = vertex
            .module
            .0
            .module
            .entry_points
            .iter()
            .find(|e| e.stage == naga::ShaderStage::Vertex && e.name == vertex.entry_point)
            .is_some_and(|e| {
                e.function
                    .arguments
                    .iter()
                    .any(|a| uses_draw_builtin(&vertex.module.0.module, a.ty, a.binding.as_ref()))
            });
        let vs_code = compile(vertex, naga::ShaderStage::Vertex, d.layout, uses_special)?;
        let mut vs = None;
        unsafe { self.0.raw.CreateVertexShader(&vs_code, None, Some(&mut vs)) }
            .map_err(device_error)?;
        let mut ps = None;
        if let Some(fragment) = &d.fragment_stage {
            let code = compile(fragment, naga::ShaderStage::Fragment, d.layout, false)?;
            unsafe { self.0.raw.CreatePixelShader(&code, None, Some(&mut ps)) }
                .map_err(device_error)?;
        }
        let mut elems = Vec::new();
        let mut strides = Vec::new();
        for (slot, b) in buffers.iter().enumerate() {
            if let Some(b) = b {
                strides.push(b.array_stride as u32);
                for a in b.attributes {
                    elems.push(D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: windows::core::s!("LOC"),
                        SemanticIndex: a.shader_location,
                        Format: vertex_format(a.format).ok_or_else(fail)?,
                        InputSlot: slot as u32,
                        AlignedByteOffset: a.offset as u32,
                        InputSlotClass: if b.step_mode == wgt::VertexStepMode::Instance {
                            D3D11_INPUT_PER_INSTANCE_DATA
                        } else {
                            D3D11_INPUT_PER_VERTEX_DATA
                        },
                        InstanceDataStepRate: u32::from(
                            b.step_mode == wgt::VertexStepMode::Instance,
                        ),
                    });
                }
            } else {
                strides.push(0)
            }
        }
        let mut input = None;
        if !elems.is_empty() {
            unsafe {
                self.0
                    .raw
                    .CreateInputLayout(&elems, &vs_code, Some(&mut input))
            }
            .map_err(device_error)?;
        }
        let mut raster = None;
        unsafe {
            self.0.raw.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: match d.primitive.cull_mode {
                        Some(wgt::Face::Front) => D3D11_CULL_FRONT,
                        Some(wgt::Face::Back) => D3D11_CULL_BACK,
                        None => D3D11_CULL_NONE,
                    },
                    FrontCounterClockwise: (d.primitive.front_face == wgt::FrontFace::Ccw).into(),
                    DepthClipEnable: true.into(),
                    ScissorEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut raster),
            )
        }
        .map_err(device_error)?;
        let mut bd = D3D11_BLEND_DESC {
            IndependentBlendEnable: true.into(),
            ..Default::default()
        };
        for (i, t) in d.color_targets.iter().enumerate() {
            if let Some(t) = t {
                let mut r = D3D11_RENDER_TARGET_BLEND_DESC {
                    RenderTargetWriteMask: t.write_mask.bits() as u8,
                    SrcBlend: D3D11_BLEND_ONE,
                    DestBlend: D3D11_BLEND_ZERO,
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D11_BLEND_ONE,
                    DestBlendAlpha: D3D11_BLEND_ZERO,
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    ..Default::default()
                };
                if let Some(b) = t.blend {
                    r.BlendEnable = true.into();
                    r.SrcBlend = factor(b.color.src_factor);
                    r.DestBlend = factor(b.color.dst_factor);
                    r.BlendOp = blend_op(b.color.operation);
                    r.SrcBlendAlpha = factor(b.alpha.src_factor);
                    r.DestBlendAlpha = factor(b.alpha.dst_factor);
                    r.BlendOpAlpha = blend_op(b.alpha.operation);
                }
                bd.RenderTarget[i] = r;
            }
        }
        let mut blend = None;
        unsafe { self.0.raw.CreateBlendState(&bd, Some(&mut blend)) }.map_err(device_error)?;
        let mut depth = None;
        unsafe {
            self.0.raw.CreateDepthStencilState(
                &D3D11_DEPTH_STENCIL_DESC {
                    DepthEnable: false.into(),
                    DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
                    DepthFunc: D3D11_COMPARISON_ALWAYS,
                    StencilReadMask: 255,
                    StencilWriteMask: 255,
                    ..Default::default()
                },
                Some(&mut depth),
            )
        }
        .map_err(device_error)?;
        let topology = match d.primitive.topology {
            wgt::PrimitiveTopology::TriangleList => D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            wgt::PrimitiveTopology::TriangleStrip => D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP,
            wgt::PrimitiveTopology::LineList => D3D_PRIMITIVE_TOPOLOGY_LINELIST,
            wgt::PrimitiveTopology::LineStrip => D3D_PRIMITIVE_TOPOLOGY_LINESTRIP,
            wgt::PrimitiveTopology::PointList => D3D_PRIMITIVE_TOPOLOGY_POINTLIST,
        };
        let mut special = None;
        if uses_special {
            unsafe {
                self.0.raw.CreateBuffer(
                    &D3D11_BUFFER_DESC {
                        ByteWidth: 16,
                        Usage: D3D11_USAGE_DYNAMIC,
                        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                        ..Default::default()
                    },
                    None,
                    Some(&mut special),
                )
            }
            .map_err(device_error)?;
        }
        Ok(Pipeline {
            special: special.map(|buffer| Arc::new((buffer, Mutex::new(None)))),
            vs: vs.unwrap(),
            ps,
            input,
            raster: raster.unwrap(),
            blend: blend.unwrap(),
            depth: depth.unwrap(),
            strides: strides.into(),
            topology,
            sample_mask: d.multisample.mask as u32,
        })
    }
}

fn uses_draw_builtin(
    m: &naga::Module,
    ty: naga::Handle<naga::Type>,
    binding: Option<&naga::Binding>,
) -> bool {
    match binding {
        Some(naga::Binding::BuiltIn(naga::BuiltIn::VertexIndex | naga::BuiltIn::InstanceIndex)) => {
            true
        }
        _ => match &m.types[ty].inner {
            naga::TypeInner::Struct { members, .. } => members
                .iter()
                .any(|v| uses_draw_builtin(m, v.ty, v.binding.as_ref())),
            _ => false,
        },
    }
}
