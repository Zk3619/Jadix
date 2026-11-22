//! Native shader leaves share the plot's DX12 surface, recovery and readback.
use super::*;
use jadix_core::hosted::{compile_shader_wgsl, pack_shader_block, CompiledShader, HostedContent, ShaderComposition, ShaderFrame};
use std::time::Instant;

const VERTEX: &str = "@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> { let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)); return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0); }";
pub(super) struct Program {
    compiled: CompiledShader,
    buffers: Vec<wgpu::Buffer>,
    groups: Vec<wgpu::BindGroup>,
    specification: HostedContent,
    clock: (Instant, f64, bool),
    scale: f32,
    images: Option<Images>,
    progress: f32,
    backdrop_pipeline: wgpu::RenderPipeline,
}
pub(crate) struct Shader {
    surface: Plot,
    output: super::capture::Output,
    window: HWND,
    root: HWND,
    failed: bool,
    loader: super::super::image::Loader,
    resources: BTreeMap<String, Texture>,
    asset_root: Arc<Mutex<std::path::PathBuf>>,
    inputs: BTreeMap<String, Arc<super::super::image::Pixels>>,
    transition: Option<Instant>,
    transition_pending: bool,
    paused_at: Option<Instant>,
}
struct Texture {
    node: NodeId,
    generation: u64,
    source: String,
    pixels: Option<super::super::image::Pixels>,
}
struct Images {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    names: Vec<String>,
    textures: Vec<wgpu::Texture>,
    group: Option<wgpu::BindGroup>,
}
impl Images {
    fn new(device: &wgpu::Device, names: Vec<String>) -> Option<Self> {
        if names.is_empty() { return None; }
        let entries: Vec<_> = names.iter().enumerate().flat_map(|(i, _)| [
            wgpu::BindGroupLayoutEntry { binding: i as u32 * 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: i as u32 * 2 + 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ]).collect();
        Some(Self {
            layout: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("Jadix shader images"), entries: &entries }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() }),
            names, textures: Vec::new(), group: None,
        })
    }
    fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, specification: &HostedContent,
        resources: &BTreeMap<String, Texture>, inputs: &BTreeMap<String, Arc<super::super::image::Pixels>>) -> Result<wgpu::BindGroup, String> {
        let HostedContent::Shader { textures, .. } = specification else { unreachable!() };
        let placeholder = super::super::image::Pixels { width: 1, height: 1, bytes: vec![0; 4] };
        let mut changed = self.group.is_none();
        for (index, name) in self.names.iter().enumerate() {
            let pixels = inputs.get(name).map(Arc::as_ref).or_else(|| resources.get(name)
                .filter(|v| textures.get(name) == Some(&v.source)).and_then(|v| v.pixels.as_ref())).unwrap_or(&placeholder);
            if pixels.width > device.limits().max_texture_dimension_2d || pixels.height > device.limits().max_texture_dimension_2d {
                return Err("shader texture exceeds device limit".into());
            }
            if self.textures.get(index).is_none_or(|v| v.width() != pixels.width || v.height() != pixels.height) {
                let texture = device.create_texture(&wgpu::TextureDescriptor { label: Some(name), size: wgpu::Extent3d { width: pixels.width, height: pixels.height, depth_or_array_layers: 1 },
                    mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
                    format: if textures.contains_key(name) { wgpu::TextureFormat::Bgra8Unorm } else { wgpu::TextureFormat::Bgra8UnormSrgb },
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
                if index == self.textures.len() { self.textures.push(texture); } else { self.textures[index] = texture; }
                changed = true;
            }
            let texture = &self.textures[index];
            queue.write_texture(texture.as_image_copy(), &pixels.bytes, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pixels.width * 4), rows_per_image: Some(pixels.height) }, texture.size());
        }
        if changed {
            let views: Vec<_> = self.textures.iter().map(|v| v.create_view(&Default::default())).collect();
            let entries: Vec<_> = views.iter().enumerate().flat_map(|(i, view)| [wgpu::BindGroupEntry { binding: i as u32 * 2, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: i as u32 * 2 + 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }]).collect();
            self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("Jadix shader images"), layout: &self.layout, entries: &entries }));
        }
        Ok(self.group.as_ref().unwrap().clone())
    }
}
impl Shader {
    pub(crate) fn new(window: HWND, root: HWND, specification: &HostedContent, scale: f32, asset_root: Arc<Mutex<std::path::PathBuf>>) -> Result<Self, String> {
        let _dpi = DpiScope::window(window);
        let output = super::capture::Output::new(window)?;
        let mut surface = Plot::new(output.0, &[[0., 0.], [1., 1.]])?;
        let HostedContent::Shader { time_seconds, animated, paused, .. } = specification else { return Err("expected shader content".into()) };
        let (program, pipeline) = Program::new(&surface, specification, scale, (Instant::now(), time_seconds.unwrap_or(0.), *animated && !*paused), &BTreeMap::new())?;
        surface.pipeline = pipeline;
        surface.program = Some(program);
        surface.count = 3;
        let mut shader = Self { surface, output, window, root, failed: false, loader: super::super::image::Loader::new(window)?, resources: BTreeMap::new(), asset_root,
            inputs: BTreeMap::new(), transition: None, transition_pending: false, paused_at: paused.then(Instant::now) };
        shader.load_textures(specification)?;
        Ok(shader)
    }
    fn load_textures(&mut self, specification: &HostedContent) -> Result<(), String> {
        let HostedContent::Shader { textures, .. } = specification else { unreachable!() };
        self.resources.retain(|name, resource| {
            let keep = textures.get(name) == Some(&resource.source);
            if !keep { self.loader.cancel(resource.node); }
            keep
        });
        for (name, source) in textures {
            if self.resources.contains_key(name) { continue; }
            let node = NodeId::new(RootId::new());
            let generation = self.loader.request(node, source, &self.asset_root.lock().map_err(|_| "shader asset root lock failed")?)?;
            self.resources.insert(name.clone(), Texture { node, generation, source: source.clone(), pixels: None });
        }
        Ok(())
    }
    pub(crate) fn drain(&mut self) -> Result<Vec<String>, String> {
        let mut errors = Vec::new();
        let mut changed = false;
        for completion in self.loader.drain()? {
            let total: usize = self.resources.values().filter_map(|v| v.pixels.as_ref()).map(|v| v.bytes.len()).sum();
            let Some((name, resource)) = self.resources.iter_mut().find(|(_, v)| v.node == completion.node && v.generation == completion.generation) else { continue; };
            match completion.result {
                Ok(pixels) if total + pixels.bytes.len() <= 32 * 1024 * 1024 => { resource.pixels = Some(pixels); changed = true; }
                Ok(_) => errors.push(format!("shader texture {name}: decoded image budget exceeded")),
                Err(error) => errors.push(format!("shader texture {name}: {error}")),
            }
        }
        if changed {
            self.refresh_images()?;
            self.surface.dirty = true;
            unsafe { InvalidateRect(self.window, null_mut(), 0); }
        }
        Ok(errors)
    }
    pub(crate) fn update(&mut self, specification: &HostedContent, scale: f32) -> Result<(), String> {
        let program = self.surface.program.as_ref().unwrap();
        if &program.specification == specification && program.scale == scale { return Ok(()); }
        let compiled = compile_shader_wgsl(specification)?;
        let HostedContent::Shader { textures, .. } = specification else { unreachable!() };
        let HostedContent::Shader { textures: old_textures, .. } = &program.specification else { unreachable!() };
        let HostedContent::Shader { composition, .. } = specification else { unreachable!() };
        let HostedContent::Shader { composition: old_composition, .. } = &program.specification else { unreachable!() };
        let HostedContent::Shader { paused, .. } = specification else { unreachable!() };
        let transition_changed = matches!((old_composition, composition),
            (ShaderComposition::Transition { value: old, .. }, ShaderComposition::Transition { value: next, .. }) if old != next);
        if compiled.source != program.compiled.source || textures != old_textures {
            let (replacement, pipeline) = Program::new(&self.surface, specification, scale, program.next_clock(specification), &self.resources)?;
            self.surface.pipeline = pipeline;
            self.surface.program = Some(replacement);
        } else {
            let program = self.surface.program.as_mut().unwrap();
            program.compiled = compiled;
            program.clock = program.next_clock(specification);
            program.specification = specification.clone();
            program.scale = scale;
        }
        self.load_textures(specification)?;
        if *paused && self.paused_at.is_none() { self.paused_at = Some(Instant::now()); }
        else if !*paused {
            if let Some(paused_at) = self.paused_at.take() {
                if let Some(start) = self.transition { self.transition = start.checked_add(paused_at.elapsed()); }
            }
        }
        if transition_changed {
            if let Some(content) = self.inputs.get("content").cloned() { self.inputs.insert("from".into(), content); }
            self.transition_pending = true;
            self.transition = None;
        } else if !matches!(composition, ShaderComposition::Transition { .. }) {
            self.transition_pending = false;
            self.transition = None;
        }
        self.failed = false;
        self.surface.dirty = true;
        Ok(())
    }
    pub(crate) fn draw(&mut self, force: bool) -> Result<(), String> {
        if self.failed { return Ok(()); }
        let _dpi = DpiScope::window(self.window);
        let result = (|| {
            self.capture()?;
            self.surface.draw(self.output.0, force)
        })();
        self.failed = result.is_err();
        if self.failed { unsafe { ShowWindow(self.output.0, SW_HIDE); } }
        result
    }
    pub(crate) fn interval(&self) -> Option<u32> {
        let HostedContent::Shader { animated, paused, frames_per_second, progress, composition, .. } = &self.surface.program.as_ref()?.specification else { return None };
        let transitioning = progress.is_none() && (self.transition_pending || self.transition.is_some_and(|start| composition.transition_progress(start.elapsed().as_secs_f64() * 1000.) < 1.));
        ((!*paused && (*animated || transitioning)) && !self.failed).then(|| (1000 / u32::from(*frames_per_second)).max(1))
    }
    fn refresh_images(&mut self) -> Result<(), String> {
        let program = self.surface.program.as_mut().unwrap();
        if let Some(images) = &mut program.images {
            program.groups[1] = images.update(&self.surface.device, &self.surface.queue, &program.specification, &self.resources, &self.inputs)?;
        }
        Ok(())
    }
    fn capture(&mut self) -> Result<(), String> {
        let program = self.surface.program.as_ref().unwrap();
        let HostedContent::Shader { composition, max_sample_offset, .. } = &program.specification else { unreachable!() };
        self.output.position(self.window, !matches!(composition, ShaderComposition::Background))?;
        let padding = (*max_sample_offset * program.scale).ceil() as i32;
        let backdrop = Arc::new(super::capture::capture(self.window, &self.output, self.root, padding, true)?);
        if matches!(composition, ShaderComposition::Background) {
            self.inputs.clear(); self.inputs.insert("_native_backdrop".into(), backdrop);
            return self.refresh_images();
        }
        let content = Arc::new(super::capture::capture(self.window, &self.output, self.root, padding, false)?);
        if self.transition_pending && self.paused_at.is_none() {
            self.transition = Some(Instant::now()); self.transition_pending = false;
        }
        if !self.inputs.contains_key("from") { self.inputs.insert("from".into(), content.clone()); }
        self.inputs.insert("to".into(), content.clone());
        self.inputs.insert("content".into(), content);
        self.inputs.insert("backdrop".into(), backdrop);
        let progress = self.transition.map_or(if self.transition_pending { 0. } else { 1. }, |start|
            composition.transition_progress(self.paused_at.unwrap_or_else(Instant::now).saturating_duration_since(start).as_secs_f64() * 1000.));
        self.surface.program.as_mut().unwrap().progress = progress;
        self.refresh_images()
    }
    pub(crate) fn print(&self, dc: HDC) -> Result<(), String> {
        let _dpi = DpiScope::window(self.window);
        let mut bounds = RECT::default();
        unsafe { GetClientRect(self.window, &mut bounds); }
        if bounds.right <= 0 || bounds.bottom <= 0 { return Ok(()); }
        let (size, mut pixels) = self.surface.readback_size([bounds.right as u32, bounds.bottom as u32])?;
        for pixel in pixels.chunks_exact_mut(4) { pixel.swap(0, 2); }
        let mut header = BITMAPINFO::default();
        header.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: size[0] as i32,
            biHeight: -(size[1] as i32), biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..Default::default() };
        if unsafe { StretchDIBits(dc, 0, 0, bounds.right, bounds.bottom, 0, 0, bounds.right, bounds.bottom,
            pixels.as_ptr().cast(), &header, DIB_RGB_COLORS, SRCCOPY) } == 0 { return Err(native_error("paint shader capture")); }
        Ok(())
    }
}
impl Program {
    fn next_clock(&self, specification: &HostedContent) -> (Instant, f64, bool) {
        let HostedContent::Shader { paused, animated, time_seconds, .. } = specification else { return self.clock };
        let HostedContent::Shader { time_seconds: old_time, .. } = &self.specification else { return self.clock };
        let elapsed = self.time();
        let value = if time_seconds != old_time { time_seconds.unwrap_or(elapsed) } else { elapsed };
        (Instant::now(), value, *animated && !*paused)
    }
    fn time(&self) -> f64 { self.clock.1 + if self.clock.2 { self.clock.0.elapsed().as_secs_f64() } else { 0. } }
    fn new(surface: &Plot, specification: &HostedContent, scale: f32, clock: (Instant, f64, bool), resources: &BTreeMap<String, Texture>) -> Result<(Self, wgpu::RenderPipeline), String> {
        let compiled = compile_shader_wgsl(specification)?;
        let HostedContent::Shader { composition, textures, .. } = specification else { unreachable!() };
        let device = &surface.device;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result: Result<_, String> = (|| {
            let buffers: Vec<_> = compiled.blocks.iter().map(|block| device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&block.name), size: u64::from(block.size), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
            })).collect();
            let entries: Vec<_> = compiled.blocks.iter().map(|block| wgpu::BindGroupLayoutEntry { binding: u32::from(!block.builtin), visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: std::num::NonZeroU64::new(u64::from(block.size)) }, count: None }).collect();
            let uniforms = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("Jadix shader inputs"), entries: &entries });
            let entries: Vec<_> = compiled.blocks.iter().zip(&buffers).map(|(block, buffer)| wgpu::BindGroupEntry { binding: u32::from(!block.builtin), resource: buffer.as_entire_binding() }).collect();
            let mut groups = vec![device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("Jadix shader inputs"), layout: &uniforms, entries: &entries })];
            let mut layouts = vec![uniforms];
            let mut names: Vec<_> = textures.keys().cloned().chain(composition.inputs().iter().map(|v| (*v).to_owned())).collect();
            if matches!(composition, ShaderComposition::Background) { names.push("_native_backdrop".into()); }
            let backdrop_binding = (names.len() - 1) * 2;
            let mut images = Images::new(device, names);
            if let Some(images) = &mut images {
                groups.push(images.update(device, &surface.queue, specification, resources, &BTreeMap::new())?);
                layouts.push(images.layout.clone());
            }
            let layouts: Vec<_> = layouts.iter().map(Some).collect();
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Jadix shader"), bind_group_layouts: &layouts, immediate_size: 0 });
            let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Jadix fullscreen triangle"), source: wgpu::ShaderSource::Wgsl(VERTEX.into()) });
            let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Jadix shared shader"), source: wgpu::ShaderSource::Wgsl(compiled.source.as_str().into()) });
            // HWND swapchains are opaque. Paint the actual lower native layers
            // before blending the portable shader's premultiplied output.
            let backdrop = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Jadix native backdrop"), source: wgpu::ShaderSource::Wgsl(format!(
                "struct Frame {{ size: vec2<f32>, time: f32, scale: f32, progress: f32, overflow: f32, pixels: vec2<f32> }};\n@group(0) @binding(0) var<uniform> frame: Frame;\n@group(1) @binding({backdrop_binding}) var image: texture_2d<f32>;\n@group(1) @binding({}) var image_sampler: sampler;\n@fragment fn backdrop(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ let uv = (p.xy / frame.pixels * frame.size + vec2<f32>(frame.overflow)) / (frame.size + vec2<f32>(2.0 * frame.overflow)); return textureSample(image, image_sampler, uv); }}", backdrop_binding + 1).into()) });
            let backdrop_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Jadix native backdrop"), layout: Some(&layout), vertex: wgpu::VertexState { module: &vertex, entry_point: Some("vertex"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(), depth_stencil: None, multisample: Default::default(), fragment: Some(wgpu::FragmentState { module: &backdrop, entry_point: Some("backdrop"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: surface.configuration.format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }), multiview_mask: None, cache: None,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Jadix native shader"), layout: Some(&layout), vertex: wgpu::VertexState { module: &vertex, entry_point: Some("vertex"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(), depth_stencil: None, multisample: Default::default(), fragment: Some(wgpu::FragmentState { module: &fragment, entry_point: Some("jadix_fragment"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: surface.configuration.format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), multiview_mask: None, cache: None,
            });
            Ok((buffers, groups, pipeline, images, backdrop_pipeline))
        })();
        let validation = pollster::block_on(scope.pop());
        let (buffers, groups, pipeline, images, backdrop_pipeline) = result?;
        if let Some(error) = validation { return Err(format!("native shader pipeline: {error}")); }
        let program = Self { compiled, buffers, groups, specification: specification.clone(), clock, scale, images, progress: 1., backdrop_pipeline };
        Ok((program, pipeline))
    }
    pub(super) fn prepare(&self, queue: &wgpu::Queue, size: [u32; 2]) -> Result<(), String> {
        let HostedContent::Shader { parameters, progress, max_sample_offset, .. } = &self.specification else { unreachable!() };
        let pixels = size.map(|v| v as f32);
        let frame = ShaderFrame { size: pixels.map(|v| v / self.scale), pixels, time: self.time() as f32, scale: self.scale, progress: progress.unwrap_or(self.progress), overflow: *max_sample_offset };
        for (block, buffer) in self.compiled.blocks.iter().zip(&self.buffers) { queue.write_buffer(buffer, 0, &pack_shader_block(block, parameters, &frame)?); }
        Ok(())
    }
    pub(super) fn bind(&self, pass: &mut wgpu::RenderPass<'_>) { for (index, group) in self.groups.iter().enumerate() { pass.set_bind_group(index as u32, Some(group), &[]); } }
    pub(super) fn backdrop(&self, pass: &mut wgpu::RenderPass<'_>) { pass.set_pipeline(&self.backdrop_pipeline); self.bind(pass); pass.draw(0..3, 0..1); }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(source: &str) -> HostedContent {
        serde_json::from_value(serde_json::json!({"type":"shader", "language":"wgsl", "source":source, "function":"shade", "uniforms":[], "frames_per_second":60, "animated":false,
            "parameters":{"color":{"type":"vec4","value":[1,0,0,1]}}})).unwrap()
    }
    unsafe extern "system" fn color_capture(window: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, color: usize) -> LRESULT {
        if matches!(message, WM_PRINTCLIENT | WM_ERASEBKGND) {
            let mut rect = RECT::default();
            unsafe {
                GetClientRect(window, &mut rect);
                let brush = CreateSolidBrush(color as u32);
                FillRect(parameter as HDC, &rect, brush); DeleteObject(brush);
            }
            return 1;
        }
        unsafe { DefSubclassProc(window, message, parameter, data) }
    }
    #[test]
    fn dx12_composition_captures_children_backdrop_and_transition_without_replacing_input() {
        let mut host = WindowsHost::new("Jadix native composition validation", [320,240]).unwrap();
        let _dpi = DpiScope::window(host.window);
        let child = |parent: HWND, class: &str, position: [i32; 4]| unsafe {
            let window = CreateWindowExW(0, wide(class).as_ptr(), wide("").as_ptr(), WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                position[0], position[1], position[2], position[3], parent, null_mut(), GetModuleHandleW(null_mut()), null_mut());
            assert!(!window.is_null()); window
        };
        let behind = child(host.window, "STATIC", [20,20,200,100]);
        let mount = child(host.window, "STATIC", [20,20,200,100]);
        let later = child(host.window, "STATIC", [20,20,200,100]);
        let content = child(mount, "STATIC", [0,0,200,100]);
        let edit = child(mount, "EDIT", [0,0,40,20]);
        unsafe {
            SetWindowSubclass(behind, Some(color_capture), 1, 0x00ff0000);
            SetWindowSubclass(later, Some(color_capture), 1, 0x0000ff00);
            SetWindowSubclass(content, Some(color_capture), 1, 0x000000ff);
            SetWindowPos(behind, HWND_BOTTOM, 0,0,0,0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetWindowPos(mount, HWND_TOP, 0,0,0,0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetWindowPos(later, HWND_TOP, 0,0,0,0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            ShowWindow(host.window, SW_SHOWNA);
        }
        let mut specification = spec("fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_content(uv); }");
        if let HostedContent::Shader { composition, max_sample_offset, .. } = &mut specification { *composition = ShaderComposition::Effect; *max_sample_offset = 4.; }
        let mut shader = Shader::new(mount, host.window, &specification, 1., Arc::new(Mutex::new(std::env::temp_dir()))).unwrap();
        shader.draw(true).unwrap();
        let center = |shader: &Shader| -> [u8;4] { let pixels = shader.surface.readback_size([201,101]).unwrap().1; pixels[(50*201+100)*4..(50*201+101)*4].try_into().unwrap() };
        assert_eq!(center(&shader), [255,0,0,255]);
        assert_eq!(shader.interval(), None);
        let mut mount_bounds = RECT::default(); unsafe { GetClientRect(mount, &mut mount_bounds); }
        assert_eq!(shader.inputs["content"].width, mount_bounds.right as u32 + 8);
        unsafe {
            assert_eq!(SendMessageW(shader.output.0, WM_NCHITTEST, 0, 0), HTTRANSPARENT as isize);
            SetFocus(edit); assert_eq!(GetFocus(), edit);
            SetWindowTextW(edit, wide("中文🙂").as_ptr());
        }
        assert_eq!(unsafe { read_text(edit) }.unwrap(), "中文🙂");
        unsafe { ValidateRect(mount, null_mut()); SetWindowTextW(edit, wide("repaint 中文🙂").as_ptr()); }
        assert_ne!(unsafe { GetUpdateRect(mount, null_mut(), 0) }, 0, "native edits wake a static shader without a frame timer");
        if let HostedContent::Shader { source, .. } = &mut specification { *source = "fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_backdrop(uv); }".into(); }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        assert_eq!(center(&shader), [0,0,255,255], "later green siblings must stay outside the backdrop");
        if let HostedContent::Shader { source, composition, .. } = &mut specification {
            *source = "fn shade(uv: vec2<f32>) -> vec4<f32> { return mix(sample_from(uv), sample_to(uv), jadix.progress); }".into();
            *composition = ShaderComposition::Transition { value: "one".into(), duration_ms: 1000, fallback: Default::default() };
        }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        if let HostedContent::Shader { composition: ShaderComposition::Transition { value, .. }, .. } = &mut specification { *value = "two".into(); }
        shader.update(&specification, 1.).unwrap();
        unsafe { SetWindowSubclass(content, Some(color_capture), 1, 0x00ff0000); }
        shader.draw(true).unwrap();
        assert!(center(&shader)[0] > 240, "transition starts with the previous red frame");
        assert_eq!(shader.interval(), Some(16));
        if let HostedContent::Shader { paused, .. } = &mut specification { *paused = true; }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        let paused_progress = shader.surface.program.as_ref().unwrap().progress;
        shader.draw(true).unwrap();
        assert_eq!(shader.surface.program.as_ref().unwrap().progress, paused_progress);
        assert_eq!(shader.interval(), None);
        if let HostedContent::Shader { paused, .. } = &mut specification { *paused = false; }
        shader.update(&specification, 1.).unwrap();
        if let HostedContent::Shader { progress, .. } = &mut specification { *progress = Some(0.5); }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        let pixel = center(&shader); assert!(pixel[0].abs_diff(188) <= 1 && pixel[2].abs_diff(188) <= 1, "linear-light transition encoded to sRGB: {pixel:?}");
        assert_eq!(shader.interval(), None);
        if let HostedContent::Shader { progress, .. } = &mut specification { *progress = None; }
        shader.update(&specification, 1.).unwrap();
        shader.transition = Some(Instant::now() - std::time::Duration::from_secs(2));
        shader.draw(true).unwrap();
        assert_eq!(center(&shader), [0,0,255,255]);
        assert_eq!(shader.interval(), None);
        assert_eq!(unsafe { GetFocus() }, edit);
        if let HostedContent::Shader { source, composition, .. } = &mut specification {
            *source = "fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.,1.,1.,0.5); }".into();
            *composition = ShaderComposition::Effect;
        }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        let pixel = center(&shader);
        assert!(pixel[0].abs_diff(188) <= 1 && pixel[1].abs_diff(188) <= 1 && pixel[2] == 255, "alpha blends over the actual blue backdrop: {pixel:?}");
        unsafe { SetWindowSubclass(content, Some(color_capture), 1, 0x00808080); }
        if let HostedContent::Shader { source, .. } = &mut specification { *source = "fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_content(uv); }".into(); }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        let pixel = center(&shader); assert!(pixel[..3].iter().all(|v| v.abs_diff(128) <= 1), "native sRGB capture must round-trip: {pixel:?}");
        if let HostedContent::Shader { source, composition, .. } = &mut specification {
            *source = "fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.); }".into();
            *composition = ShaderComposition::Background;
        }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        assert_eq!(center(&shader), [0,0,255,255], "transparent shader background retains lower native layers");
        unsafe { SetWindowPos(mount, null_mut(), 0,0,5000,100, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE); }
        assert!(shader.draw(true).unwrap_err().contains("budget"));
        assert_eq!(unsafe { GetWindowLongPtrW(shader.output.0, GWL_STYLE) } as u32 & WS_VISIBLE, 0);
        unsafe { SetWindowPos(mount, null_mut(), 0,0,200,100, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE); }
        if let HostedContent::Shader { max_sample_offset, .. } = &mut specification { *max_sample_offset = 0.; }
        shader.update(&specification, 1.).unwrap(); shader.draw(true).unwrap();
        assert_ne!(unsafe { GetWindowLongPtrW(shader.output.0, GWL_STYLE) } as u32 & WS_VISIBLE, 0);
        drop(shader); host.dispose();
    }
    #[test]
    fn dx12_shader_pixels_updates_clock_errors_and_surface_recovery() {
        let mut host = WindowsHost::new("Jadix native shader validation", [320,240]).unwrap();
        let mut specification = spec("fn shade(uv: vec2<f32>) -> vec4<f32> { if uv.y < 0.5 { return uniforms.color; } return vec4<f32>(0.,0.,1.,1.); }");
        let root = std::env::temp_dir().join(format!("jadix-shader-textures-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut shader = Shader::new(host.window, host.window, &specification, 1., Arc::new(Mutex::new(root.clone()))).unwrap();
        let pixel = |pixels: &[u8], x: usize, y: usize| { let i=(y*256+x)*4; <[u8;4]>::try_from(&pixels[i..i+4]).unwrap() };
        let (_, pixels) = shader.surface.readback().unwrap();
        assert_eq!(pixel(&pixels,128,16), [255,0,0,255]);
        assert_eq!(pixel(&pixels,128,112), [0,0,255,255]);
        if let HostedContent::Shader { parameters, .. } = &mut specification { parameters.insert("color".into(), jadix_core::hosted::ShaderUniform::Vec4([0.,1.,0.,1.])); }
        shader.update(&specification,2.).unwrap();
        let (_, pixels) = shader.surface.readback().unwrap();
        assert_eq!(pixel(&pixels,128,16), [0,255,0,255]);
        assert_eq!(shader.interval(),None);
        let mut invalid = specification.clone();
        if let HostedContent::Shader { source, .. } = &mut invalid { *source="fn shade(".into(); }
        assert!(shader.update(&invalid,1.).is_err());
        assert_eq!(pixel(&shader.surface.readback().unwrap().1,128,16),[0,255,0,255]);
        // Source changes preserve elapsed time unless the controlling time changes.
        let program = shader.surface.program.as_mut().unwrap();
        program.clock=(Instant::now(),42.,false);
        if let HostedContent::Shader { source, animated, paused, .. } = &mut specification { *source="fn shade(uv: vec2<f32>) -> vec4<f32> { return uniforms.color * vec4<f32>(1.); }".into(); *animated=true; *paused=true; }
        shader.update(&specification,1.).unwrap();
        assert_eq!(shader.surface.program.as_ref().unwrap().time(),42.);
        assert_eq!(shader.interval(),None);
        if let HostedContent::Shader { animated, paused, time_seconds, .. } = &mut specification { *animated=true; *paused=false; *time_seconds=Some(7.); }
        shader.update(&specification,1.).unwrap();
        assert!(shader.surface.program.as_ref().unwrap().time() >= 7.);
        assert_eq!(shader.interval(),Some(16));
        shader.surface.recreate(shader.output.0).unwrap();
        assert_eq!(pixel(&shader.surface.readback().unwrap().1,128,16),[0,255,0,255]);
        // Default-language GLSL preserves the GLES pixel coordinate convention
        // while using the same DX12 pipeline and premultiplied image inputs.
        specification = serde_json::from_value(serde_json::json!({"type":"shader", "source":"vec4 shade(vec2 p) { if (p.y > uResolution.y * 0.5) return sample_image(p / uResolution) * color * uValues[0]; return vec4(0,0,1,1); }", "function":"shade", "uniforms":[1.0], "frames_per_second":60, "animated":false,
            "parameters":{"color":{"type":"vec4","value":[0,1,0,1]}},
            "textures":{"image":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4////fwAJ+wP9KobjigAAAABJRU5ErkJggg=="}})).unwrap();
        shader.update(&specification,1.).unwrap();
        let await_texture = |shader: &mut Shader| {
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            while shader.resources.values().any(|resource| resource.pixels.is_none()) {
                assert!(shader.drain().unwrap().is_empty());
                assert!(Instant::now() < deadline, "asynchronous shader texture timed out");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        await_texture(&mut shader);
        let (_, pixels) = shader.surface.readback().unwrap();
        assert_eq!(pixel(&pixels,128,16),[0,255,0,255]);
        assert_eq!(pixel(&pixels,128,112),[0,0,255,255]);
        // A bundle texture uses the same asynchronous, bounded resolver as images.
        let HostedContent::Shader { textures, .. } = &mut specification else { unreachable!() };
        use base64::Engine;
        let png = textures["image"].split_once(",").unwrap().1;
        std::fs::write(root.join("纹理.png"), base64::engine::general_purpose::STANDARD.decode(png).unwrap()).unwrap();
        textures.insert("image".into(), "asset:/%E7%BA%B9%E7%90%86.png".into());
        shader.update(&specification, 1.).unwrap();
        assert!(shader.resources["image"].pixels.is_none());
        await_texture(&mut shader);
        assert_eq!(pixel(&shader.surface.readback().unwrap().1,128,16),[0,255,0,255]);
        drop(shader); host.dispose();
        std::fs::remove_file(root.join("纹理.png")).unwrap(); std::fs::remove_dir(root).unwrap();
    }
}
