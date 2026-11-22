use super::*;
use rwh_06::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
use std::num::NonZeroIsize;
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;
mod shader;
mod capture;
pub(super) use shader::Shader;

const SHADER: &str = r#"
@vertex fn vertex(@location(0) point: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(point, 0.0, 1.0);
}
@fragment fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(0.05, 0.65, 0.9, 1.0);
}
"#;

pub(super) struct Plot {
    surface: wgpu::Surface<'static>,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    count: u32,
    configuration: wgpu::SurfaceConfiguration,
    dirty: bool,
    lost: Arc<Mutex<Option<String>>>,
    frames: u64,
    recreations: u64,
    program: Option<shader::Program>,
}

fn target(window: HWND) -> Result<wgpu::SurfaceTargetUnsafe, String> {
    let mut handle =
        Win32WindowHandle::new(NonZeroIsize::new(window as isize).ok_or("missing GPU window")?);
    handle.hinstance = NonZeroIsize::new(unsafe { GetModuleHandleW(null_mut()) } as isize);
    Ok(wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
        raw_window_handle: RawWindowHandle::Win32(handle),
    })
}

fn vertex_bytes(points: &[[f32; 2]]) -> Result<Vec<u8>, String> {
    if points.len() < 2
        || points.len() > 65_536
        || points
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || !(-1.0..=1.0).contains(value))
    {
        return Err(String::from(
            "GPU plot requires 2..65536 finite normalized points",
        ));
    }
    Ok(points
        .iter()
        .flatten()
        .flat_map(|value| value.to_ne_bytes())
        .collect())
}

impl Plot {
    pub(super) fn new(window: HWND, points: &[[f32; 2]]) -> Result<Self, String> {
        let bytes = vertex_bytes(points)?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = unsafe { instance.create_surface_unsafe(target(window)?) }
            .map_err(|error| error.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Jadix native GPU plot"),
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let lost = Arc::new(Mutex::new(None));
        let reported = lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                if let Ok(mut error) = reported.lock() {
                    *error = Some(format!("GPU device lost: {reason:?}: {message}"));
                }
            }
        });
        let reported = lost.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            if let Ok(mut reported) = reported.lock() {
                *reported = Some(format!("GPU error: {error}"));
            }
        }));
        let mut configuration = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or("GPU surface has no compatible configuration")?;
        configuration.present_mode = wgpu::PresentMode::Fifo;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Jadix native line chart"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Jadix native line chart"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: configuration.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(format!("GPU plot pipeline: {error}"));
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Jadix native plot vertices"),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        Ok(Self {
            surface,
            instance,
            adapter,
            device,
            queue,
            pipeline,
            vertices,
            count: points.len() as u32,
            configuration,
            dirty: true,
            lost,
            frames: 0,
            recreations: 0,
            program: None,
        })
    }

    pub(super) fn update(&mut self, points: &[[f32; 2]]) -> Result<(), String> {
        let bytes = vertex_bytes(points)?;
        self.vertices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Jadix native plot vertices"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.count = points.len() as u32;
        self.dirty = true;
        Ok(())
    }

    pub(super) fn recreate(&mut self, window: HWND) -> Result<(), String> {
        let surface = unsafe { self.instance.create_surface_unsafe(target(window)?) }
            .map_err(|error| error.to_string())?;
        if !self.adapter.is_surface_supported(&surface) {
            return Err(String::from(
                "GPU adapter no longer supports the native surface",
            ));
        }
        self.surface = surface;
        self.configuration.width = 0;
        self.configuration.height = 0;
        self.dirty = true;
        self.recreations += 1;
        Ok(())
    }

    pub(super) fn draw(&mut self, window: HWND, force: bool) -> Result<(), String> {
        if let Some(error) = self
            .lost
            .lock()
            .map_err(|_| "GPU error lock failed")?
            .as_ref()
        {
            return Err(error.clone());
        }
        let mut frame = RECT::default();
        if unsafe { GetClientRect(window, &mut frame) } == 0 {
            return Err(native_error("read GPU viewport"));
        }
        let width = (frame.right - frame.left).max(0) as u32;
        let height = (frame.bottom - frame.top).max(0) as u32;
        if width == 0 || height == 0 || unsafe { IsWindowVisible(window) } == 0 {
            self.dirty = true;
            return Ok(());
        }
        if width > self.device.limits().max_texture_dimension_2d
            || height > self.device.limits().max_texture_dimension_2d
        {
            return Err(String::from("native GPU viewport exceeds device limits"));
        }
        if width != self.configuration.width || height != self.configuration.height {
            self.configuration.width = width;
            self.configuration.height = height;
            self.surface.configure(&self.device, &self.configuration);
            self.dirty = true;
        }
        if !self.dirty && !force {
            return Ok(());
        }
        let mut recovered = false;
        let mut suboptimal = false;
        let texture = loop {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) => break texture,
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    suboptimal = true;
                    break texture;
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    self.dirty = true;
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                    if recovered =>
                {
                    return Err(String::from(
                        "native GPU surface did not recover after one retry",
                    ));
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    self.surface.configure(&self.device, &self.configuration);
                    recovered = true;
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    self.recreate(window)?;
                    self.configuration.width = width;
                    self.configuration.height = height;
                    self.surface.configure(&self.device, &self.configuration);
                    recovered = true;
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(String::from("native GPU surface validation failed"));
                }
            }
        };
        let view = texture.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Jadix native chart frame"),
            });
        self.encode(&mut encoder, &view, [width, height])?;
        self.queue.submit([encoder.finish()]);
        self.queue.present(texture);
        if suboptimal {
            self.surface.configure(&self.device, &self.configuration);
        }
        self.frames += 1;
        self.dirty = false;
        Ok(())
    }

    fn encode(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, size: [u32; 2]) -> Result<(), String> {
        if let Some(program) = &self.program { program.prepare(&self.queue, size)?; }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Jadix native chart frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.035,
                            b: 0.06,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(program) = &self.program { program.backdrop(&mut pass); }
            pass.set_pipeline(&self.pipeline);
            if let Some(program) = &self.program { program.bind(&mut pass); }
            else { pass.set_vertex_buffer(0, self.vertices.slice(..)); }
            pass.draw(0..self.count, 0..1);
        }
        Ok(())
    }

    pub(super) fn readback(&self) -> Result<([u32; 2], Vec<u8>), String> {
        self.readback_size([256, 128])
    }

    fn readback_size(&self, dimensions: [u32; 2]) -> Result<([u32; 2], Vec<u8>), String> {
        if dimensions.contains(&0) || dimensions.iter().any(|v| *v > 4096)
            || u64::from(dimensions[0]) * u64::from(dimensions[1]) > 4_194_304 {
            return Err("GPU readback exceeds the 4096-pixel / 4-megapixel budget".into());
        }
        if !matches!(
            self.configuration.format,
            wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
                | wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
        ) {
            return Err(String::from(
                "GPU plot readback requires an RGBA8 or BGRA8 surface",
            ));
        }
        let size = wgpu::Extent3d {
            width: dimensions[0],
            height: dimensions[1],
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Jadix chart diagnostic target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.configuration.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let row_bytes = (size.width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Jadix chart diagnostic pixels"),
            size: u64::from(row_bytes) * u64::from(size.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(&mut encoder, &texture.create_view(&Default::default()), [size.width, size.height])?;
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: Some(size.height),
                },
            },
            size,
        );
        let submission = self.queue.submit([encoder.finish()]);
        let (sent, received) = std::sync::mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sent.send(result);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .map_err(|error| error.to_string())?;
        received
            .recv_timeout(std::time::Duration::from_secs(1))
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        let mut pixels = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| error.to_string())?
            .chunks_exact(row_bytes as usize)
            .flat_map(|row| row[..size.width as usize * 4].iter().copied()).collect::<Vec<_>>();
        buffer.unmap();
        if matches!(
            self.configuration.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Ok((dimensions, pixels))
    }

    pub(super) fn diagnostics(&self) -> serde_json::Value {
        let adapter = self.adapter.get_info();
        serde_json::json!({"adapter": adapter.name, "backend": format!("{:?}", adapter.backend),
            "device_type": format!("{:?}", adapter.device_type), "frames": self.frames, "points": self.count,
            "size": [self.configuration.width, self.configuration.height], "surface_recreations": self.recreations,
            "dirty": self.dirty, "idle_redraw": false})
    }
}

