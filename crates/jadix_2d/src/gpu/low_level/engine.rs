use super::*;
use wgpu::util::DeviceExt;

#[derive(Clone)]
enum Resource {
    Buffer(BufferProxy, wgpu::Buffer),
    Image(ImageProxy, wgpu::Texture),
}
impl Resource {
    fn descriptor(&self) -> Descriptor {
        match self {
            Self::Buffer(b, _) => Descriptor::Buffer(*b),
            Self::Image(i, _) => Descriptor::Image(*i),
        }
    }
}
enum Pipeline {
    Compute(wgpu::ComputePipeline),
    Render(wgpu::RenderPipeline),
}
struct Shader {
    module: wgpu::ShaderModule,
    bindings: Vec<BindType>,
    entry: String,
    vertex: Option<String>,
    format: Option<ImageFormat>,
    layout: wgpu::BindGroupLayout,
    pipeline: Option<Pipeline>,
}
/// Engine and registered shaders are bound to the exact device supplied to `new`.
/// `record` returns an owned command buffer; `execute` records and submits it.
/// Invalid recordings submit nothing.
pub struct Engine {
    device: wgpu::Device,
    cache: Option<wgpu::PipelineCache>,
    limits: EngineLimits,
    resources: HashMap<ResourceId, Resource>,
    retired: std::collections::HashSet<ResourceId>,
    shaders: HashMap<ShaderId, Shader>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionStats {
    pub commands: usize,
    pub dispatches: u32,
    pub draws: u32,
    pub allocated_bytes: u64,
    pub live_bytes: u64,
    pub staging_bytes: u64,
    pub compiled_pipelines: u32,
}
pub struct Download {
    pub resource: ResourceId,
    pub buffer: wgpu::Buffer,
    pub size: u64,
    pub bytes_per_row: Option<u32>,
    pub image: Option<ImageProxy>,
}
/// A validated command buffer and its position-specific readbacks. Engine logical
/// resource state advances when recording succeeds. Submit packets in recording
/// order before using those resources elsewhere. If a packet is discarded, reset
/// affected resource state before reuse; uploads/clears in it have not happened.
/// Fields may be moved into a caller-owned multi-command-buffer submission.
pub struct RecordedExecution {
    pub commands: wgpu::CommandBuffer,
    pub downloads: Vec<Download>,
    pub stats: ExecutionStats,
}
impl RecordedExecution {
    /// The queue must belong to the same device as the recording engine.
    pub fn submit(self, queue: &wgpu::Queue) -> Execution {
        Execution {
            submission: queue.submit([self.commands]),
            downloads: self.downloads,
            stats: self.stats,
        }
    }
}
pub struct Execution {
    pub submission: wgpu::SubmissionIndex,
    pub downloads: Vec<Download>,
    pub stats: ExecutionStats,
}
impl Execution {
    pub fn take_download(&mut self, id: ResourceId) -> Option<Download> {
        let i = self.downloads.iter().position(|d| d.resource == id)?;
        Some(self.downloads.remove(i))
    }
}
impl Engine {
    pub fn new(
        device: &wgpu::Device,
        limits: EngineLimits,
        cache: Option<wgpu::PipelineCache>,
    ) -> Result<Self, Error> {
        let l = device.limits();
        if l.max_compute_invocations_per_workgroup == 0
            || l.max_compute_workgroups_per_dimension == 0
        {
            return Err(Error::UnsupportedDevice);
        }
        Ok(Self {
            device: device.clone(),
            cache,
            limits,
            resources: HashMap::new(),
            retired: Default::default(),
            shaders: HashMap::new(),
        })
    }
    /// Registers WGSL and an explicit, consecutive group-0 binding schema.
    /// Pipeline compilation remains lazy until a valid recording uses the shader.
    pub fn add_compute_shader(
        &mut self,
        label: &str,
        source: &str,
        entry: &str,
        bindings: &[BindType],
    ) -> Result<ShaderId, Error> {
        self.add_shader(label, source, entry, None, None, bindings)
    }
    /// Render shaders have no vertex buffers; use vertex_index and storage bindings.
    pub fn add_render_shader(
        &mut self,
        label: &str,
        source: &str,
        vertex: &str,
        fragment: &str,
        format: ImageFormat,
        bindings: &[BindType],
    ) -> Result<ShaderId, Error> {
        self.add_shader(
            label,
            source,
            fragment,
            Some(vertex),
            Some(format),
            bindings,
        )
    }
    fn add_shader(
        &mut self,
        label: &str,
        source: &str,
        entry: &str,
        vertex: Option<&str>,
        format: Option<ImageFormat>,
        bindings: &[BindType],
    ) -> Result<ShaderId, Error> {
        if self.shaders.len() >= self.limits.max_shaders {
            return Err(Error::Budget);
        }
        let visibility = if vertex.is_some() {
            wgpu::ShaderStages::VERTEX_FRAGMENT
        } else {
            wgpu::ShaderStages::COMPUTE
        };
        let entries = bindings
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupLayoutEntry {
                binding: i as u32,
                visibility,
                count: None,
                ty: match b {
                    BindType::Buffer | BindType::BufReadOnly | BindType::Uniform => {
                        wgpu::BindingType::Buffer {
                            ty: match b {
                                BindType::Uniform => wgpu::BufferBindingType::Uniform,
                                _ => wgpu::BufferBindingType::Storage {
                                    read_only: *b == BindType::BufReadOnly,
                                },
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        }
                    }
                    BindType::Image(f) => wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: f.to_wgpu(),
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    BindType::ImageRead(_) => wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                },
            })
            .collect::<Vec<_>>();
        let (module, layout) = jadix_renderer::wgpu_compat::try_gpu_init(
            &self.device,
            "recorded shader registration",
            || {
                (
                    self.device
                        .create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some(label),
                            source: wgpu::ShaderSource::Wgsl(source.into()),
                        }),
                    self.device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: Some(label),
                            entries: &entries,
                        }),
                )
            },
        )
        .ok_or(Error::GpuValidation)?;
        let id = ShaderId(ResourceId::next().0);
        self.shaders.insert(
            id,
            Shader {
                module,
                bindings: bindings.to_vec(),
                entry: entry.into(),
                vertex: vertex.map(str::to_owned),
                format,
                layout,
                pipeline: None,
            },
        );
        Ok(id)
    }
    pub fn buffer(&self, b: BufferProxy) -> Option<&wgpu::Buffer> {
        match self.resources.get(&b.id)? {
            Resource::Buffer(p, bf) if *p == b => Some(bf),
            _ => None,
        }
    }
    pub fn image(&self, i: ImageProxy) -> Option<&wgpu::Texture> {
        match self.resources.get(&i.id)? {
            Resource::Image(p, t) if *p == i => Some(t),
            _ => None,
        }
    }
    /// Adopt an existing same-device buffer without copying. Its physical size
    /// must equal the proxy's aligned size; use BufferRange for subranges.
    /// Each command checks the imported handle's actual usage before encoding.
    pub fn import_buffer(&mut self, proxy: BufferProxy, buffer: wgpu::Buffer) -> Result<(), Error> {
        if Descriptor::Buffer(proxy).bytes() != Some(buffer.size()) {
            return Err(Error::InvalidResource);
        }
        self.import(Resource::Buffer(proxy, buffer))
    }
    /// Adopt one same-device 2D mip/layer. No implicit color/alpha conversion;
    /// kernels see the texture's declared values. Exported clones keep ownership.
    pub fn import_image(&mut self, proxy: ImageProxy, texture: wgpu::Texture) -> Result<(), Error> {
        if texture.width() != proxy.width
            || texture.height() != proxy.height
            || texture.format() != proxy.format.to_wgpu()
            || texture.sample_count() != 1
            || texture.mip_level_count() != 1
            || texture.depth_or_array_layers() != 1
            || texture.dimension() != wgpu::TextureDimension::D2
        {
            return Err(Error::InvalidImage);
        }
        self.import(Resource::Image(proxy, texture))
    }
    fn import(&mut self, resource: Resource) -> Result<(), Error> {
        let descriptor = resource.descriptor();
        if self.resources.contains_key(&descriptor.id()) {
            return Err(Error::DescriptorConflict);
        }
        let mut state = self
            .resources
            .iter()
            .map(|(id, r)| (*id, r.descriptor()))
            .collect();
        self.descriptor(descriptor, &mut state, &self.retired)?;
        if self.resources.len() + self.retired.len() + 1 > self.limits.max_resources {
            return Err(Error::Budget);
        }
        self.resources.insert(descriptor.id(), resource);
        Ok(())
    }
    fn buffer_usage(&self, b: BufferProxy, usage: wgpu::BufferUsages) -> Result<(), Error> {
        if let Some(Resource::Buffer(_, buffer)) = self.resources.get(&b.id) {
            if !buffer.usage().contains(usage) {
                return Err(Error::InvalidBinding);
            }
        }
        Ok(())
    }
    fn image_usage(&self, i: ImageProxy, usage: wgpu::TextureUsages) -> Result<(), Error> {
        if let Some(Resource::Image(_, image)) = self.resources.get(&i.id) {
            if !image.usage().contains(usage) {
                return Err(Error::InvalidBinding);
            }
        }
        Ok(())
    }
    pub fn live_resources(&self) -> usize {
        self.resources.len()
    }
    pub fn live_bytes(&self) -> u64 {
        self.resources
            .values()
            .map(|r| r.descriptor().bytes().unwrap())
            .sum()
    }
    /// Releases engine ownership and proxy tombstones. Already submitted work and
    /// exported wgpu handles keep their resources alive. Old IDs may be used again
    /// after this explicit reset, so discard old recordings at the same boundary.
    pub fn reset_resources(&mut self) {
        self.resources.clear();
        self.retired.clear();
    }
    pub fn remove_shader(&mut self, id: ShaderId) -> bool {
        self.shaders.remove(&id).is_some()
    }
    fn descriptor(
        &self,
        d: Descriptor,
        state: &mut HashMap<ResourceId, Descriptor>,
        retired: &std::collections::HashSet<ResourceId>,
    ) -> Result<(), Error> {
        if retired.contains(&d.id()) {
            return Err(Error::ReleasedResource);
        }
        match d {
            Descriptor::Buffer(b) => {
                if b.size == 0
                    || d.bytes()
                        .is_none_or(|n| n > self.device.limits().max_buffer_size)
                {
                    return Err(Error::InvalidResource);
                }
            }
            Descriptor::Image(i) => {
                if i.width == 0
                    || i.height == 0
                    || i.width.max(i.height) > self.device.limits().max_texture_dimension_2d
                {
                    return Err(Error::InvalidImage);
                }
            }
        }
        if let Some(previous) = state.insert(d.id(), d) {
            if previous != d {
                return Err(Error::DescriptorConflict);
            }
        }
        if state.len() > self.limits.max_resources
            || state
                .values()
                .try_fold(0u64, |sum, d| sum.checked_add(d.bytes()?))
                .is_none_or(|n| n > self.limits.max_live_bytes)
        {
            return Err(Error::Budget);
        }
        Ok(())
    }
    fn bindings(
        &self,
        id: ShaderId,
        resources: &[ResourceProxy],
        render: bool,
        state: &mut HashMap<ResourceId, Descriptor>,
        retired: &std::collections::HashSet<ResourceId>,
    ) -> Result<(), Error> {
        let shader = self.shaders.get(&id).ok_or(Error::InvalidShader)?;
        if shader.vertex.is_some() != render || shader.bindings.len() != resources.len() {
            return Err(Error::InvalidBinding);
        }
        for (kind, r) in shader.bindings.iter().zip(resources) {
            match (*kind, *r) {
                (
                    BindType::Buffer | BindType::BufReadOnly | BindType::Uniform,
                    ResourceProxy::Buffer(b) | ResourceProxy::BufferRange { proxy: b, .. },
                ) => {
                    self.descriptor(Descriptor::Buffer(b), state, retired)?;
                    self.buffer_usage(
                        b,
                        if *kind == BindType::Uniform {
                            wgpu::BufferUsages::UNIFORM
                        } else {
                            wgpu::BufferUsages::STORAGE
                        },
                    )?;
                    let (offset, size) = match *r {
                        ResourceProxy::BufferRange { offset, size, .. } => (offset, size),
                        _ => (0, b.size),
                    };
                    let l = self.device.limits();
                    let uniform = *kind == BindType::Uniform;
                    let alignment = if uniform {
                        l.min_uniform_buffer_offset_alignment
                    } else {
                        l.min_storage_buffer_offset_alignment
                    } as u64;
                    let max = if uniform {
                        l.max_uniform_buffer_binding_size
                    } else {
                        l.max_storage_buffer_binding_size
                    };
                    if size == 0
                        || offset % alignment != 0
                        || size > max
                        || offset.checked_add(size).is_none_or(|end| end > b.size)
                    {
                        return Err(Error::InvalidRange);
                    }
                }
                (BindType::Image(f) | BindType::ImageRead(f), ResourceProxy::Image(i))
                    if f == i.format =>
                {
                    self.descriptor(Descriptor::Image(i), state, retired)?;
                    self.image_usage(
                        i,
                        if matches!(kind, BindType::Image(_)) {
                            wgpu::TextureUsages::STORAGE_BINDING
                        } else {
                            wgpu::TextureUsages::TEXTURE_BINDING
                        },
                    )?;
                }
                _ => return Err(Error::InvalidBinding),
            }
        }
        Ok(())
    }
    fn preflight(&self, recording: &Recording) -> Result<ExecutionStats, Error> {
        if recording.commands.len() > self.limits.max_commands {
            return Err(Error::Budget);
        }
        let mut state = self
            .resources
            .iter()
            .map(|(id, r)| (*id, r.descriptor()))
            .collect::<HashMap<_, _>>();
        let mut retired = self.retired.clone();
        let mut seen = state
            .keys()
            .copied()
            .chain(retired.iter().copied())
            .collect::<std::collections::HashSet<_>>();
        let initial_bytes = self.live_bytes();
        let mut stats = ExecutionStats {
            commands: recording.commands.len(),
            ..Default::default()
        };
        for cmd in &recording.commands {
            match cmd {
                Command::Upload(b, d) | Command::UploadUniform(b, d) => {
                    self.buffer_usage(*b, wgpu::BufferUsages::COPY_DST)?;
                    self.descriptor(Descriptor::Buffer(*b), &mut state, &retired)?;
                    if d.len() as u64 > b.size || (d.len() % 4 != 0 && d.len() as u64 != b.size) {
                        return Err(Error::InvalidRange);
                    }
                    stats.staging_bytes = stats
                        .staging_bytes
                        .checked_add((d.len() as u64).div_ceil(4) * 4)
                        .ok_or(Error::Budget)?;
                }
                Command::UploadImage(i, d) => {
                    self.image_usage(*i, wgpu::TextureUsages::COPY_DST)?;
                    self.descriptor(Descriptor::Image(*i), &mut state, &retired)?;
                    stats.staging_bytes = stats
                        .staging_bytes
                        .checked_add(image_layout(*i, [0, 0], [i.width, i.height], d)?.1)
                        .ok_or(Error::Budget)?;
                }
                Command::WriteImage(i, o, s, d) => {
                    self.image_usage(*i, wgpu::TextureUsages::COPY_DST)?;
                    self.descriptor(Descriptor::Image(*i), &mut state, &retired)?;
                    stats.staging_bytes = stats
                        .staging_bytes
                        .checked_add(image_layout(*i, *o, *s, d)?.1)
                        .ok_or(Error::Budget)?;
                }
                Command::Clear(b, offset, size) => {
                    self.buffer_usage(*b, wgpu::BufferUsages::COPY_DST)?;
                    self.descriptor(Descriptor::Buffer(*b), &mut state, &retired)?;
                    let size =
                        size.unwrap_or(b.size.checked_sub(*offset).ok_or(Error::InvalidRange)?);
                    if offset % 4 != 0
                        || size % 4 != 0
                        || offset.checked_add(size).is_none_or(|n| n > b.size)
                    {
                        return Err(Error::InvalidRange);
                    }
                }
                Command::Dispatch(id, g, resources) => {
                    if g.iter()
                        .any(|n| *n > self.device.limits().max_compute_workgroups_per_dimension)
                    {
                        return Err(Error::InvalidDispatch);
                    }
                    self.bindings(*id, resources, false, &mut state, &retired)?;
                    stats.dispatches += 1;
                }
                Command::DispatchIndirect(id, b, offset, resources) => {
                    self.buffer_usage(*b, wgpu::BufferUsages::INDIRECT)?;
                    self.descriptor(Descriptor::Buffer(*b), &mut state, &retired)?;
                    if offset % 4 != 0 || offset.checked_add(12).is_none_or(|n| n > b.size) {
                        return Err(Error::InvalidRange);
                    }
                    self.bindings(*id, resources, false, &mut state, &retired)?;
                    stats.dispatches += 1;
                }
                Command::Draw(p) => {
                    self.image_usage(p.target, wgpu::TextureUsages::RENDER_ATTACHMENT)?;
                    self.bindings(p.shader, &p.resources, true, &mut state, &retired)?;
                    self.descriptor(Descriptor::Image(p.target), &mut state, &retired)?;
                    if self.shaders[&p.shader].format != Some(p.target.format)
                        || p.vertices.start > p.vertices.end
                        || p.instances.start > p.instances.end
                        || p.resources.iter().any(|r| r.id() == p.target.id)
                    {
                        return Err(Error::InvalidBinding);
                    }
                    stats.draws += 1;
                }
                Command::Download(b) => {
                    self.buffer_usage(*b, wgpu::BufferUsages::COPY_SRC)?;
                    self.descriptor(Descriptor::Buffer(*b), &mut state, &retired)?;
                    stats.staging_bytes = stats
                        .staging_bytes
                        .checked_add(Descriptor::Buffer(*b).bytes().unwrap())
                        .ok_or(Error::Budget)?;
                }
                Command::DownloadImage(i) => {
                    self.image_usage(*i, wgpu::TextureUsages::COPY_SRC)?;
                    self.descriptor(Descriptor::Image(*i), &mut state, &retired)?;
                    stats.staging_bytes = stats
                        .staging_bytes
                        .checked_add(
                            (i.width as u64 * i.format.bytes_per_pixel() as u64).div_ceil(256)
                                * 256
                                * i.height as u64,
                        )
                        .ok_or(Error::Budget)?;
                }
                Command::FreeBuffer(b) => {
                    if state.remove(&b.id) != Some(Descriptor::Buffer(*b)) {
                        return Err(Error::InvalidResource);
                    }
                    retired.insert(b.id);
                }
                Command::FreeImage(i) => {
                    if state.remove(&i.id) != Some(Descriptor::Image(*i)) {
                        return Err(Error::InvalidResource);
                    }
                    retired.insert(i.id);
                }
            }
            for (id, d) in &state {
                if seen.insert(*id) {
                    stats.allocated_bytes = stats
                        .allocated_bytes
                        .checked_add(d.bytes().ok_or(Error::Budget)?)
                        .ok_or(Error::Budget)?;
                }
            }
            if seen.len() > self.limits.max_resources
                || initial_bytes
                    .checked_add(stats.allocated_bytes)
                    .is_none_or(|n| n > self.limits.max_live_bytes)
            {
                return Err(Error::Budget);
            }
            if stats.staging_bytes > self.limits.max_staging_bytes {
                return Err(Error::Budget);
            }
        }
        stats.allocated_bytes = 0; // encode counts actual allocations after successful preflight.
        Ok(stats)
    }
    fn ensure(
        &self,
        r: ResourceProxy,
        state: &mut HashMap<ResourceId, Resource>,
        stats: &mut ExecutionStats,
    ) {
        if state.contains_key(&r.id()) {
            return;
        }
        let resource = match r {
            ResourceProxy::Buffer(b) | ResourceProxy::BufferRange { proxy: b, .. } => {
                let size = Descriptor::Buffer(b).bytes().unwrap();
                stats.allocated_bytes += size;
                Resource::Buffer(
                    b,
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(b.name),
                        size,
                        usage: wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::UNIFORM
                            | wgpu::BufferUsages::INDIRECT
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                )
            }
            ResourceProxy::Image(i) => {
                stats.allocated_bytes += Descriptor::Image(i).bytes().unwrap();
                let format = i.format.to_wgpu();
                let usages = format
                    .guaranteed_format_features(self.device.features())
                    .allowed_usages;
                let usage = wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | (usages & wgpu::TextureUsages::STORAGE_BINDING);
                Resource::Image(
                    i,
                    self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("recorded-image"),
                        size: wgpu::Extent3d {
                            width: i.width,
                            height: i.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    }),
                )
            }
        };
        state.insert(r.id(), resource);
    }
    fn compile(&mut self, id: ShaderId, stats: &mut ExecutionStats) {
        let s = self.shaders.get_mut(&id).unwrap();
        if s.pipeline.is_some() {
            return;
        }
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&s.layout)],
                immediate_size: 0,
            });
        let pipeline = if let Some(vertex) = &s.vertex {
            Pipeline::Render(
                self.device
                    .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("recorded-render"),
                        layout: Some(&layout),
                        vertex: wgpu::VertexState {
                            module: &s.module,
                            entry_point: Some(vertex),
                            compilation_options: Default::default(),
                            buffers: &[],
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &s.module,
                            entry_point: Some(&s.entry),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: s.format.unwrap().to_wgpu(),
                                blend: None,
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        primitive: Default::default(),
                        depth_stencil: None,
                        multisample: Default::default(),
                        multiview_mask: None,
                        cache: self.cache.as_ref(),
                    }),
            )
        } else {
            Pipeline::Compute(self.device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("recorded-compute"),
                    layout: Some(&layout),
                    module: &s.module,
                    entry_point: Some(&s.entry),
                    compilation_options: Default::default(),
                    cache: self.cache.as_ref(),
                },
            ))
        };
        s.pipeline = Some(pipeline);
        stats.compiled_pipelines += 1;
    }
    fn bind(
        &self,
        id: ShaderId,
        resources: &[ResourceProxy],
        state: &HashMap<ResourceId, Resource>,
    ) -> wgpu::BindGroup {
        let views = resources
            .iter()
            .map(|r| match state.get(&r.id()).unwrap() {
                Resource::Image(_, t) => Some(t.create_view(&Default::default())),
                _ => None,
            })
            .collect::<Vec<_>>();
        let entries = resources
            .iter()
            .enumerate()
            .map(|(n, r)| wgpu::BindGroupEntry {
                binding: n as u32,
                resource: match *r {
                    ResourceProxy::Image(_) => {
                        wgpu::BindingResource::TextureView(views[n].as_ref().unwrap())
                    }
                    _ => {
                        let Resource::Buffer(b, buffer) = &state[&r.id()] else {
                            unreachable!()
                        };
                        let (offset, size) = match r {
                            ResourceProxy::BufferRange { offset, size, .. } => (*offset, *size),
                            _ => (0, b.size),
                        };
                        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer,
                            offset,
                            size: wgpu::BufferSize::new(size),
                        })
                    }
                },
            })
            .collect::<Vec<_>>();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.shaders[&id].layout,
            entries: &entries,
        })
    }
    /// Compile just the shaders used by a valid recording without allocating
    /// resources, staging uploads, changing proxy state or submitting work.
    /// Returns the number of newly compiled pipelines (zero after warmup).
    pub fn warm_up(&mut self, recording: &Recording) -> Result<u32, Error> {
        self.preflight(recording)?;
        let device = self.device.clone();
        let mut stats = ExecutionStats::default();
        let valid = jadix_renderer::wgpu_compat::try_gpu_init(&device, "recorded warmup", || {
            for cmd in &recording.commands {
                let id = match cmd {
                    Command::Dispatch(id, ..) | Command::DispatchIndirect(id, ..) => *id,
                    Command::Draw(p) => p.shader,
                    _ => continue,
                };
                self.compile(id, &mut stats);
            }
        });
        if valid.is_none() {
            for shader in self.shaders.values_mut() {
                shader.pipeline = None;
            }
            return Err(Error::GpuValidation);
        }
        Ok(stats.compiled_pipelines)
    }
    /// Submit in command order. Uploads use encoder copies, so two writes separated
    /// by dispatches see different data. Downloads capture their exact position.
    /// Failed preflight does not mutate resources; native GPU validation errors do not submit.
    /// Browser validation is asynchronous: use an outer awaited wgpu error scope
    /// and treat the returned handle as provisional until that scope succeeds.
    pub fn execute(
        &mut self,
        queue: &wgpu::Queue,
        recording: &Recording,
    ) -> Result<Execution, Error> {
        Ok(self.record(recording)?.submit(queue))
    }
    /// Encode a validated stream without submitting. Uploads are encoder copies,
    /// not queue writes, so callers may combine this packet with other command
    /// buffers. Resource ownership/state commits only after successful encoding.
    /// Submit in record order. Discarded packets require resource reset before
    /// reuse. Browser validation still needs an outer awaited error scope.
    pub fn record(&mut self, recording: &Recording) -> Result<RecordedExecution, Error> {
        let mut stats = self.preflight(recording)?;
        let device = self.device.clone();
        let encoded =
            jadix_renderer::wgpu_compat::try_gpu_init(&device, "recorded execution", || {
                let mut state = self.resources.clone();
                let mut retired = self.retired.clone();
                let mut downloads = Vec::new();
                let mut encoder = self.device.create_command_encoder(&Default::default());
                for cmd in &recording.commands {
                    match cmd {
                        Command::Upload(b, data) | Command::UploadUniform(b, data) => {
                            self.ensure((*b).into(), &mut state, &mut stats);
                            let mut padded = data.to_vec();
                            padded.resize(padded.len().div_ceil(4) * 4, 0);
                            if !padded.is_empty() {
                                let upload = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: None,
                                        contents: &padded,
                                        usage: wgpu::BufferUsages::COPY_SRC,
                                    },
                                );
                                let Resource::Buffer(_, dest) = &state[&b.id] else {
                                    unreachable!()
                                };
                                encoder.copy_buffer_to_buffer(
                                    &upload,
                                    0,
                                    dest,
                                    0,
                                    padded.len() as u64,
                                );
                            }
                        }
                        Command::UploadImage(i, data) => self.upload_image(
                            &mut encoder,
                            &mut state,
                            &mut stats,
                            *i,
                            [0, 0],
                            [i.width, i.height],
                            data,
                        ),
                        Command::WriteImage(i, origin, size, data) => self.upload_image(
                            &mut encoder,
                            &mut state,
                            &mut stats,
                            *i,
                            *origin,
                            *size,
                            data,
                        ),
                        Command::Clear(b, o, s) => {
                            self.ensure((*b).into(), &mut state, &mut stats);
                            let Resource::Buffer(_, buffer) = &state[&b.id] else {
                                unreachable!()
                            };
                            encoder.clear_buffer(buffer, *o, *s);
                        }
                        Command::Dispatch(id, _, resources)
                        | Command::DispatchIndirect(id, _, _, resources) => {
                            for r in resources {
                                self.ensure(*r, &mut state, &mut stats);
                            }
                            if let Command::DispatchIndirect(_, b, _, _) = cmd {
                                self.ensure((*b).into(), &mut state, &mut stats);
                            }
                            self.compile(*id, &mut stats);
                            let bg = self.bind(*id, resources, &state);
                            let Pipeline::Compute(pipeline) =
                                self.shaders[id].pipeline.as_ref().unwrap()
                            else {
                                unreachable!()
                            };
                            let mut pass = encoder.begin_compute_pass(&Default::default());
                            pass.set_pipeline(pipeline);
                            pass.set_bind_group(0, &bg, &[]);
                            match cmd {
                                Command::Dispatch(_, g, _) => {
                                    pass.dispatch_workgroups(g[0], g[1], g[2])
                                }
                                Command::DispatchIndirect(_, b, offset, _) => {
                                    let Resource::Buffer(_, buffer) = &state[&b.id] else {
                                        unreachable!()
                                    };
                                    pass.dispatch_workgroups_indirect(buffer, *offset);
                                }
                                _ => unreachable!(),
                            }
                        }
                        Command::Draw(p) => {
                            self.ensure(p.target.into(), &mut state, &mut stats);
                            for r in &p.resources {
                                self.ensure(*r, &mut state, &mut stats);
                            }
                            self.compile(p.shader, &mut stats);
                            let bg = self.bind(p.shader, &p.resources, &state);
                            let Resource::Image(_, target) = &state[&p.target.id] else {
                                unreachable!()
                            };
                            let view = target.create_view(&Default::default());
                            let Pipeline::Render(pipeline) =
                                self.shaders[&p.shader].pipeline.as_ref().unwrap()
                            else {
                                unreachable!()
                            };
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: None,
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: p
                                            .clear
                                            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                ..Default::default()
                            });
                            pass.set_pipeline(pipeline);
                            pass.set_bind_group(0, &bg, &[]);
                            pass.draw(p.vertices.clone(), p.instances.clone());
                        }
                        Command::Download(b) => {
                            self.ensure((*b).into(), &mut state, &mut stats);
                            let size = Descriptor::Buffer(*b).bytes().unwrap();
                            let staging = self.download_buffer(size);
                            let Resource::Buffer(_, source) = &state[&b.id] else {
                                unreachable!()
                            };
                            encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
                            downloads.push(Download {
                                resource: b.id,
                                buffer: staging,
                                size: b.size,
                                bytes_per_row: None,
                                image: None,
                            });
                        }
                        Command::DownloadImage(i) => {
                            self.ensure((*i).into(), &mut state, &mut stats);
                            let row = (i.width * i.format.bytes_per_pixel()).div_ceil(256) * 256;
                            let size = row as u64 * i.height as u64;
                            let staging = self.download_buffer(size);
                            let Resource::Image(_, source) = &state[&i.id] else {
                                unreachable!()
                            };
                            encoder.copy_texture_to_buffer(
                                source.as_image_copy(),
                                wgpu::TexelCopyBufferInfo {
                                    buffer: &staging,
                                    layout: wgpu::TexelCopyBufferLayout {
                                        offset: 0,
                                        bytes_per_row: Some(row),
                                        rows_per_image: Some(i.height),
                                    },
                                },
                                source.size(),
                            );
                            downloads.push(Download {
                                resource: i.id,
                                buffer: staging,
                                size,
                                bytes_per_row: Some(row),
                                image: Some(*i),
                            });
                        }
                        Command::FreeBuffer(b) => {
                            state.remove(&b.id);
                            retired.insert(b.id);
                        }
                        Command::FreeImage(i) => {
                            state.remove(&i.id);
                            retired.insert(i.id);
                        }
                    }
                }
                (encoder.finish(), state, retired, downloads)
            });
        let Some((commands, state, retired, downloads)) = encoded else {
            for shader in self.shaders.values_mut() {
                shader.pipeline = None;
            }
            return Err(Error::GpuValidation);
        };
        self.resources = state;
        self.retired = retired;
        stats.live_bytes = self.live_bytes();
        Ok(RecordedExecution {
            commands,
            downloads,
            stats,
        })
    }
    fn download_buffer(&self, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("recorded-download"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit encoded texture copy domain"
    )]
    fn upload_image(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        state: &mut HashMap<ResourceId, Resource>,
        stats: &mut ExecutionStats,
        i: ImageProxy,
        origin: [u32; 2],
        size: [u32; 2],
        data: &[u8],
    ) {
        self.ensure(i.into(), state, stats);
        let (row, bytes) = image_layout(i, origin, size, data).unwrap();
        let tight = (size[0] * i.format.bytes_per_pixel()) as usize;
        let mut padded = vec![0; bytes as usize];
        for y in 0..size[1] as usize {
            padded[y * row as usize..y * row as usize + tight]
                .copy_from_slice(&data[y * tight..(y + 1) * tight]);
        }
        let staging = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &padded,
                usage: wgpu::BufferUsages::COPY_SRC,
            });
        let Resource::Image(_, target) = &state[&i.id] else {
            unreachable!()
        };
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin[0],
                    y: origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
    }
}
fn image_layout(
    i: ImageProxy,
    origin: [u32; 2],
    size: [u32; 2],
    data: &[u8],
) -> Result<(u32, u64), Error> {
    if size.contains(&0)
        || origin[0].checked_add(size[0]).is_none_or(|n| n > i.width)
        || origin[1].checked_add(size[1]).is_none_or(|n| n > i.height)
    {
        return Err(Error::InvalidRange);
    }
    let tight = size[0]
        .checked_mul(i.format.bytes_per_pixel())
        .ok_or(Error::InvalidRange)?;
    if data.len() as u64 != tight as u64 * size[1] as u64 {
        return Err(Error::InvalidImage);
    }
    let row = tight.checked_add(255).ok_or(Error::InvalidRange)? / 256 * 256;
    Ok((row, row as u64 * size[1] as u64))
}
