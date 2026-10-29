#[cfg(feature = "devtools")]
use super::devtools::*;
use crate::core::tree::Element;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

include!("native_acceptance_scene.rs");
include!("native_acceptance_state.rs");
include!("native_acceptance_matrix.rs");
include!("native_acceptance_sample.rs");
#[cfg(feature = "devtools")]
include!("native_acceptance_recovery.rs");
const ID: &str = "source.rs:1:13";

#[cfg(not(feature = "devtools"))]
struct NativeNode {
    instance_id: u64,
    text_preview: Option<String>,
    template_id: Option<String>,
    layout_bounds: [f32; 4],
    style: String,
}

#[cfg(not(feature = "devtools"))]
fn snapshot_tree(root: &Element) -> Vec<NativeNode> {
    fn collect(element: &Element, nodes: &mut Vec<NativeNode>) {
        nodes.push(NativeNode {
            instance_id: element.inspector_instance_id,
            text_preview: element.text.clone(),
            template_id: element.template_id.clone(),
            layout_bounds: element.layout_bounds,
            style: format!("{:?}", element.style),
        });
        for child in &element.children {
            collect(child, nodes);
        }
    }
    let mut nodes = Vec::new();
    collect(root, &mut nodes);
    nodes
}

fn body(mode: &str) -> &'static str {
    if mode.contains("stateful") {
        STATEFUL_BODY
    } else {
        BODY
    }
}

fn construct(mode: &str) -> Element {
    if mode.contains("matrix") {
        return compiled_matrix_scene().template_id(ID);
    }
    if mode.contains("stateful") {
        #[cfg(feature = "macros")]
        if !mode.contains("registered") {
            return compiled_stateful_static_scene().template_id(ID);
        }
        return registered_stateful_scene().template_id(ID);
    }
    #[cfg(feature = "macros")]
    if mode == "static" || mode == "cold-static" {
        return compiled_static_scene().template_id(ID);
    }
    let element = compiled_builder_scene();
    if mode == "unregistered" {
        return element;
    }
    let mut description = jadix_hot_reload::TemplateDescription::new(
        jadix_hot_reload::syntax::template_signature(BODY).unwrap(),
        jadix_hot_reload::syntax::declarations_signature(BODY).unwrap(),
    );
    description.tree = jadix_hot_reload::syntax::template_tree(BODY);
    element.template_id(ID).hot_reload_description(description)
}

struct Watcher(std::process::Child);

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Harness {
    mode: String,
    app: Option<crate::app::App>,
    directory: PathBuf,
    saved: Option<Instant>,
    started: Instant,
    step: usize,
    samples: Vec<f64>,
    instance: Option<u64>,
    pending_layout_reported: bool,
    registry_baseline: Option<crate::core::registry_stats::RegistryStats>,
    memory_samples: Vec<f64>,
    edits: usize,
    redraw_events: usize,
    idle_started: Option<Instant>,
    idle_measurement: Option<(Instant, usize)>,
    state_expected: Option<serde_json::Value>,
    animation_samples: Vec<f32>,
    async_wait_started: Option<Instant>,
    matrix_instances: Vec<u64>,
    recovery_errors: usize,
    pending_sample: Option<(String, Instant)>,
}

fn semantics(element: &Element) -> serde_json::Value {
    serde_json::json!({"text":element.text,"bounds":element.layout_bounds,
        "style":format!("{:?}",element.style),
        "text_style":format!("{:?}",element.text_style),
        "click_handler":element.on_click.is_some(),
        "focus_id":element.focus_id,
        "a11y":element.attrs.iter().filter(|(name,_)| name.starts_with("aria-") || name.starts_with("accesskit_")).collect::<std::collections::BTreeMap<_,_>>(),
        "children":element.children.iter().map(semantics).collect::<Vec<_>>()})
}