impl WindowsHost {
    pub fn set_gpu_plot(&mut self, node: NodeId, points: &[[f32; 2]]) -> Result<(), String> {
        if self.is_closed() || !unsafe { GetParent(self.window) }.is_null() {
            return Err(String::from(
                "GPU plot currently requires a live standalone native root",
            ));
        }
        let control = self.controls.get(&node).ok_or("unknown GPU plot mount")?;
        if !matches!(control.descriptor.content, NodeContent::Container)
            || self
                .controls
                .values()
                .any(|child| child.parent == Some(node))
        {
            return Err(String::from("GPU plot requires an empty native Container"));
        }
        if let Some(plot) = self.plots.get_mut(&node) {
            plot.update(points)?;
        } else {
            if self.plots.len() >= 4 {
                return Err(String::from("native root GPU plot budget exceeded"));
            }
            let plot = Plot::new(control.window, points)?;
            self.plots.insert(node, plot);
            self.context
                .gpu_windows
                .borrow_mut()
                .insert(control.window as usize);
        }
        self.redraw_plots()
    }

    pub fn recreate_gpu_plot_surface(&mut self, node: NodeId) -> Result<(), String> {
        let window = self
            .controls
            .get(&node)
            .ok_or("unknown GPU plot mount")?
            .window;
        self.plots
            .get_mut(&node)
            .ok_or("native Container has no GPU plot")?
            .recreate(window)?;
        self.redraw_plots()
    }

    pub fn gpu_plot_diagnostics(&self, node: NodeId) -> Option<serde_json::Value> {
        self.plots.get(&node).map(Plot::diagnostics)
    }

    pub fn gpu_plot_readback(&self, node: NodeId) -> Result<([u32; 2], Vec<u8>), String> {
        self.plots
            .get(&node)
            .ok_or("native Container has no GPU plot")?
            .readback()
    }

    pub fn remove_gpu_plot(&mut self, node: NodeId) {
        self.plots.remove(&node);
        if let Some(control) = self.controls.get(&node) {
            self.context
                .gpu_windows
                .borrow_mut()
                .remove(&(control.window as usize));
            self.context
                .gpu_dirty
                .borrow_mut()
                .remove(&(control.window as usize));
            unsafe { InvalidateRect(control.window, null_mut(), 1) };
        }
    }

    pub(super) fn redraw_plots(&mut self) -> Result<(), String> {
        let dirty = std::mem::take(&mut *self.context.gpu_dirty.borrow_mut());
        for (node, plot) in &mut self.plots {
            let window = self.controls[node].window;
            plot.draw(window, dirty.contains(&(window as usize)))?;
        }
        Ok(())
    }
}
