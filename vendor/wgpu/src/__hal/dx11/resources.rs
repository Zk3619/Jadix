//! Small native offscreen rendering path. All COM objects retain their device;
//! public operations validate ownership and serialize the immediate context.

use super::{compile_shader_with_uniforms, Device, Error};
use alloc::vec::Vec;
use windows::Win32::{
    Foundation::RECT,
    Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        Direct3D11::*,
        Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC},
    },
};

#[derive(Debug)]
pub struct RenderTarget {
    owner: ID3D11Device,
    texture: ID3D11Texture2D,
    view: ID3D11RenderTargetView,
    size: [u32; 2],
}

#[derive(Debug)]
pub struct UniformBuffer {
    owner: ID3D11Device,
    raw: ID3D11Buffer,
    size: usize,
}

#[derive(Debug)]
pub struct RenderPipeline {
    owner: ID3D11Device,
    vertex: ID3D11VertexShader,
    fragment: ID3D11PixelShader,
    rasterizer: ID3D11RasterizerState,
    uniforms: Vec<(u32, u32)>,
}

impl Device {
    fn check_owner(&self, owner: &ID3D11Device) -> Result<(), Error> {
        if &self.raw == owner {
            Ok(())
        } else {
            Err(Error::Unsupported("resource belongs to another device"))
        }
    }

    /// Single-mip, single-sample RGBA8Unorm color target.
    pub fn create_render_target(&self, size: [u32; 2]) -> Result<RenderTarget, Error> {
        if size.into_iter().any(|n| n == 0 || n > 16384) {
            return Err(Error::Unsupported("invalid render target size"));
        }
        let desc = texture_desc(size);
        let (mut texture, mut view) = (None, None);
        unsafe {
            self.raw.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.ok_or(Error::Unsupported("missing texture"))?;
            self.raw
                .CreateRenderTargetView(&texture, None, Some(&mut view))?;
            let view = view.ok_or(Error::Unsupported("missing render target view"))?;
            self.context.lock().ClearRenderTargetView(&view, &[0.; 4]);
            Ok(RenderTarget {
                owner: self.raw.clone(),
                texture,
                view,
                size,
            })
        }
    }