impl Harness {
    fn capture_semantics(&self, name: &str) {
        let state = self.app.as_ref().unwrap().state.borrow();
        let state = state.as_ref().unwrap();
        std::fs::write(
            self.directory.join(name),
            serde_json::to_vec_pretty(&semantics(&state.root_element)).unwrap(),
        )
        .unwrap();
        let info = state.adapter.get_info();
        std::fs::write(self.directory.join("device.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "name":info.name,"vendor":info.vendor,"device":info.device,"backend":format!("{:?}",info.backend),
            "driver":info.driver,"driver_info":info.driver_info,"scale_factor":state.window.scale_factor(),
            "surface_format":format!("{:?}",state.config.format),"physical_size":[state.config.width,state.config.height],
            "capture_encoding":"rgba8",
            "a11y_feature":cfg!(feature="a11y"),"macros_feature":cfg!(feature="macros"),
            "hot_reload_feature":cfg!(feature="hot-reload"),"devtools_feature":cfg!(feature="devtools")
        })).unwrap()).unwrap();
        #[cfg(feature = "a11y")]
        std::fs::write(
            self.directory.join(name.replace("semantics", "accesskit")),
            format!("{:#?}", state.accesskit_tree_update()),
        )
        .unwrap();
    }
    fn capture(&self, name: &str) {
        let state = self.app.as_ref().unwrap().state.borrow();
        let state = state.as_ref().unwrap();
        state.capture_hr6_native_frame(&self.directory.join(name));
    }

    fn drive(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.pending_sample.is_some() {
            self.drive_sample(event_loop);
            return;
        }
        #[cfg(feature = "devtools")]
        if self.mode == "recovery" {
            self.drive_recovery(event_loop);
            return;
        }
        if self.async_wait_started.is_some() {
            self.finish_async(event_loop);
            return;
        }
        if self.mode == "deferred" {
            self.drive_deferred(event_loop);
            return;
        }
        if !self.matrix_ready() {
            return;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(60 + self.edits as u64 * 2),
            "native watcher timeout; inspect watcher.log"
        );
        let app = self.app.as_ref().unwrap();
        let state_guard = app.state.borrow();
        let state = state_guard.as_ref().unwrap();
        let nodes = snapshot_tree(&state.root_element);
        if self.mode.contains("stateful") && self.started.elapsed() < Duration::from_secs(3) {
            return;
        }
        if self.mode == "idle" {
            drop(state_guard);
            if self.idle_started.is_none() {
                self.idle_started = Some(Instant::now());
                let proxy = event_loop.create_proxy();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(2));
                    proxy.wake_up();
                });
            } else if self.idle_started.unwrap().elapsed() >= Duration::from_secs(2) {
                if let Some((started, redraws)) = self.idle_measurement {
                    if started.elapsed() >= Duration::from_secs(10) {
                        let extra = self.redraw_events - redraws;
                        assert_eq!(extra, 0, "static idle must not request continuous redraws");
                        let result = serde_json::json!({"passed":true,"scope":"native production Wait control flow after 2s warmup; one timer wake terminates the 10s measurement; no harness polling","idle_seconds":started.elapsed().as_secs_f64(),"redraw_events":extra,"pid":std::process::id()});
                        std::fs::write(
                            self.directory.join("result.json"),
                            serde_json::to_vec_pretty(&result).unwrap(),
                        )
                        .unwrap();
                        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now()));
                        event_loop.exit();
                    }
                } else {
                    self.idle_measurement = Some((Instant::now(), self.redraw_events));
                    std::fs::write(self.directory.join("idle-start.json"), b"{}").unwrap();
                    let proxy = event_loop.create_proxy();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(10));
                        proxy.wake_up();
                    });
                }
            }
            return;
        }
        if self.mode == "unregistered" {
            assert!(nodes
                .iter()
                .any(|node| node.text_preview.as_deref() == Some("HR6 A")));
            assert!(nodes
                .iter()
                .all(|node| node.template_id.as_deref() != Some(ID)));
            drop(state_guard);
            self.capture("A-native.png");
            self.capture_semantics("A-semantics.json");
            std::fs::write(self.directory.join("result.json"), r#"{"passed":true,"scope":"native unregistered Builder inspector snapshot; no source registration or hot reload claim"}"#).unwrap();
            event_loop.exit();
            return;
        }
        let node = nodes
            .iter()
            .find(|node| node.template_id.as_deref() == Some(ID))
            .expect("registered source node");
        if self.mode.starts_with("cold-") {
            assert_eq!(node.layout_bounds[2], 128.0);
            assert!(nodes
                .iter()
                .any(|node| node.text_preview.as_deref() == Some("HR6 B30")));
            drop(state_guard);
            self.schedule_sample(event_loop, "C");
            return;
        }
        if let Some(instance) = self.instance {
            assert_eq!(
                node.instance_id, instance,
                "source edits must retain runtime identity"
            );
        } else {
            self.instance = Some(node.instance_id);
        }
        if let Some(saved) = self.saved {
            let expected = format!("HR6 B{}", self.step);
            if !nodes
                .iter()
                .any(|node| node.text_preview.as_deref() == Some(&expected))
            {
                return;
            }
            let expected_width = if self.step % 2 == 0 { 128.0 } else { 96.0 };
            if node.layout_bounds[2] != expected_width {
                if !self.pending_layout_reported {
                    println!(
                        "HR6_LAYOUT_WAIT {}",
                        serde_json::json!({"step":self.step,"bounds":node.layout_bounds,"style":format!("{:?}",node.style)})
                    );
                    self.pending_layout_reported = true;
                }
                return;
            }
            #[cfg(feature = "devtools")]
            let changes = DevToolsState::changes();
            #[cfg(not(feature = "devtools"))]
            let changes: Vec<(String, String)> = Vec::new();
            #[cfg(feature = "devtools")]
            assert!(changes.iter().any(|(kind, _)| kind == "Sources"));
            #[cfg(feature = "devtools")]
            assert!(changes
                .iter()
                .any(|(kind, detail)| kind == "HMR commit" && detail.contains("applied")));
            if self.mode.contains("stateful") {
                assert_eq!(
                    Some(&probe_snapshot(&state.root_element)),
                    self.state_expected.as_ref()
                );
            }
            if self.mode.contains("matrix") {
                let current = ["hr6-first", "hr6-last", "hr6-badge"]
                    .iter()
                    .map(|id| {
                        nodes
                            .iter()
                            .find(|node| node.template_id.as_deref() == Some(id))
                            .unwrap()
                            .instance_id
                    })
                    .collect::<Vec<_>>();
                if self.matrix_instances.is_empty() {
                    self.matrix_instances = current;
                } else {
                    assert_eq!(self.matrix_instances, current);
                }
                BADGE_BUILDS.with(|builds| {
                    assert_eq!(
                        builds.get(),
                        1,
                        "keyed factory must not run again during reorder/type changes"
                    )
                });
            }
            let stats = crate::core::registry_stats::RegistryStats::collect();
            if let Some(baseline) = &self.registry_baseline {
                assert_eq!(
                    &stats, baseline,
                    "registrations must converge after each source edit"
                );
            } else {
                self.registry_baseline = Some(stats);
            }
            self.memory_samples
                .push(crate::core::tree_stats::process_private_mb());
            if self.mode.contains("stateful") {
                let value = self.animation_value();
                self.animation_samples.push(value);
            }
            self.samples.push(saved.elapsed().as_secs_f64() * 1000.0);
            println!(
                "HR6_SAMPLE {}",
                serde_json::json!({"step":self.step,"save_to_observed_present_ms":self.samples.last(),"instance":node.instance_id,"bounds":node.layout_bounds,"style":format!("{:?}",node.style),"changes":changes})
            );
            self.saved = None;
            self.step += 1;
            if self.step == self.edits {
                drop(state_guard);
                self.schedule_sample(event_loop, "B");
                return;
            }
        }
        drop(state_guard);
        if self.step == 0 && self.saved.is_none() {
            self.capture("A-native.png");
            self.capture_semantics("A-semantics.json");
            if self.mode.contains("stateful") && self.state_expected.is_none() {
                self.arm_state(event_loop);
            }
        }
        let width = if self.step % 2 == 0 { "128.0" } else { "96.0" };
        let source = if self.mode.contains("matrix") {
            MATRIX_UPDATED
                .replace("128.0", "96.0")
                .replace("HR6 B30", "HR6 A")
                .replace(
                    "VStack",
                    if self.step % 2 == 0 {
                        "VStack"
                    } else {
                        "GStack"
                    },
                )
        } else {
            body(&self.mode).into()
        };
        let body = source
            .replace("96.0", width)
            .replace("HR6 A", &format!("HR6 B{}", self.step));
        let updated = format!("fn demo() {{ ui! {{ {body} }} }}\n");
        self.saved = Some(Instant::now());
        self.pending_layout_reported = false;
        #[cfg(feature = "devtools")]
        hr6_save(updated);
        #[cfg(not(feature = "devtools"))]
        std::fs::write(
            PathBuf::from(std::env::var_os("JADIX_DEVTOOLS_ROOT").unwrap()).join("source.rs"),
            updated,
        )
        .unwrap();
    }
}

