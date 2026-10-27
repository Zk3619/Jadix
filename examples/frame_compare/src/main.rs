#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use jadix_components::prelude::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use winit::{event::WindowEvent, window::Window};

#[cfg(all(windows, feature = "backend-dx11-angle"))]
#[path = "../../../scripts/gpui-compare/angle_identity.rs"]
mod angle_identity;
#[path = "../../../scripts/gpui-compare/gpu_identity.rs"]
mod gpu_identity;
#[path = "../../../scripts/gpui-compare/workload.rs"]
mod workload;

struct Telemetry {
    output: PathBuf,
    start: Instant,
    start_unix: f64,
    frames: Mutex<Vec<(f64, f32, u64)>>,
    wheels: std::sync::atomic::AtomicUsize,
    renderer: Mutex<Option<serde_json::Value>>,
}

impl jadix::app::WindowLifecycleHook for Telemetry {
    fn on_created(&self, _: &dyn Window, _: &str) {}
    fn on_closing(&self, _: &dyn Window) {}
    fn on_gpu_ready(
        &self,
        _: &dyn Window,
        adapter: &wgpu::Adapter,
        surface: &wgpu::SurfaceConfiguration,
    ) {
        let info = adapter.get_info();
        #[cfg(all(windows, feature = "backend-dx11-angle"))]
        let dxgi = if info.backend == wgpu::Backend::Gl {
            angle_identity::selected_device(adapter)
        } else {
            gpu_identity::dxgi_identity(&info.name)
        };
        #[cfg(not(all(windows, feature = "backend-dx11-angle")))]
        let dxgi = gpu_identity::dxgi_identity(&info.name);
        #[cfg(all(windows, feature = "backend-dx11-native"))]
        let dxgi = if info.backend == wgpu::Backend::Dx11 {
            let hal =
                unsafe { adapter.as_hal::<wgpu_hal::dx11::Api>() }.expect("native DX11 adapter");
            let desc = unsafe { hal.raw_adapter().GetDesc1() }.expect("native DXGI identity");
            serde_json::json!({"vendor_id":desc.VendorId,"device_id":desc.DeviceId,"adapter_luid":[desc.AdapterLuid.HighPart,desc.AdapterLuid.LowPart]})
        } else {
            dxgi
        };
        let adapter = info;
        *self.renderer.lock().unwrap() = Some(serde_json::json!({
            "backend": format!("{:?}", adapter.backend),
            "implementation": if cfg!(feature = "backend-dx11-angle") && adapter.backend == wgpu::Backend::Gl { "angle-d3d11" } else if adapter.backend == wgpu::Backend::Dx11 {"native-wgpu-hal-d3d11"} else { "native" },
            "device_name": adapter.name, "vendor_id": adapter.vendor, "device_id": adapter.device,
            "device_type": format!("{:?}", adapter.device_type),
            "driver": adapter.driver, "driver_info": adapter.driver_info,
            "surface_format": format!("{:?}", surface.format),
            "present_mode": format!("{:?}", surface.present_mode),
            "dxgi": dxgi,
        }));
    }
    fn on_event(&self, _: &dyn Window, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                let key =
                    jadix::core::scroll_viewport::scroll_offset_y_signal_key("scroll-compare");
                let y = use_signal(&key, || 0_f32).peek();
                self.frames.lock().unwrap().push((
                    self.start.elapsed().as_secs_f64(),
                    y,
                    jadix::app::frame_stats::presented_frames_count(),
                ));
            }
            WindowEvent::MouseWheel { .. } => {
                self.wheels
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        }
    }
    fn on_exit(&self) {
        let frames = self.frames.lock().unwrap();
        let samples = frames
            .iter()
            .map(|(t, y, n)| format!("[{t:.6},{y:.3},{n}]"))
            .collect::<Vec<_>>()
            .join(",");
        let wheels = self.wheels.load(std::sync::atomic::Ordering::Relaxed);
        let renderer = self.renderer.lock().unwrap();
        let renderer = renderer.as_ref().expect("GPU identity was not observed");
        std::fs::write(&self.output, format!("{{\"start_unix\":{},\"wheel_events\":{wheels},\"redraws\":[{samples}],\"frame_counter_kind\":\"presented_frames\",\"renderer\":{renderer}}}", self.start_unix)).unwrap();
    }
}

fn scene() -> jadix::core::tree::Element {
    let scene = workload::scene();
    let rows: Vec<_> = (0..workload::rows(&scene)).map(|i| {
        let [number, title, status] = workload::labels(i);
        let mut row = ui! {
            <HStack w="100%" h={workload::ROW_HEIGHT} min_h={workload::ROW_HEIGHT} items="center" gap="16px" px="16px" border_style="solid" border_width={1.0} border_color={&format!("#{:06x}", workload::BORDER)}>
                <Label w="80px" text_size={workload::FONT_SIZE} font_family={workload::FONT} color={format!("#{:06x}", workload::FOREGROUND)} line_height={workload::LINE_HEIGHT}>{number}</Label>
                <Label grow="1" text_size={workload::FONT_SIZE} font_family={workload::FONT} color={format!("#{:06x}", workload::FOREGROUND)} line_height={workload::LINE_HEIGHT}>{title}</Label>
                <Label text_size={workload::FONT_SIZE} font_family={workload::FONT} color={format!("#{:06x}", workload::FOREGROUND)} line_height={workload::LINE_HEIGHT}>{status}</Label>
            </HStack>
        };
        if scene == "nested" {
            for _ in 0..8 { row = ui! { <VStack w="100%" h={workload::ROW_HEIGHT} min_h={workload::ROW_HEIGHT}>{row}</VStack> }; }
        }
        row
    }).collect();
    ui! {
        <VStack w="100%" h="100%" bg={&format!("#{:06x}", workload::BACKGROUND)}>
            <HStack w="100%" h={workload::HEADER_HEIGHT} min_h={workload::HEADER_HEIGHT} px="16px" items="center">
                <Label text_size={workload::FONT_SIZE} font_family={workload::FONT} color={format!("#{:06x}", workload::FOREGROUND)} line_height={workload::LINE_HEIGHT}>{workload::HEADER}</Label>
            </HStack>
            <ScrollArea id="scroll-compare" flex_grow="1" min_h="0" overflow_y="auto" scroll_behavior="jump">
                <VStack w="100%">{rows}</VStack>
            </ScrollArea>
        </VStack>
    }
}

fn main() {
    if std::env::var_os("JADIX_RENDER_PROFILE").is_some() {
        struct DiagnosticLog;
        impl log::Log for DiagnosticLog {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, record: &log::Record<'_>) {
                eprintln!("{}", record.args());
            }
            fn flush(&self) {}
        }
        static LOGGER: DiagnosticLog = DiagnosticLog;
        log::set_logger(&LOGGER).expect("diagnostic logger");
        log::set_max_level(log::LevelFilter::Warn);
    }
    // Keep telemetry identical to Gallery; no per-frame disk I/O or forced frames.
    jadix::app::set_window_lifecycle_hook(Some(Arc::new(Telemetry {
        output: std::env::var_os("JADIX_CPU_TELEMETRY")
            .expect("telemetry output")
            .into(),
        start: Instant::now(),
        start_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64(),
        frames: Mutex::new(Vec::new()),
        wheels: Default::default(),
        renderer: Mutex::new(None),
    })));
    app().title("Frame comparison").size(1280., 800.).run(scene);
}
