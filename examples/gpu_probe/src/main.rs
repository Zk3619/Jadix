//! Memory floor probe: winit + wgpu clear-screen only, zero jadix code.
//!
//! Prints working set / private commit after each initialization stage so the
//! GPU driver's contribution can be told apart from framework allocations.
//!
//! Stages can be stopped early with `PROBE_STOP=instance|adapter|device|surface`.
//! By default the window clears `PROBE_FRAMES` times (default 300) then exits.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake};

    struct NoopWaker;
    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
        fn wake_by_ref(self: &Arc<Self>) {}
    }

    let waker = Arc::new(NoopWaker).into();
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

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

fn target_frames() -> u32 {
    std::env::var("PROBE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(300)
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
        // ADR-3 / jadix default: GL on Windows for lower driver private bytes.
        _ if cfg!(windows) => wgpu::Backends::GL,
        _ => wgpu::Backends::PRIMARY,
    }
}

struct Probe {
    window: Option<Arc<dyn Window>>,
    gpu: Option<Gpu>,
    frames: u32,
    done: bool,
}

/// Keep `Instance` alive for the whole surface lifetime (same as `jadix` AppState).
struct Gpu {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Probe {
    fn finish_probe(&mut self, event_loop: &dyn ActiveEventLoop, tag: &str) {
        stage(tag);
        self.done = true;
        event_loop.exit();
    }

    fn start_if_needed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        stage("start");

        let window: Arc<dyn Window> = Arc::from(
            event_loop
                .create_window(WindowAttributes::default().with_title("gpu_probe"))
                .expect("create window"),
        );
        stage("after window");

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: selected_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        stage("after wgpu::Instance");
        if stop_after() == "instance" {
            self.window = Some(window);
            self.finish_probe(event_loop, "probe stop=instance");
            return;
        }

        let surface = instance.create_surface(window.clone()).expect("surface");
        stage("after create_surface");
        if stop_after() == "surface" {
            self.window = Some(window);
            self.finish_probe(event_loop, "probe stop=surface");
            return;
        }

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("adapter");
        println!("[probe] adapter: {:?}", adapter.get_info());
        stage("after request_adapter");
        if stop_after() == "adapter" {
            self.window = Some(window);
            self.finish_probe(event_loop, "probe stop=adapter");
            return;
        }

        let mut limits = wgpu::Limits::downlevel_defaults();
        let adapter_limits = adapter.limits();
        limits.max_texture_dimension_1d = adapter_limits.max_texture_dimension_1d;
        limits.max_texture_dimension_2d = adapter_limits.max_texture_dimension_2d;

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            label: None,
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            experimental_features: wgpu::ExperimentalFeatures::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("device");
        stage("after request_device");
        if stop_after() == "device" {
            self.window = Some(window);
            self.finish_probe(event_loop, "probe stop=device");
            return;
        }

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let size = window.surface_size();
        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: caps.alpha_modes[0],
                color_space: wgpu::SurfaceColorSpace::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 1,
            },
        );
        stage("after surface.configure");

        self.window = Some(window);
        self.gpu = Some(Gpu {
            _instance: instance,
            surface,
            device,
            queue,
        });
    }
}

impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.start_if_needed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let Some(gpu) = self.gpu.as_ref() else {
                    return;
                };
                let frame = match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Timeout => return,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Occluded => return,
                    wgpu::CurrentSurfaceTexture::Validation => return,
                };
                let view = frame.texture.create_view(&Default::default());
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                }
                gpu.queue.submit(Some(encoder.finish()));
                gpu.queue.present(frame);

                self.frames += 1;
                if matches!(self.frames, 1 | 60 | 300) {
                    stage(&format!("after frame {}", self.frames));
                }
                if self.frames >= target_frames() {
                    self.finish_probe(event_loop, "probe complete");
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.done {
            return;
        }
        if self.gpu.is_some() {
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        } else if self.window.is_some() {
            // Early PROBE_STOP without exit (should not happen) — avoid a Poll hot loop.
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let app = Probe {
        window: None,
        gpu: None,
        frames: 0,
        done: false,
    };
    event_loop.run_app(app).expect("run");
}
