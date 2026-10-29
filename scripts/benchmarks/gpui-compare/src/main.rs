#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use gpui_kit::*;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[path = "../gpu_identity.rs"]
mod gpu_identity;
#[path = "../workload.rs"]
mod workload;

struct Telemetry {
    start: Instant,
    start_unix: f64,
    frames: Vec<(f64, f32, u64)>,
    wheels: usize,
    renderer: Option<serde_json::Value>,
}

struct Bench {
    scene: String,
    scroll: ScrollHandle,
    telemetry: Rc<RefCell<Telemetry>>,
}

impl Render for Bench {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.telemetry.borrow().renderer.is_none() {
            let gpu = window.gpu_specs().expect("selected GPU identity");
            assert!(!gpu.is_software_emulated, "hardware GPU is required");
            self.telemetry.borrow_mut().renderer = Some(serde_json::json!({
                "backend": "Dx11", "implementation": "native",
                "device_name": gpu.device_name, "driver": gpu.driver_name,
                "driver_info": gpu.driver_info, "device_type": "Hardware",
                "dxgi": gpu_identity::dxgi_identity(&gpu.device_name),
            }));
        }
        let rows = (0..workload::rows(&self.scene)).map(|index| {
            let [number, title, status] = workload::labels(index);
            let mut row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(16.))
                .w_full()
                .h(px(workload::ROW_HEIGHT))
                .flex_shrink_0()
                .px(px(16.))
                .border_1()
                .border_color(rgb(workload::BORDER))
                .child(div().w(px(80.)).flex_shrink_0().child(number))
                .child(div().flex_1().child(title))
                .child(status);
            if self.scene == "nested" {
                for _ in 0..8 {
                    row = div()
                        .w_full()
                        .h(px(workload::ROW_HEIGHT))
                        .flex_shrink_0()
                        .child(row);
                }
            }
            row
        });
        let telemetry = self.telemetry.clone();
        let scroll = self.scroll.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(workload::BACKGROUND))
            .text_color(rgb(workload::FOREGROUND))
            .font_family(workload::FONT)
            .text_size(px(workload::FONT_SIZE))
            .line_height(px(workload::LINE_HEIGHT))
            .child(
                div()
                    .h(px(workload::HEADER_HEIGHT))
                    .flex_shrink_0()
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .child(workload::HEADER),
            )
            .child(
                div()
                    .id("scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(rows),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, (), window, _| {
                        let mut data = telemetry.borrow_mut();
                        let t = data.start.elapsed().as_secs_f64();
                        let count = data.frames.len() as u64;
                        data.frames.push((t, f32::from(scroll.offset().y), count));
                        drop(data);
                        let telemetry = telemetry.clone();
                        window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, _| {
                            if phase == DispatchPhase::Capture {
                                telemetry.borrow_mut().wheels += 1;
                            }
                        });
                    },
                )
                .absolute()
                .size(px(0.)),
            )
    }
}

fn main() {
    let telemetry = Rc::new(RefCell::new(Telemetry {
        start: Instant::now(),
        start_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64(),
        frames: Vec::new(),
        wheels: 0,
        renderer: None,
    }));
    let output = std::env::var_os("JADIX_CPU_TELEMETRY").expect("telemetry output");
    let data = telemetry.clone();
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(800.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Frame comparison".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            |_, cx| {
                cx.new(|_| Bench {
                    scene: workload::scene(),
                    scroll: ScrollHandle::new(),
                    telemetry: data.clone(),
                })
            },
        )
        .expect("open benchmark window");
        cx.activate(true);
    });
    let data = telemetry.borrow();
    std::fs::write(
        output,
        serde_json::to_vec(&serde_json::json!({
            "start_unix": data.start_unix, "wheel_events": data.wheels,
            "redraws": data.frames, "frame_counter_kind": "paint_callbacks",
            "workload": workload::scene(), "rows": workload::rows(&workload::scene()),
            "font": workload::FONT, "font_size": workload::FONT_SIZE,
            "renderer": data.renderer,
        }))
        .unwrap(),
    )
    .expect("write telemetry");
}
