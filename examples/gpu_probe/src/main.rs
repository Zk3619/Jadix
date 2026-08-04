//! Memory floor probe: winit + wgpu(DX12), clear-screen only, zero rui code.
//!
//! Prints working set / private commit after each initialization stage so the
//! GPU driver's contribution can be told apart from framework allocations.
//!
//! Stages can be stopped early with `PROBE_STOP=instance|adapter|device|surface`.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[cfg(windows)]
fn mem_mb() -> (f64, f64) {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            std::ptr::addr_of_mut!(counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    };
    if ok == 0 {
        return (0.0, 0.0);
    }
    const MB: f64 = 1024.0 * 1024.0;
    (
        counters.WorkingSetSize as f64 / MB,
        counters.PrivateUsage as f64 / MB,
    )
}

#[cfg(not(windows))]
fn mem_mb() -> (f64, f64) {
    (0.0, 0.0)
}

fn stage(tag: &str) {
    let (ws, pm) = mem_mb();
    println!("[probe] {tag:<28} WS={ws:7.1} MB  Private={pm:7.1} MB");
}

fn stop_after() -> String {
    std::env::var("PROBE_STOP").unwrap_or_else(|_| "all".into())
}

fn selected_backends() -> wgpu::Backends {
    match std::env::var("PROBE_BACKEND")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "vulkan" | "vk" => wgpu::Backends::VULKAN,
        "gl" | "gles" | "opengl" => wgpu::Backends::GL,
        "dx12" => wgpu::Backends::DX12,
        _ if cfg!(windows) => wgpu::Backends::DX12,
        _ => wgpu::Backends::PRIMARY,
    }
}

struct Probe {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    frames: u32,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        stage("start");

        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("gpu_probe"))
                .expect("create window"),
        );
        stage("after window");

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: selected_backends(),
            ..Default::default()
        });
        stage("after wgpu::Instance");
        if stop_after() == "instance" {
            self.window = Some(window);
            return;
        }

        let surface = instance.create_surface(window.clone()).expect("surface");
        stage("after create_surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("adapter");
        println!("[probe] adapter: {:?}", adapter.get_info());
        stage("after request_adapter");
        if stop_after() == "adapter" {
            self.window = Some(window);
            return;
        }

        let mut limits = wgpu::Limits::downlevel_defaults();
        let adapter_limits = adapter.limits();
        limits.max_texture_dimension_1d = adapter_limits.max_texture_dimension_1d;
        limits.max_texture_dimension_2d = adapter_limits.max_texture_dimension_2d;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                label: None,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        ))
        .expect("device");
        stage("after request_device");
        if stop_after() == "device" {
            self.window = Some(window);
            return;
        }

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 1,
            },
        );
        stage("after surface.configure");

        self.window = Some(window);
        self.gpu = Some(Gpu {
            surface,
            device,
            queue,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let Some(gpu) = self.gpu.as_ref() else {
                    return;
                };
                let Ok(frame) = gpu.surface.get_current_texture() else {
                    return;
                };
                let view = frame.texture.create_view(&Default::default());
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }
                gpu.queue.submit(Some(encoder.finish()));
                frame.present();

                self.frames += 1;
                if matches!(self.frames, 1 | 60 | 300) {
                    stage(&format!("after frame {}", self.frames));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = Probe {
        window: None,
        gpu: None,
        frames: 0,
    };
    event_loop.run_app(&mut app).expect("run");
}
