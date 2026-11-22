#[cfg(not(windows))]
fn main() {
    eprintln!("native-gpu-chart requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = application::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod application {
    use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
    use std::io::Write;

    struct App {
        root: NativeRoot<WindowsHost>,
        outer: NodeId,
        caption: NodeId,
        input: NodeId,
        button: NodeId,
        plot: NodeId,
        status: NodeId,
        value: String,
        preview: Option<String>,
        revision: u64,
        amplitude: f32,
        error: bool,
        resets: u64,
        viewport: [f32; 2],
        environment: u64,
    }

    fn node(id: NodeId, content: NodeContent, size: [f32; 2]) -> PresentationNode {
        PresentationNode {
            id,
            descriptor: NodeDescriptor {
                interaction: Default::default(),
                keyboard: Default::default(),
                navigation_keys: vec![],
                accessibility: Default::default(),
                accessibility_relations: Default::default(),
                content,
                frame: Rect::default(),
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                accessible_description: None,
                accessible_name: None,
                style: style::NodeStyle {
                    width: Some(size[0]),
                    height: Some(size[1]),
                    shrink: 0.0,
                    ..Default::default()
                },
            },
            children: vec![],
        }
    }

    impl App {
        fn new() -> Result<Self, String> {
            let host = WindowsHost::new("Jadix Native + GPU chart", [900, 720])?;
            let viewport = host.viewport();
            let root = NativeRoot::new(host);
            let identifier = root.model().id();
            let mut app = Self {
                root,
                outer: NodeId::new(identifier),
                caption: NodeId::new(identifier),
                input: NodeId::new(identifier),
                button: NodeId::new(identifier),
                plot: NodeId::new(identifier),
                status: NodeId::new(identifier),
                value: "0.5".into(),
                preview: None,
                revision: 0,
                amplitude: 0.5,
                error: false,
                resets: 0,
                viewport,
                environment: 0,
            };
            app.render()?;
            app.root.host().show();
            app.update_plot()?;
            Ok(app)
        }

        fn update_plot(&mut self) -> Result<(), String> {
            let points: Vec<_> = (0..512)
                .map(|index| {
                    let horizontal = index as f32 / 511.0;
                    [
                        horizontal * 1.9 - 0.95,
                        (horizontal * std::f32::consts::TAU * 3.0).sin() * self.amplitude * 0.9,
                    ]
                })
                .collect();
            self.root.host_mut().set_gpu_plot(self.plot, &points)
        }

        fn render(&mut self) -> Result<(), String> {
            let width = self.viewport[0];
            let mut caption = node(
                self.caption,
                NodeContent::Text("Native EDIT + BUTTON → Rust state → DX12 chart".into()),
                [width, 40.0],
            );
            caption.descriptor.accessible_name = Some("Native and GPU chart controls".into());
            let mut input = node(
                self.input,
                NodeContent::Input {
                    max_length: None,
                    value: self.preview.clone().unwrap_or_else(|| self.value.clone()),
                    placeholder: "Amplitude 0..1".into(),
                    enabled: true,
                    password: false,
                    revision: self.revision,
                },
                [width, 40.0],
            );
            input.descriptor.accessible_name =
                Some("Chart amplitude, a number between zero and one".into());
            let button = node(
                self.button,
                NodeContent::Button {
                    label: "Reset amplitude".into(),
                    enabled: true,
                },
                [width, 40.0],
            );
            let mut plot = node(
                self.plot,
                NodeContent::Container,
                [width, (self.viewport[1] - 180.0).max(0.0)],
            );
            plot.descriptor.accessible_name =
                Some("GPU sine chart; amplitude is announced in the following text".into());
            let status = node(
                self.status,
                NodeContent::Text(if self.error {
                    "Invalid amplitude; previous plot retained".into()
                } else {
                    format!(
                        "Sine amplitude {:.2}; 512 GPU vertices; {} resets",
                        self.amplitude, self.resets
                    )
                }),
                [width, 60.0],
            );
            let mut outer = node(self.outer, NodeContent::Container, self.viewport);
            outer.children = vec![caption, input, button, plot, status];
            self.root
                .present(&mut [outer], self.viewport, self.environment)
                .map_err(|error| error.to_string())?;
            Ok(())
        }

        fn poll(&mut self) -> Result<(), String> {
            let events = self.root.poll_events().map_err(|error| error.to_string())?;
            let mut changed = false;
            let mut plot_changed = false;
            for event in events {
                match event.payload {
                    EventPayload::Edit {
                        value,
                        revision,
                        composing_utf16,
                        ..
                    } if event.node == self.input => {
                        self.revision = revision;
                        if composing_utf16.is_some() {
                            self.preview = Some(value);
                        } else {
                            self.preview = None;
                            self.value = value;
                            if let Ok(amplitude) = self.value.parse::<f32>() {
                                if amplitude.is_finite() && (0.0..=1.0).contains(&amplitude) {
                                    self.amplitude = amplitude;
                                    self.error = false;
                                    plot_changed = true;
                                } else {
                                    self.error = true;
                                }
                            } else {
                                self.error = true;
                            }
                        }
                        changed = true;
                    }
                    EventPayload::Activate if event.node == self.button => {
                        self.value = "0.5".into();
                        self.preview = None;
                        self.amplitude = 0.5;
                        self.revision = self
                            .revision
                            .checked_add(1)
                            .ok_or("chart edit revision exhausted")?;
                        self.error = false;
                        self.resets += 1;
                        changed = true;
                        plot_changed = true;
                    }
                    _ => {}
                }
            }
            if changed {
                self.render()?;
            }
            if plot_changed {
                self.update_plot()?;
            }
            Ok(())
        }
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        let probe = match arguments.as_slice() {
            [] => None,
            [mode, nonce, output]
                if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
            {
                Some((nonce, std::path::Path::new(output)))
            }
            _ => {
                return Err(String::from(
                    "Usage: native-gpu-chart [--probe NONCE NEW_REPORT_JSON]",
                ));
            }
        };
        if probe.is_some_and(|(_, output)| output.exists()) {
            return Err(String::from("probe output already exists"));
        }
        let mut app = App::new()?;
        if let Some((nonce, output)) = probe {
            let input_identity = app
                .root
                .host()
                .control_identity(app.input)
                .ok_or("missing native amplitude input")?;
            app.root.host().select_range(app.input, [0, 3])?;
            app.root.host().replace_selection(app.input, "0.75")?;
            app.poll()?;
            if app.amplitude != 0.75 || app.error {
                return Err(String::from(
                    "native editing did not update GPU business data",
                ));
            }
            let edited = app
                .root
                .host()
                .gpu_plot_diagnostics(app.plot)
                .ok_or("missing GPU plot")?;
            app.root.host().activate(app.button)?;
            app.poll()?;
            if app.amplitude != 0.5
                || app.resets != 1
                || app.root.host().control_identity(app.input) != Some(input_identity)
            {
                return Err(String::from(
                    "GPU update replaced native input or reset failed",
                ));
            }
            app.root.host_mut().recreate_gpu_plot_surface(app.plot)?;
            app.root.host().pump();
            app.poll()?;
            let before_idle = app
                .root
                .host()
                .gpu_plot_diagnostics(app.plot)
                .ok_or("missing GPU plot")?;
            app.poll()?;
            app.poll()?;
            let after_idle = app
                .root
                .host()
                .gpu_plot_diagnostics(app.plot)
                .ok_or("missing GPU plot")?;
            if after_idle["frames"] != before_idle["frames"]
                || after_idle["frames"].as_u64().unwrap_or(0) < 3
            {
                return Err(format!(
                    "GPU chart did not present or submitted idle frames: {before_idle} / {after_idle}"
                ));
            }
            let native = app.root.host().diagnostics();
            let inspection = app.root.inspection(false);
            if app
                .root
                .host_mut()
                .set_gpu_plot(app.plot, &[[f32::NAN, 0.0], [0.0, 0.0]])
                .is_ok()
            {
                return Err(String::from("GPU plot accepted invalid vertices"));
            }
            let (size, pixels) = app.root.host().gpu_plot_readback(app.plot)?;
            let line_pixels = pixels
                .chunks_exact(4)
                .filter(|pixel| {
                    pixel[0] < 100 && pixel[1] > 100 && pixel[2] > 150 && pixel[3] == 255
                })
                .count();
            if line_pixels < 128 || line_pixels > 8192 {
                return Err(format!(
                    "GPU readback did not contain the rendered line: {line_pixels}"
                ));
            }
            let image = output.with_extension("ppm");
            let mut image_file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&image)
                .map_err(|error| error.to_string())?;
            write!(image_file, "P6\n{} {}\n255\n", size[0], size[1])
                .map_err(|error| error.to_string())?;
            let rgb: Vec<_> = pixels
                .chunks_exact(4)
                .flat_map(|pixel| pixel[..3].iter().copied())
                .collect();
            image_file
                .write_all(&rgb)
                .map_err(|error| error.to_string())?;
            app.root
                .present(&mut [], app.viewport, app.environment)
                .map_err(|error| error.to_string())?;
            if app.root.host().gpu_plot_diagnostics(app.plot).is_some() {
                return Err(String::from("removed native node retained its GPU surface"));
            }
            let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true,
                "application": "native-gpu-chart", "edited_plot": edited, "final_plot": after_idle,
                "native": native, "inspection": inspection, "native_input_retained": true,
                "gpu_surface_recreated": true, "gpu_disposed_before_native_node": true, "idle_gpu_submissions": 0,
                "gpu_readback": {"size": size, "line_pixels": line_pixels, "image": image, "native_window_screenshot": false},
                "gpu_input_to_photon_accepted": false, "android_ohos_gpu_accepted": false, "full_jadix_gpu_widget_tree": false});
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)
                .map_err(|error| error.to_string())?;
            writeln!(
                file,
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::json!({"output": output, "gpu": after_idle})
            );
            return Ok(());
        }
        loop {
            app.root.host().pump();
            if app.root.host().is_closed() {
                return Ok(());
            }
            app.poll()?;
            let environment = app.root.host().environment();
            if environment != app.environment {
                app.environment = environment;
                app.viewport = app.root.host().viewport();
                app.render()?;
            }
            app.root.host().wait_for_events(u32::MAX)?;
        }
    }
}
