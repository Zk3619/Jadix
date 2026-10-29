use crate::core::tree::Element;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ButtonSource, ElementState, MouseButton, PointerSource, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

fn scene(revision: usize, offset: f32, translation: f32) -> Element {
    let mut rows = Element::new().w(600.0).flex_col();
    for index in 0..120 {
        rows = rows.child(
            Element::new()
                .w(580.0)
                .h(24.0)
                .flex_shrink(0.0)
                .bg(if index % 2 == 0 { "#eeeeee" } else { "#ffffff" })
                .text(format!("HR6 native row {index:03}: {revision:04}")),
        );
    }
    let viewport = crate::core::scroll_viewport::activate_scroll_viewport(
        Element::new()
            .w(620.0)
            .h(460.0)
            .translate(translation, 0.0)
            .child(rows),
        "hr6-cost-scroll",
        "hidden",
        "scroll",
        "jump",
        "scrollbar",
    );
    let _ = offset;
    Element::new()
        .w(640.0)
        .h(480.0)
        .bg("#ffffff")
        .child(viewport)
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn GetProcessTimes(
        process: *mut std::ffi::c_void,
        creation: *mut u64,
        exit: *mut u64,
        kernel: *mut u64,
        user: *mut u64,
    ) -> i32;
}

fn cpu_ms() -> f64 {
    let (mut creation, mut exit, mut kernel, mut user) = (0_u64, 0_u64, 0_u64, 0_u64);
    assert_ne!(
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        },
        0
    );
    (kernel + user) as f64 / 10_000.0
}

struct PendingFrame {
    started: Instant,
    update_ms: f64,
}

struct Harness {
    app: Option<super::App>,
    directory: PathBuf,
    started: Instant,
    first_present_ms: Option<f64>,
    phase: usize,
    frame: usize,
    samples: Vec<Vec<f64>>,
    latencies: Vec<Vec<f64>>,
    phase_cpu_ms: Vec<f64>,
    phase_started_cpu: Option<f64>,
    private_mib: Vec<f64>,
    pending: Option<PendingFrame>,
    frames: usize,
    adapter: Option<serde_json::Value>,
}

impl Harness {
    fn next_frame(&mut self, event_loop: &dyn ActiveEventLoop) {
        let names = [
            "warmup",
            "pointer_input",
            "scroll",
            "animation",
            "list_update",
        ];
        if self.phase == names.len() {
            let result = serde_json::json!({
                "schema": 2,
                "cfg_test":cfg!(test),"panic_abort":cfg!(panic="abort"),
                "first_present_ms": self.first_present_ms.unwrap(),
                "adapter": self.adapter,
                "debug_assertions": cfg!(debug_assertions),
                "hot_reload_feature": cfg!(feature = "hot-reload"),
                "devtools_feature": cfg!(feature = "devtools"),
                "window_logical": [640, 480],
                "rows": 120,
                "frames_per_phase": self.frames,
                "measurement": "actual native presented frames only; UI update plus redraw excludes event-loop waiting; process CPU and end-to-end latency separate",
                "phase_cpu_ms":self.phase_cpu_ms,"private_mib":self.private_mib,"present_latency_ms":self.latencies,
                "phases": names.iter().zip(&self.samples).map(|(name, samples)| {
                    serde_json::json!({"name":name,"milliseconds":samples})
                }).collect::<Vec<_>>()
            });
            std::fs::write(
                self.directory.join("native-cost.json"),
                serde_json::to_vec_pretty(&result).unwrap(),
            )
            .unwrap();
            event_loop.exit();
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now()));
            return;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(240),
            "native measurement timeout"
        );
        if self.pending.is_some() || self.first_present_ms.is_none() {
            return;
        }
        if self.phase_started_cpu.is_none() {
            self.phase_started_cpu = Some(cpu_ms());
        }
        let app = self.app.as_mut().unwrap();
        let window = {
            let state = app.state.borrow();
            let state = state.as_ref().unwrap();
            state.window.id()
        };
        let started = Instant::now();
        match self.phase {
            1 => {
                let position =
                    winit::dpi::PhysicalPosition::new(80.0 + (self.frame % 100) as f64, 80.0);
                app.window_event(
                    event_loop,
                    window,
                    WindowEvent::PointerMoved {
                        device_id: None,
                        position,
                        primary: true,
                        source: PointerSource::Mouse,
                    },
                );
                for state in [ElementState::Pressed, ElementState::Released] {
                    app.window_event(
                        event_loop,
                        window,
                        WindowEvent::PointerButton {
                            device_id: None,
                            position,
                            primary: true,
                            button: ButtonSource::Mouse(MouseButton::Left),
                            state,
                            is_macos_activation_click: false,
                        },
                    );
                }
            }
            2 => {
                use crate::core::scroll_viewport::{
                    scroll_offset_y_signal_key, scroll_target_y_signal_key,
                };
                let offset = (self.frame % 100) as f32 * 4.0;
                crate::core::reactivity::use_signal(
                    &scroll_offset_y_signal_key("hr6-cost-scroll"),
                    || 0.0_f32,
                )
                .set_silent(offset);
                crate::core::reactivity::use_signal(
                    &scroll_target_y_signal_key("hr6-cost-scroll"),
                    || 0.0_f32,
                )
                .set_silent(offset);
                app.state
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .update_scroll_offset_in_place(
                        "hr6-cost-scroll",
                        [0.0, (self.frame % 100) as f32 * 4.0],
                    );
            }
            3 | 4 => {
                let revision = if self.phase == 4 { self.frame } else { 0 };
                let translation = if self.phase == 3 {
                    (self.frame % 10) as f32
                } else {
                    0.0
                };
                app.state.borrow_mut().as_mut().unwrap().rebuild_ui(scene(
                    revision,
                    0.0,
                    translation,
                ));
            }
            _ => {}
        }
        let mut state = app.state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.mark_full_redraw();
        state.force_present = true;
        state.window.request_redraw();
        self.pending = Some(PendingFrame {
            started,
            update_ms: started.elapsed().as_secs_f64() * 1000.0,
        });
    }
}

