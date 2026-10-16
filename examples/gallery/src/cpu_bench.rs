//! Opt-in measurement only: buffer frame/scroll samples, flush after the run.
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use jadix::app::{set_window_lifecycle_hook, WindowLifecycleHook};
use winit::{event::WindowEvent, window::Window};

/// A fixed, non-virtualized ordinary list. Keep its workload stable across
/// Gallery showcase edits so results from different commits stay comparable.
pub(super) fn ordinary_scene() -> jadix::core::tree::Element {
    use jadix::prelude::*;
    let rows: Vec<_> = (0..200).map(|i| {
        let title = format!("List item {i:03}");
        ui! {
            <HStack w="100%" h="48px" min_h="48px" items="center" gap="1rem" px="1rem" border_width={1.0} border_color="border">
                <Label w="5rem" color="muted-foreground">{format!("{:03}", i + 1)}</Label>
                <Label grow="1">{title}</Label>
                <Label color="muted-foreground">"Ready"</Label>
            </HStack>
        }
    }).collect();
    ui! {
        <VStack w="100%" h="100%" bg="background">
            <Label p="1rem" text_size="1.25rem">"Jadix CPU benchmark - 200 rows / 600 labels"</Label>
            <ScrollArea id="scroll-ordinary" flex_grow="1" min_h="0" overflow_y="auto" scroll_behavior="smooth">
                <VStack w="100%">{rows}</VStack>
            </ScrollArea>
        </VStack>
    }
}

struct Telemetry {
    output: PathBuf,
    scroll_key: String,
    start: Instant,
    start_unix: f64,
    frames: Mutex<Vec<(f64, f32, u64)>>,
    wheels: std::sync::atomic::AtomicUsize,
}

impl WindowLifecycleHook for Telemetry {
    fn on_created(&self, _: &dyn Window, _: &str) {}
    fn on_closing(&self, _: &dyn Window) {}
    fn on_event(&self, _: &dyn Window, event: &WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                let offset = jadix::prelude::use_signal(&self.scroll_key, || 0.0_f32).peek();
                self.frames.lock().unwrap().push((
                    self.start.elapsed().as_secs_f64(),
                    offset,
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
            .map(|(t, y, presents)| format!("[{t:.6},{y:.3},{presents}]"))
            .collect::<Vec<_>>()
            .join(",");
        let wheels = self.wheels.load(std::sync::atomic::Ordering::Relaxed);
        std::fs::write(
            &self.output,
            format!(
                "{{\"start_unix\":{},\"wheel_events\":{wheels},\"redraws\":[{samples}]}}",
                self.start_unix
            ),
        )
        .expect("write CPU benchmark telemetry");
    }
}

struct ProfileLog(Mutex<std::fs::File>);
impl log::Log for ProfileLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let _ = writeln!(self.0.lock().unwrap(), "{}", record.args());
        }
    }
    fn flush(&self) {}
}

pub(super) fn install() -> bool {
    let Some(output) = std::env::var_os("JADIX_CPU_TELEMETRY") else {
        return false;
    };
    if let Some(path) = std::env::var_os("JADIX_CPU_PROFILE") {
        let logger = ProfileLog(Mutex::new(std::fs::File::create(path).unwrap()));
        log::set_logger(Box::leak(Box::new(logger))).expect("install CPU profile logger");
        log::set_max_level(log::LevelFilter::Warn);
    }
    set_window_lifecycle_hook(Some(Arc::new(Telemetry {
        output: output.into(),
        scroll_key: jadix::core::scroll_viewport::scroll_offset_y_signal_key(&format!(
            "scroll-{}",
            std::env::var("JADIX_GALLERY_SECTION").unwrap_or_default()
        )),
        start: Instant::now(),
        start_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64(),
        frames: Mutex::new(Vec::new()),
        wheels: std::sync::atomic::AtomicUsize::new(0),
    })));
    true
}