    /// Constant buffers are fully initialized, 16-byte aligned and at most 64 KiB.
    pub fn create_uniform_buffer(&self, bytes: &[u8]) -> Result<UniformBuffer, Error> {
        if bytes.is_empty() || bytes.len() > 65536 || bytes.len() % 16 != 0 {
            return Err(Error::Unsupported(
                "uniform size must be a multiple of 16 in 16..=65536",
            ));
        }
        let mut raw = None;
        unsafe {
            self.raw.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: bytes.len() as u32,
                    Usage: D3D11_USAGE_DYNAMIC,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: bytes.as_ptr().cast(),
                    ..Default::default()
                }),
                Some(&mut raw),
            )?;
        }
        Ok(UniformBuffer {
            owner: self.raw.clone(),
            raw: raw.ok_or(Error::Unsupported("missing constant buffer"))?,
            size: bytes.len(),
        })
    }

    /// Rename the complete buffer so prior GPU draws retain their old contents.
    pub fn write_uniform_buffer(&self, buffer: &UniformBuffer, bytes: &[u8]) -> Result<(), Error> {
        self.check_owner(&buffer.owner)?;
        if bytes.len() != buffer.size {
            return Err(Error::Unsupported(
                "uniform update must cover the complete buffer",
            ));
        }
        let context = self.context.lock();
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(
                &buffer.raw,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut mapped),
            )?;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.pData.cast(), bytes.len());
            context.Unmap(&buffer.raw, 0);
        }
        Ok(())
    }

    /// Procedural triangle-list pipeline: vertex inputs must use builtins.
    /// Texture/storage bindings and vertex buffers are not supported by this path.
    pub fn create_render_pipeline(
        &self,
        module: &crate::__naga::Module,
        info: &crate::__naga::valid::ModuleInfo,
        vertex_entry: &str,
        fragment_entry: &str,
        uniforms: &[(crate::__naga::ResourceBinding, u32)],
    ) -> Result<RenderPipeline, Error> {
        fn has_location(
            module: &crate::__naga::Module,
            ty: crate::__naga::Handle<crate::__naga::Type>,
            binding: &Option<crate::__naga::Binding>,
        ) -> bool {
            match binding {
                Some(crate::__naga::Binding::Location { .. }) => true,
                Some(_) => false,
                None => match &module.types[ty].inner {
                    crate::__naga::TypeInner::Struct { members, .. } => members
                        .iter()
                        .any(|m| has_location(module, m.ty, &m.binding)),
                    _ => true,
                },
            }
        }
        if module
            .entry_points
            .iter()
            .filter(|ep| ep.stage == crate::__naga::ShaderStage::Vertex && ep.name == vertex_entry)
            .any(|ep| {
                ep.function
                    .arguments
                    .iter()
                    .any(|a| has_location(module, a.ty, &a.binding))
            })
        {
            return Err(Error::Unsupported(
                "vertex-buffer inputs are not implemented",
            ));
        }
        fn color_output(
            module: &crate::__naga::Module,
            ty: crate::__naga::Handle<crate::__naga::Type>,
            binding: &Option<crate::__naga::Binding>,
        ) -> bool {
            match binding {
                Some(crate::__naga::Binding::Location { location: 0, .. }) => true,
                Some(_) => false,
                None => match &module.types[ty].inner {
                    crate::__naga::TypeInner::Struct { members, .. } => {
                        members.len() == 1
                            && color_output(module, members[0].ty, &members[0].binding)
                    }
                    _ => false,
                },
            }
        }
        if module
            .entry_points
            .iter()
            .filter(|ep| {
                ep.stage == crate::__naga::ShaderStage::Fragment && ep.name == fragment_entry
            })
            .any(|ep| {
                ep.function
                    .result
                    .as_ref()
                    .is_none_or(|r| !color_output(module, r.ty, &r.binding))
            })
        {
            return Err(Error::Unsupported(
                "only one color output at location zero is supported",
            ));
        }
        let vs = compile_shader_with_uniforms(
            module,
            info,
            crate::__naga::ShaderStage::Vertex,
            vertex_entry,
            uniforms,
        )?;
        let ps = compile_shader_with_uniforms(
            module,
            info,
            crate::__naga::ShaderStage::Fragment,
            fragment_entry,
            uniforms,
        )?;
        let mut layouter = crate::__naga::proc::Layouter::default();
        layouter
            .update(module.to_ctx())
            .map_err(|e| Error::Shader(alloc::format!("{e}")))?;
        let sizes = uniforms
            .iter()
            .map(|&(binding, slot)| {
                let ty = module
                    .global_variables
                    .iter()
                    .find(|(_, g)| g.binding == Some(binding))
                    .unwrap()
                    .1
                    .ty;
                (slot, layouter[ty].size)
            })
            .collect();
        let (mut vertex, mut fragment, mut rasterizer) = (None, None, None);
        unsafe {
            self.raw.CreateVertexShader(&vs, None, Some(&mut vertex))?;
            self.raw.CreatePixelShader(&ps, None, Some(&mut fragment))?;
            self.raw.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    ScissorEnable: true.into(),
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut rasterizer),
            )?;
        }
        Ok(RenderPipeline {
            owner: self.raw.clone(),
            vertex: vertex.ok_or(Error::Unsupported("missing vertex shader"))?,
            fragment: fragment.ok_or(Error::Unsupported("missing pixel shader"))?,
            rasterizer: rasterizer.ok_or(Error::Unsupported("missing rasterizer"))?,
            uniforms: sizes,
        })
    }

    /// Submit one procedural draw. Scissor is `[x, y, width, height]`; clear is
    /// whole-target, as required by D3D11. Validate everything before any GPU work.
    pub fn draw(
        &self,
        target: &RenderTarget,
        pipeline: &RenderPipeline,
        uniforms: &[(u32, &UniformBuffer)],
        scissor: [u32; 4],
        vertices: core::ops::Range<u32>,
        clear: Option<[f32; 4]>,
    ) -> Result<(), Error> {
        self.check_owner(&target.owner)?;
        self.check_owner(&pipeline.owner)?;
        if vertices.end < vertices.start
            || uniforms.len() != pipeline.uniforms.len()
            || scissor[0]
                .checked_add(scissor[2])
                .is_none_or(|n| n > target.size[0])
            || scissor[1]
                .checked_add(scissor[3])
                .is_none_or(|n| n > target.size[1])
            || clear.is_some_and(|c| c.iter().any(|n| !n.is_finite()))
        {
            return Err(Error::Unsupported(
                "invalid draw range, bindings, scissor or clear",
            ));
        }
        let mut seen = 0u32;
        for &(slot, buffer) in uniforms {
            self.check_owner(&buffer.owner)?;
            if slot >= 14
                || seen & (1 << slot) != 0
                || !pipeline
                    .uniforms
                    .iter()
                    .any(|&(s, size)| s == slot && buffer.size >= size as usize)
            {
                return Err(Error::Unsupported(
                    "missing, duplicate or undersized uniform binding",
                ));
            }
            seen |= 1 << slot;
        }
        let context = self.context.lock();
        unsafe {
            context.ClearState();
            context.RSSetState(&pipeline.rasterizer);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: target.size[0] as f32,
                Height: target.size[1] as f32,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            context.RSSetScissorRects(Some(&[RECT {
                left: scissor[0] as i32,
                top: scissor[1] as i32,
                right: (scissor[0] + scissor[2]) as i32,
                bottom: (scissor[1] + scissor[3]) as i32,
            }]));
            context.OMSetRenderTargets(Some(&[Some(target.view.clone())]), None);
            if let Some(color) = clear {
                context.ClearRenderTargetView(&target.view, &color);
            }
            context.VSSetShader(&pipeline.vertex, None);
            context.PSSetShader(&pipeline.fragment, None);
            for &(slot, buffer) in uniforms {
                context.VSSetConstantBuffers(slot, Some(&[Some(buffer.raw.clone())]));
                context.PSSetConstantBuffers(slot, Some(&[Some(buffer.raw.clone())]));
            }
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.Draw(vertices.end - vertices.start, vertices.start);
            // Release bound resources after submit; queued GPU work retains them.
            context.ClearState();
        }
        Ok(())
    }

    /// Copy a retained pixel region between distinct targets. Overlapping copies
    /// within one resource require a separate scratch target and are rejected.
    pub fn copy_render_target_region(
        &self,
        source: &RenderTarget,
        target: &RenderTarget,
        origin: [u32; 2],
        size: [u32; 2],
        destination: [u32; 2],
    ) -> Result<(), Error> {
        self.check_owner(&source.owner)?;
        self.check_owner(&target.owner)?;
        if source.texture == target.texture
            || (0..2).any(|axis| {
                size[axis] == 0
                    || origin[axis]
                        .checked_add(size[axis])
                        .is_none_or(|n| n > source.size[axis])
                    || destination[axis]
                        .checked_add(size[axis])
                        .is_none_or(|n| n > target.size[axis])
            })
        {
            return Err(Error::Unsupported(
                "invalid texture copy region or same-resource copy",
            ));
        }
        unsafe {
            self.context.lock().CopySubresourceRegion(
                &target.texture,
                0,
                destination[0],
                destination[1],
                0,
                &source.texture,
                0,
                Some(&D3D11_BOX {
                    left: origin[0],
                    top: origin[1],
                    front: 0,
                    right: origin[0] + size[0],
                    bottom: origin[1] + size[1],
                    back: 1,
                }),
            );
        }
        Ok(())
    }

    /// Blocking staging readback for correctness diagnostics. Map waits for the
    /// preceding copy; rows are copied using the driver pitch, never assumed tight.
    pub fn read_render_target(&self, target: &RenderTarget) -> Result<Vec<u8>, Error> {
        self.check_owner(&target.owner)?;
        let mut staging = None;
        unsafe {
            self.raw.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..texture_desc(target.size)
                },
                None,
                Some(&mut staging),
            )?;
            let staging = staging.ok_or(Error::Unsupported("missing staging texture"))?;
            // Allocate before mapping so allocation cannot leave a resource mapped.
            let row_bytes = target.size[0] as usize * 4;
            let mut pixels = Vec::with_capacity(row_bytes * target.size[1] as usize);
            let context = self.context.lock();
            context.CopyResource(&staging, &target.texture);
            let mut mapping = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapping))?;
            for y in 0..target.size[1] as usize {
                pixels.extend_from_slice(core::slice::from_raw_parts(
                    mapping
                        .pData
                        .cast::<u8>()
                        .add(y * mapping.RowPitch as usize),
                    row_bytes,
                ));
            }
            context.Unmap(&staging, 0);
            Ok(pixels)
        }
    }
}

fn texture_desc(size: [u32; 2]) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: size[0],
        Height: size[1],
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    }
}