impl ApplicationHandler for Harness {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        crate::init();
        let (sender, receiver) = std::sync::mpsc::channel();
        super::ui_bridge::install_proxy(event_loop.create_proxy(), sender);
        let mut app = super::lifecycle::create_app(
            super::WindowOptions::new().size(640.0, 480.0),
            || scene(0, 0.0, 0.0),
            event_loop.create_proxy(),
            receiver,
        );
        super::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
        {
            let state = app.state.borrow();
            let state = state.as_ref().unwrap();
            let info = state.adapter.get_info();
            self.adapter = Some(serde_json::json!({
                "name":info.name,"vendor":info.vendor,"device":info.device,
                "device_type":format!("{:?}",info.device_type),
                "backend":format!("{:?}",info.backend),"driver":info.driver,
                "driver_info":info.driver_info,"scale_factor":state.window.scale_factor(),
                "surface_format":format!("{:?}",state.config.format),
                "present_mode":format!("{:?}",state.config.present_mode),
                "physical_size":[state.config.width,state.config.height],
            }));
        }
        self.app = Some(app);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.as_mut().unwrap().proxy_wake_up(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window: WindowId,
        event: WindowEvent,
    ) {
        let redraw = matches!(&event, WindowEvent::RedrawRequested);
        if redraw {
            if let Some(state) = self.app.as_ref().unwrap().state.borrow_mut().as_mut() {
                state.gpu_presented_this_frame = false;
            }
        }
        let started = Instant::now();
        self.app
            .as_mut()
            .unwrap()
            .window_event(event_loop, window, event);
        let render_ms = started.elapsed().as_secs_f64() * 1000.0;
        if !redraw {
            return;
        }
        let presented = self
            .app
            .as_ref()
            .unwrap()
            .state
            .borrow()
            .as_ref()
            .map(|state| state.gpu_presented_this_frame)
            .unwrap_or(false);
        if !presented {
            return;
        }
        if self.first_present_ms.is_none() {
            self.first_present_ms = Some(self.started.elapsed().as_secs_f64() * 1000.0);
        }
        if let Some(pending) = self.pending.take() {
            self.samples[self.phase].push(pending.update_ms + render_ms);
            self.latencies[self.phase].push(pending.started.elapsed().as_secs_f64() * 1000.0);
            self.frame += 1;
            if self.frame == self.frames {
                self.phase_cpu_ms
                    .push(cpu_ms() - self.phase_started_cpu.take().unwrap());
                self.private_mib
                    .push(crate::core::tree_stats::process_private_mb());
                self.phase += 1;
                self.frame = 0;
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.as_mut().unwrap().about_to_wait(event_loop);
        self.next_frame(event_loop);
    }
}

#[test]
#[ignore = "requires a native Windows GPU and an isolated HR6 performance run"]
fn native_release_cost() {
    let directory = PathBuf::from(
        std::env::var_os("JADIX_HR6_COST_EVIDENCE").expect("set JADIX_HR6_COST_EVIDENCE"),
    );
    let frames = std::env::var("JADIX_HR6_COST_FRAMES")
        .ok()
        .map(|value| value.parse().unwrap())
        .unwrap_or(300);
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder
        .build()
        .unwrap()
        .run_app(Harness {
            app: None,
            directory,
            started: Instant::now(),
            first_present_ms: None,
            phase: 0,
            frame: 0,
            samples: vec![Vec::new(); 5],
            latencies: vec![Vec::new(); 5],
            phase_cpu_ms: Vec::new(),
            phase_started_cpu: None,
            private_mib: Vec::new(),
            pending: None,
            frames,
            adapter: None,
        })
        .unwrap();
    super::runtime::clear_app_runtime();
    super::reactive::reset_registered_anchors();
}