impl ApplicationHandler for Harness {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        crate::init();
        let (sender, receiver) = std::sync::mpsc::channel();
        crate::app::ui_bridge::install_proxy(event_loop.create_proxy(), sender);
        let mode = self.mode.clone();
        if mode.contains("matrix") {
            crate::core::hot_reload::register_factory("HrNativeBadge", 1, badge_factory).unwrap();
        }
        let mut app = crate::app::lifecycle::create_app(
            crate::app::WindowOptions::new().size(640.0, 480.0),
            move || construct_root(&mode),
            event_loop.create_proxy(),
            receiver,
        );
        crate::app::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
        #[cfg(feature = "devtools")]
        {
            DevToolsState::clear_changes();
            if !super::devtools::hr6_writes_enabled() {
                super::devtools::hr6_enable_writes();
            }
        }
        self.app = Some(app);
        if self.mode.contains("multi") {
            super::multi_window::open_secondary_window_with(
                super::WindowOptions::new()
                    .title("HR6 secondary")
                    .size(320.0, 240.0),
                || construct_root("registered"),
            )
            .unwrap();
        }
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
        if self.mode.contains("stateful")
            && matches!(
                &event,
                WindowEvent::Focused(_) | WindowEvent::Ime(_) | WindowEvent::PointerButton { .. }
            )
        {
            println!(
                "HR6_NATIVE_EVENT {:.3} step={} {:?}",
                self.started.elapsed().as_secs_f64(),
                self.step,
                event
            );
        }
        if matches!(&event, WindowEvent::RedrawRequested) {
            self.redraw_events += 1;
        }
        self.app
            .as_mut()
            .unwrap()
            .window_event(event_loop, window, event);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.as_mut().unwrap().about_to_wait(event_loop);
        self.drive(event_loop);
        if self.mode == "idle" {
            return;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(10),
        ));
    }
}

