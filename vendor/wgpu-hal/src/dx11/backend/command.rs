//! Commands retain their resources until wgpu releases the command buffer after the fence.
#![allow(unused_variables, unused_unsafe)]
use super::*;
use core::ops::Range;
type DeviceResult<T> = Result<T, crate::DeviceError>;
type Command = Box<dyn Fn(&ID3D11DeviceContext) -> DeviceResult<()> + Send + Sync>;
pub struct CommandBuffer {
    pub commands: Vec<Command>,
    device: HalDevice,
    pipeline: Option<Pipeline>,
    srv_count: u32,
    vertices: alloc::collections::BTreeMap<u32, (Buffer, u32)>,
}
impl core::fmt::Debug for CommandBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DX11CommandBuffer")
            .field("commands", &self.commands.len())
            .finish()
    }
}
impl CommandBuffer {
    pub fn new(device: HalDevice) -> Self {
        Self {
            commands: vec![],
            device,
            pipeline: None,
            srv_count: 0,
            vertices: Default::default(),
        }
    }
    fn push(
        &mut self,
        f: impl Fn(&ID3D11DeviceContext) -> DeviceResult<()> + Send + Sync + 'static,
    ) {
        self.commands.push(Box::new(f));
    }
    fn bind_vertex(&mut self, index: u32, b: Buffer, offset: u32) {
        let stride = self
            .pipeline
            .as_ref()
            .and_then(|p| p.strides.get(index as usize))
            .copied()
            .unwrap_or(0);
        self.push(move |c| {
            unsafe {
                c.IASetVertexBuffers(index, 1, Some(&b.0.raw), Some(&stride), Some(&offset));
            }
            Ok(())
        });
    }
}
unsafe fn unbind_srvs(c: &ID3D11DeviceContext, count: u32) {
    unsafe {
        if count != 0 {
            let empty: [Option<ID3D11ShaderResourceView>; 128] = [const { None }; 128];
            c.VSSetShaderResources(0, Some(&empty[..count as usize]));
            c.PSSetShaderResources(0, Some(&empty[..count as usize]));
        }
    }
}
unsafe fn upload(c: &ID3D11DeviceContext, b: &Buffer, range: Range<u64>) -> DeviceResult<()> {
    unsafe {
        if let Some(raw) = &b.0.raw {
            if b.0.uniform {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                c.Map(raw, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
                    .map_err(device_error)?;
                // Partial queue writes already update the shadow; discard needs all bytes.
                core::ptr::copy_nonoverlapping(
                    b.bytes().as_ptr(), mapped.pData.cast(), b.bytes().len(),
                );
                c.Unmap(raw, 0);
            } else if !range.is_empty() {
                let area = D3D11_BOX {
                    left: range.start as u32,
                    right: range.end as u32,
                    top: 0,
                    bottom: 1,
                    front: 0,
                    back: 1,
                };
                c.UpdateSubresource(
                    raw,
                    0,
                    Some(&area),
                    b.bytes().as_ptr().add(range.start as usize).cast(),
                    0,
                    0,
                );
            }
        }
    }
    Ok(())
}
fn sub(t: &Texture, b: &crate::TextureCopyBase) -> u32 {
    b.mip_level + b.array_layer * t.mips
}
fn texture_box(b: &crate::TextureCopyBase, s: crate::CopyExtent) -> D3D11_BOX {
    D3D11_BOX {
        left: b.origin.x,
        top: b.origin.y,
        front: 0,
        right: b.origin.x + s.width,
        bottom: b.origin.y + s.height,
        back: 1,
    }
}
impl crate::CommandEncoder for CommandBuffer {
    type A = Api;
    unsafe fn begin_encoding(&mut self, label: crate::Label) -> DeviceResult<()> {
        unsafe {
            self.commands.clear();
            self.pipeline = None;
            self.vertices.clear();
            self.srv_count = 0;
            Ok(())
        }
    }
    unsafe fn discard_encoding(&mut self) {
        unsafe {
            self.commands.clear();
            self.pipeline = None;
            self.vertices.clear();
            self.srv_count = 0;
        }
    }
    unsafe fn end_encoding(&mut self) -> DeviceResult<CommandBuffer> {
        unsafe {
            Ok(Self {
                commands: core::mem::take(&mut self.commands),
                device: self.device.clone(),
                pipeline: None,
                srv_count: 0,
                vertices: Default::default(),
            })
        }
    }
    unsafe fn reset_all<I>(&mut self, command_buffers: I) {
        unsafe {
            drop(command_buffers);
        }
    }
    unsafe fn transition_buffers<'a, T>(&mut self, barriers: T)
    where
        T: Iterator<Item = crate::BufferBarrier<'a, Buffer>>,
    {
        unsafe {}
    }
    unsafe fn transition_textures<'a, T>(&mut self, barriers: T)
    where
        T: Iterator<Item = crate::TextureBarrier<'a, Texture>>,
    {
        unsafe {}
    }
    unsafe fn clear_buffer(&mut self, buffer: &Buffer, range: crate::MemoryRange) {
        unsafe {
            let b = buffer.clone();
            self.push(move |c| {
                b.bytes()[range.start as usize..range.end as usize].fill(0);
                upload(c, &b, range.clone())?;
                Ok(())
            });
        }
    }
    unsafe fn copy_buffer_to_buffer<T>(&mut self, src: &Buffer, dst: &Buffer, regions: T)
    where
        T: Iterator<Item = crate::BufferCopy>,
    {
        unsafe {
            let (s, d) = (src.clone(), dst.clone());
            let regions: Vec<_> = regions.collect();
            self.push(move |c| {
                for r in &regions {
                    let n = r.size.get() as usize;
                    d.bytes()[r.dst_offset as usize..r.dst_offset as usize + n].copy_from_slice(
                        &s.bytes()[r.src_offset as usize..r.src_offset as usize + n],
                    );
                }
                if let (Some(start), Some(end)) = (
                    regions.iter().map(|r| r.dst_offset).min(),
                    regions.iter().map(|r| r.dst_offset + r.size.get()).max(),
                ) {
                    upload(c, &d, start..end)?;
                }
                Ok(())
            });
        }
    }
    unsafe fn copy_texture_to_texture<T: Iterator<Item = crate::TextureCopy>>(
        &mut self,
        src: &Texture,
        src_usage: wgt::TextureUses,
        dst: &Texture,
        regions: T,
    ) {
        unsafe {
            let (s, d) = (src.clone(), dst.clone());
            let regions: Vec<_> = regions.collect();
            self.push(move |c| {
                for r in &regions {
                    for layer in 0..r.size.depth {
                        let b = texture_box(&r.src_base, r.size);
                        c.CopySubresourceRegion(
                            &d.raw,
                            sub(&d, &r.dst_base) + layer * d.mips,
                            r.dst_base.origin.x,
                            r.dst_base.origin.y,
                            0,
                            &s.raw,
                            sub(&s, &r.src_base) + layer * s.mips,
                            Some(&b),
                        );
                    }
                }
                Ok(())
            });
        }
    }
    unsafe fn copy_buffer_to_texture<T: Iterator<Item = crate::BufferTextureCopy>>(
        &mut self,
        src: &Buffer,
        dst: &Texture,
        regions: T,
    ) {
        unsafe {
            let (s, d) = (src.clone(), dst.clone());
            let regions: Vec<_> = regions.collect();
            self.push(move |c| {
                for r in &regions {
                    let bpp = format(d.format).ok_or(crate::DeviceError::Unexpected)?.1;
                    let pitch = r.buffer_layout.bytes_per_row.unwrap_or(r.size.width * bpp);
                    let rows = r.buffer_layout.rows_per_image.unwrap_or(r.size.height);
                    for layer in 0..r.size.depth {
                        let start = (r.buffer_layout.offset
                            + layer as u64 * pitch as u64 * rows as u64)
                            as usize;
                        let b = texture_box(&r.texture_base, r.size);
                        c.UpdateSubresource(
                            &d.raw,
                            sub(&d, &r.texture_base) + layer * d.mips,
                            Some(&b),
                            s.bytes().as_ptr().add(start).cast(),
                            pitch,
                            pitch * rows,
                        );
                    }
                }
                Ok(())
            });
        }
    }
    unsafe fn copy_texture_to_buffer<T: Iterator<Item = crate::BufferTextureCopy>>(
        &mut self,
        src: &Texture,
        src_usage: wgt::TextureUses,
        dst: &Buffer,
        regions: T,
    ) {
        unsafe {
            let (s, d) = (src.clone(), dst.clone());
            let regions: Vec<_> = regions.collect();
            let device = self.device.clone();
            self.push(move |c| {
                for r in &regions {
                    let bpp = format(s.format).ok_or(crate::DeviceError::Unexpected)?.1;
                    let pitch = r.buffer_layout.bytes_per_row.unwrap_or(r.size.width * bpp);
                    let rows = r.buffer_layout.rows_per_image.unwrap_or(r.size.height);
                    let mut sd = D3D11_TEXTURE2D_DESC::default();
                    s.raw.GetDesc(&mut sd);
                    sd.Width = r.size.width;
                    sd.Height = r.size.height;
                    sd.MipLevels = 1;
                    sd.ArraySize = 1;
                    sd.Usage = D3D11_USAGE_STAGING;
                    sd.BindFlags = 0;
                    sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                    sd.MiscFlags = 0;
                    let mut staging = None;
                    device
                        .0
                        .raw
                        .CreateTexture2D(&sd, None, Some(&mut staging))
                        .map_err(device_error)?;
                    let staging = staging.unwrap();
                    for layer in 0..r.size.depth {
                        let b = texture_box(&r.texture_base, r.size);
                        c.CopySubresourceRegion(
                            &staging,
                            0,
                            0,
                            0,
                            0,
                            &s.raw,
                            sub(&s, &r.texture_base) + layer * s.mips,
                            Some(&b),
                        );
                        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                        c.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                            .map_err(device_error)?;
                        let row_bytes = (r.size.width * bpp) as usize;
                        for y in 0..r.size.height {
                            let out = (r.buffer_layout.offset
                                + layer as u64 * pitch as u64 * rows as u64
                                + y as u64 * pitch as u64)
                                as usize;
                            core::ptr::copy_nonoverlapping(
                                mapped
                                    .pData
                                    .cast::<u8>()
                                    .add(y as usize * mapped.RowPitch as usize),
                                d.bytes().as_mut_ptr().add(out),
                                row_bytes,
                            );
                        }
                        c.Unmap(&staging, 0);
                    }
                }
                Ok(())
            });
        }
    }
    unsafe fn begin_query(&mut self, set: &Unsupported, index: u32) {
        unsafe {
            panic!("DX11 operation is not advertised: begin_query");
        }
    }
    unsafe fn end_query(&mut self, set: &Unsupported, index: u32) {
        unsafe {
            panic!("DX11 operation is not advertised: end_query");
        }
    }
    unsafe fn write_timestamp(&mut self, set: &Unsupported, index: u32) {
        unsafe {
            panic!("DX11 operation is not advertised: write_timestamp");
        }
    }
    unsafe fn read_acceleration_structure_compact_size(
        &mut self,
        acceleration_structure: &Unsupported,
        buf: &Buffer,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: read_acceleration_structure_compact_size");
        }
    }
    unsafe fn reset_queries(&mut self, set: &Unsupported, range: Range<u32>) {
        unsafe {
            panic!("DX11 operation is not advertised: reset_queries");
        }
    }
    unsafe fn copy_query_results(
        &mut self,
        set: &Unsupported,
        range: Range<u32>,
        buffer: &Buffer,
        offset: wgt::BufferAddress,
        stride: wgt::BufferSize,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: copy_query_results");
        }
    }
    unsafe fn begin_render_pass(
        &mut self,
        desc: &crate::RenderPassDescriptor<Unsupported, TextureView>,
    ) -> DeviceResult<()> {
        unsafe {
            if desc.depth_stencil_attachment.is_some()
                || desc.sample_count != 1
                || desc.timestamp_writes.is_some()
            {
                return unsupported();
            }
            self.vertices.clear();
            self.srv_count = 0;
            self.pipeline = None;
            let rtvs: Vec<_> = desc
                .color_attachments
                .iter()
                .map(|a| a.as_ref().and_then(|a| a.target.view.rtv.clone()))
                .collect();
            let clears: Vec<_> = desc
                .color_attachments
                .iter()
                .filter_map(|a| a.as_ref())
                .filter(|a| a.ops.contains(crate::AttachmentOps::LOAD_CLEAR))
                .map(|a| {
                    (
                        a.target.view.rtv.clone().unwrap(),
                        [
                            a.clear_value.r as f32,
                            a.clear_value.g as f32,
                            a.clear_value.b as f32,
                            a.clear_value.a as f32,
                        ],
                    )
                })
                .collect();
            let extent = desc.extent;
            self.push(move |c| {
                c.OMSetRenderTargets(Some(&rtvs), None);
                for (r, color) in &clears {
                    c.ClearRenderTargetView(r, color);
                }
                c.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    TopLeftX: 0.,
                    TopLeftY: 0.,
                    Width: extent.width as f32,
                    Height: extent.height as f32,
                    MinDepth: 0.,
                    MaxDepth: 1.,
                }]));
                c.RSSetScissorRects(Some(&[RECT {
                    left: 0,
                    top: 0,
                    right: extent.width as i32,
                    bottom: extent.height as i32,
                }]));
                Ok(())
            });
            Ok(())
        }
    }
    unsafe fn end_render_pass(&mut self) {
        unsafe {
            let count = self.srv_count;
            self.srv_count = 0;
            self.push(move |c| {
                c.OMSetRenderTargets(None, None);
                unbind_srvs(c, count);
                Ok(())
            });
        }
    }
    unsafe fn set_bind_group(
        &mut self,
        layout: &PipelineLayout,
        index: u32,
        group: &BindGroup,
        dynamic_offsets: &[wgt::DynamicOffset],
    ) {
        unsafe {
            for (binding, _, value) in group.entries.iter() {
                if matches!(value, Binding::Storage { .. } | Binding::Texture(_)) {
                    let slot = layout.map[&naga::ResourceBinding {
                        group: index,
                        binding: *binding,
                    }]
                        .register;
                    self.srv_count = self.srv_count.max(slot + 1);
                }
            }
            let (l, g, o) = (layout.clone(), group.clone(), dynamic_offsets.to_vec());
            self.push(move |c| g.bind(c, &l, index, &o));
        }
    }
    unsafe fn set_immediates(&mut self, layout: &PipelineLayout, offset_bytes: u32, data: &[u32]) {
        unsafe {
            assert!(data.is_empty(), "DX11 immediates are not advertised");
        }
    }
    unsafe fn insert_debug_marker(&mut self, label: &str) {
        unsafe {}
    }
    unsafe fn begin_debug_marker(&mut self, group_label: &str) {
        unsafe {}
    }
    unsafe fn end_debug_marker(&mut self) {
        unsafe {}
    }
    unsafe fn set_render_pipeline(&mut self, pipeline: &Pipeline) {
        unsafe {
            let p = pipeline.clone();
            self.pipeline = Some(p.clone());
            self.push(move |c| {
                p.bind(c);
                Ok(())
            });
            let vertices: Vec<_> = self
                .vertices
                .iter()
                .map(|(&i, (b, o))| (i, b.clone(), *o))
                .collect();
            for (i, b, o) in vertices {
                self.bind_vertex(i, b, o);
            }
        }
    }
    unsafe fn set_index_buffer<'a>(
        &mut self,
        binding: crate::BufferBinding<'a, Buffer>,
        format: wgt::IndexFormat,
    ) {
        unsafe {
            let b = binding.buffer.clone();
            let offset = binding.offset as u32;
            self.push(move |c| {
                c.IASetIndexBuffer(
                    b.0.raw.as_ref(),
                    if format == wgt::IndexFormat::Uint16 {
                        DXGI_FORMAT_R16_UINT
                    } else {
                        DXGI_FORMAT_R32_UINT
                    },
                    offset,
                );
                Ok(())
            });
        }
    }
    unsafe fn set_vertex_buffer<'a>(
        &mut self,
        index: u32,
        binding: crate::BufferBinding<'a, Buffer>,
    ) {
        unsafe {
            let b = binding.buffer.clone();
            let offset = binding.offset as u32;
            self.vertices.insert(index, (b.clone(), offset));
            if self.pipeline.is_some() {
                self.bind_vertex(index, b, offset);
            }
        }
    }
    unsafe fn set_viewport(&mut self, rect: &crate::Rect<f32>, depth_range: Range<f32>) {
        unsafe {
            let r = rect.clone();
            self.push(move |c| {
                c.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    TopLeftX: r.x,
                    TopLeftY: r.y,
                    Width: r.w,
                    Height: r.h,
                    MinDepth: depth_range.start,
                    MaxDepth: depth_range.end,
                }]));
                Ok(())
            });
        }
    }
    unsafe fn set_scissor_rect(&mut self, rect: &crate::Rect<u32>) {
        unsafe {
            let r = rect.clone();
            self.push(move |c| {
                c.RSSetScissorRects(Some(&[RECT {
                    left: r.x as i32,
                    top: r.y as i32,
                    right: (r.x + r.w) as i32,
                    bottom: (r.y + r.h) as i32,
                }]));
                Ok(())
            });
        }
    }
    unsafe fn set_stencil_reference(&mut self, value: u32) {
        unsafe {
            assert_eq!(value, 0, "DX11 stencil is not advertised");
        }
    }
    unsafe fn set_blend_constants(&mut self, color: &[f32; 4]) {
        unsafe {
            let color = *color;
            let p = self
                .pipeline
                .clone()
                .expect("blend constant without pipeline");
            self.push(move |c| {
                c.OMSetBlendState(&p.blend, Some(&color), p.sample_mask);
                Ok(())
            });
        }
    }
    unsafe fn draw(
        &mut self,
        first_vertex: u32,
        vertex_count: u32,
        first_instance: u32,
        instance_count: u32,
    ) {
        unsafe {
            let p = self.pipeline.clone().expect("draw without pipeline");
            self.push(move |c| {
                p.draw_constants(c, first_vertex as i32, first_instance)?;
                c.DrawInstanced(vertex_count, instance_count, first_vertex, first_instance);
                Ok(())
            });
        }
    }
    unsafe fn draw_indexed(
        &mut self,
        first_index: u32,
        index_count: u32,
        base_vertex: i32,
        first_instance: u32,
        instance_count: u32,
    ) {
        unsafe {
            let p = self.pipeline.clone().expect("draw without pipeline");
            self.push(move |c| {
                p.draw_constants(c, base_vertex, first_instance)?;
                c.DrawIndexedInstanced(
                    index_count,
                    instance_count,
                    first_index,
                    base_vertex,
                    first_instance,
                );
                Ok(())
            });
        }
    }
    unsafe fn draw_mesh_tasks(
        &mut self,
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_mesh_tasks");
        }
    }
    unsafe fn draw_indirect(
        &mut self,
        buffer: &Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_indirect");
        }
    }
    unsafe fn draw_indexed_indirect(
        &mut self,
        buffer: &Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_indexed_indirect");
        }
    }
    unsafe fn draw_mesh_tasks_indirect(
        &mut self,
        buffer: &<Self::A as crate::Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_mesh_tasks_indirect");
        }
    }
    unsafe fn draw_indirect_count(
        &mut self,
        buffer: &Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_indirect_count");
        }
    }
    unsafe fn draw_indexed_indirect_count(
        &mut self,
        buffer: &Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_indexed_indirect_count");
        }
    }
    unsafe fn draw_mesh_tasks_indirect_count(
        &mut self,
        buffer: &<Self::A as crate::Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as crate::Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: draw_mesh_tasks_indirect_count");
        }
    }
    unsafe fn begin_compute_pass(&mut self, desc: &crate::ComputePassDescriptor<Unsupported>) {
        unsafe {
            panic!("DX11 operation is not advertised: begin_compute_pass");
        }
    }
    unsafe fn end_compute_pass(&mut self) {
        unsafe {
            panic!("DX11 operation is not advertised: end_compute_pass");
        }
    }
    unsafe fn set_compute_pipeline(&mut self, pipeline: &Unsupported) {
        unsafe {
            panic!("DX11 operation is not advertised: set_compute_pipeline");
        }
    }
    unsafe fn dispatch_workgroups(&mut self, count: [u32; 3]) {
        unsafe {
            panic!("DX11 operation is not advertised: dispatch_workgroups");
        }
    }
    unsafe fn dispatch_workgroups_indirect(&mut self, buffer: &Buffer, offset: wgt::BufferAddress) {
        unsafe {
            panic!("DX11 operation is not advertised: dispatch_workgroups_indirect");
        }
    }
    unsafe fn build_acceleration_structures<'a, T>(
        &mut self,
        _descriptor_count: u32,
        descriptors: T,
    ) where
        Api: 'a,
        T: IntoIterator<
            Item = crate::BuildAccelerationStructureDescriptor<'a, Buffer, Unsupported>,
        >,
    {
        unsafe {
            panic!("DX11 operation is not advertised: build_acceleration_structures");
        }
    }
    unsafe fn place_acceleration_structure_barrier(
        &mut self,
        _barriers: crate::AccelerationStructureBarrier,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: place_acceleration_structure_barrier");
        }
    }
    unsafe fn copy_acceleration_structure_to_acceleration_structure(
        &mut self,
        src: &Unsupported,
        dst: &Unsupported,
        copy: wgt::AccelerationStructureCopy,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: copy_acceleration_structure_to_acceleration_structure");
        }
    }
    unsafe fn set_acceleration_structure_dependencies(
        command_buffers: &[&CommandBuffer],
        dependencies: &[&Unsupported],
    ) {
        unsafe {
            assert!(
                dependencies.is_empty(),
                "DX11 ray tracing is not advertised"
            );
        }
    }
    unsafe fn begin_ray_tracing_pass(&mut self, _desc: &crate::RayTracingPassDescriptor) {
        unsafe {
            panic!("DX11 operation is not advertised: begin_ray_tracing_pass");
        }
    }
    unsafe fn end_ray_tracing_pass(&mut self) {
        unsafe {
            panic!("DX11 operation is not advertised: end_ray_tracing_pass");
        }
    }
    unsafe fn set_ray_tracing_pipeline(
        &mut self,
        _pipeline: &<Self::A as crate::Api>::RayTracingPipeline,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: set_ray_tracing_pipeline");
        }
    }
    unsafe fn trace_rays(
        &mut self,
        _count: [u32; 3],
        _ray_generation_group_data: crate::PipelineGroupData<Buffer>,
        _miss_group_data: crate::PipelineGroupData<Buffer>,
        _intersection_group_data: crate::PipelineGroupData<Buffer>,
    ) {
        unsafe {
            panic!("DX11 operation is not advertised: trace_rays");
        }
    }
}