fn native(mode: &str) {
    let directory =
        PathBuf::from(std::env::var_os("JADIX_HR6_EVIDENCE").expect("use check-hr6-native.py"));
    let root = PathBuf::from(std::env::var_os("JADIX_DEVTOOLS_ROOT").unwrap());
    let log = std::fs::File::create(directory.join("watcher.log")).unwrap();
    let child = std::process::Command::new(std::env::var_os("JADIX_HR6_CLI").unwrap())
        .args([
            "--root",
            root.to_str().unwrap(),
            "--addr",
            &std::env::var("JADIX_HOT_RELOAD_ADDR").unwrap(),
        ])
        .current_dir(&root)
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let _watcher = Watcher(child);
    let ready = Instant::now();
    while !std::fs::read_to_string(directory.join("watcher.log"))
        .unwrap()
        .contains("watching")
    {
        assert!(ready.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder
        .build()
        .unwrap()
        .run_app(Harness {
            mode: mode.into(),
            app: None,
            directory,
            saved: None,
            started: Instant::now(),
            step: 0,
            samples: Vec::new(),
            instance: None,
            pending_layout_reported: false,
            registry_baseline: None,
            memory_samples: Vec::new(),
            redraw_events: 0,
            idle_started: None,
            idle_measurement: None,
            state_expected: None,
            animation_samples: Vec::new(),
            async_wait_started: None,
            matrix_instances: Vec::new(),
            recovery_errors: 0,
            pending_sample: None,
            edits: std::env::var("JADIX_HR6_EDITS")
                .ok()
                .map(|value| value.parse().unwrap())
                .unwrap_or(31),
        })
        .unwrap();
    crate::app::runtime::clear_app_runtime();
    crate::app::reactive::reset_registered_anchors();
}

#[test]
#[ignore = "requires Windows native GPU and the HR6 evidence runner"]
fn native_unregistered_builder_inspector() {
    native("unregistered");
}

#[test]
#[ignore = "requires Windows native GPU and the real hot-reload watcher"]
fn native_registered_builder_sources_watcher() {
    native("registered");
}

#[test]
#[ignore = "requires Windows native GPU and the real hot-reload watcher"]
fn native_static_ui_sources_watcher() {
    native("static");
}

#[test]
#[ignore = "requires the modified Rust fixture to have been recompiled"]
fn native_recompiled_static() {
    native("cold-static");
}

#[test]
#[ignore = "requires the modified Rust fixture to have been recompiled"]
fn native_recompiled_builder() {
    native("cold-registered");
}

#[test]
#[ignore = "requires an undisturbed native window for the 10s idle interval"]
fn native_idle_static() {
    native("idle");
}

#[test]
#[ignore = "requires the native HR6 state and source watcher runner"]
fn native_stateful_static() {
    native("stateful");
}

#[test]
#[ignore = "requires the native HR6 state and source watcher runner"]
fn native_stateful_registered() {
    native("registered-stateful");
}

#[test]
#[ignore = "requires actual recompilation of the native stateful Rust scene"]
fn native_recompiled_stateful_static() {
    native("cold-stateful");
}

#[test]
#[ignore = "requires actual recompilation of the native stateful Rust scene"]
fn native_recompiled_stateful_registered() {
    native("cold-registered-stateful");
}
