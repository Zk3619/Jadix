#[cfg(not(windows))]
fn main() {
    eprintln!("native-images requires Windows");
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
    use base64::Engine;
    use jadix_native::{
        host::NativeRoot,
        images::{ImageDiskCache, ImageLoader, ImageSource},
        presentation::*,
        windows::WindowsHost,
    };
    use std::{
        io::{Read, Write},
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        Networking::WinHttp::WinHttpTimeFromSystemTime,
        System::Time::FileTimeToSystemTime,
    };

    const FILE: &str = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAQCAYAAAB3AH1ZAAAAKElEQVR4nGOUy3v2n2EAAdNAWj7qgNEQGA2B0RAYDYHREBgNAVAIAAAxpAKR1IQUtAAAAABJRU5ErkJggg==";
    const HTTP: &str = "iVBORw0KGgoAAAANSUhEUgAAABAAAAAgCAYAAAAbifjMAAAAM0lEQVR4nO3QMQEAIBDDwFAhCEM2hmB4CV1zeztk7XsehTRjD0YoxQNsgA2wgQ1GKMUDPpeRAon+g4MDAAAAAElFTkSuQmCC";

    struct App {
        root: NativeRoot<WindowsHost>,
        loader: ImageLoader,
        ids: [NodeId; 9],
        source: String,
        preview: Option<String>,
        revision: u64,
        uri: String,
        status: String,
        completed: usize,
        cache_hit: bool,
        disk_hit: bool,
        loading: bool,
        directory: PathBuf,
        disk_directory: PathBuf,
        viewport: [f32; 2],
        environment: u64,
    }

    fn node(id: NodeId, content: NodeContent, size: [Option<f32>; 2]) -> PresentationNode {
        PresentationNode {
            id,
            descriptor: NodeDescriptor {
                content,
                frame: Rect::default(),
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                accessible_name: None,
                style: style::NodeStyle {
                    width: size[0],
                    height: size[1],
                    shrink: 0.0,
                    ..Default::default()
                },
            },
            children: vec![],
        }
    }

    impl App {
        fn new(probe: bool) -> Result<Self, String> {
            let host = WindowsHost::new("Jadix Native asynchronous images", [900, 560])?;
            let viewport = host.viewport();
            let root = NativeRoot::new(host);
            let identifier = root.model().id();
            let directory = std::env::temp_dir().join(format!(
                "jadix-image-product-{}-{}",
                std::process::id(),
                identifier.token()
            ));
            std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
            let disk_directory = if probe {
                directory.clone()
            } else {
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("Jadix/NativeImages")
            };
            let loader = ImageLoader::with_disk_cache(
                root.model().lifetime(),
                Default::default(),
                Self::disk_config(disk_directory.clone()),
                root.host().waker(),
            )
            .map_err(|error| format!("{error:?}"))?;
            let path = directory.join("source.png");
            let mut app = Self {
                root,
                loader,
                ids: std::array::from_fn(|_| NodeId::new(identifier)),
                source: path.to_string_lossy().into(),
                preview: None,
                revision: 0,
                uri: format!("data:image/png;base64,{FILE}"),
                status: "Load a local PNG or an HTTP(S) URL".into(),
                completed: 0,
                cache_hit: false,
                disk_hit: false,
                loading: false,
                directory,
                disk_directory,
                viewport,
                environment: 0,
            };
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(FILE)
                .map_err(|error| error.to_string())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| error.to_string())?;
            file.write_all(&bytes).map_err(|error| error.to_string())?;
            app.render()?;
            app.root.host().show();
            Ok(app)
        }

        fn render(&mut self) -> Result<(), String> {
            let mut outer = node(self.ids[0], NodeContent::Container, [None, None]);
            outer.descriptor.style.padding = [20.0; 4];
            outer.descriptor.style.gap = 12.0;
            let mut actions = node(self.ids[3], NodeContent::Container, [None, Some(40.0)]);
            actions.descriptor.style.axis = style::Axis::Row;
            actions.descriptor.style.gap = 12.0;
            for (index, label) in [(4, "Load / reload"), (5, "Use cache"), (6, "Cancel")] {
                actions.children.push(node(
                    self.ids[index],
                    NodeContent::Button {
                        label: label.into(),
                        enabled: true,
                    },
                    [Some(160.0), Some(40.0)],
                ));
            }
            outer.children = vec![
                node(
                    self.ids[1],
                    NodeContent::Text(
                        "Native PNG resources · file / HTTP disk cache / bounded memory".into(),
                    ),
                    [None, Some(32.0)],
                ),
                node(
                    self.ids[2],
                    NodeContent::Input {
                        value: self.preview.clone().unwrap_or_else(|| self.source.clone()),
                        placeholder: "Absolute file path or HTTP(S) URL".into(),
                        enabled: true,
                        password: false,
                        revision: self.revision,
                    },
                    [Some(760.0), Some(40.0)],
                ),
                actions,
                node(
                    self.ids[7],
                    NodeContent::Image {
                        source: self.uri.clone(),
                        alt: "Downloaded PNG".into(),
                    },
                    [Some(240.0), Some(180.0)],
                ),
                node(
                    self.ids[8],
                    NodeContent::Text(self.status.clone()),
                    [None, Some(48.0)],
                ),
            ];
            self.root
                .present(&mut [outer], self.viewport, self.environment)
                .map_err(|error| error.to_string())?;
            Ok(())
        }

        fn begin(&mut self, refresh: bool) {
            let source =
                if self.source.starts_with("http://") || self.source.starts_with("https://") {
                    ImageSource::Url(self.source.clone())
                } else {
                    ImageSource::File(PathBuf::from(&self.source))
                };
            let result = if refresh {
                self.loader.reload(self.root.model(), self.ids[7], source)
            } else {
                self.loader.cancel(self.ids[7]);
                self.loader.request(self.root.model(), self.ids[7], source)
            };
            match result {
                Ok(_) => {
                    self.loading = true;
                    self.status = "Loading in the background…".into();
                }
                Err(error) => {
                    self.loading = false;
                    self.status = format!("Image request rejected: {error:?}");
                }
            }
        }

        fn poll(&mut self) -> Result<(), String> {
            let mut changed = false;
            for event in self.root.poll_events().map_err(|error| error.to_string())? {
                match event.payload {
                    EventPayload::Edit {
                        value,
                        revision,
                        composing_utf16,
                        ..
                    } if event.node == self.ids[2] => {
                        self.revision = revision;
                        self.preview = composing_utf16.map(|_| value.clone());
                        if composing_utf16.is_none() {
                            self.source = value;
                        }
                        changed = true;
                    }
                    EventPayload::Activate
                        if event.node == self.ids[4] || event.node == self.ids[5] =>
                    {
                        self.begin(event.node == self.ids[4]);
                        changed = true;
                    }
                    EventPayload::Activate if event.node == self.ids[6] => {
                        self.loader.cancel(self.ids[7]);
                        self.loading = false;
                        self.status = "Request cancelled; last image retained".into();
                        changed = true;
                    }
                    _ => {}
                }
            }
            for result in self.loader.poll(self.root.model()) {
                self.completed += 1;
                self.cache_hit = result.cache_hit;
                self.disk_hit = result.disk_hit;
                self.loading = false;
                match result.result {
                    Ok(uri) => {
                        self.uri = uri.to_string();
                        self.status = if result.disk_hit {
                            "Image ready from bounded disk cache".into()
                        } else if result.cache_hit {
                            "Image ready from bounded memory cache".into()
                        } else {
                            "Image ready from worker".into()
                        };
                    }
                    Err(error) => {
                        self.status = format!("Image load failed: {error:?}; last image retained");
                    }
                }
                if let Some(error) = result.disk_error {
                    self.status
                        .push_str(&format!("; disk cache unavailable: {error:?}"));
                }
                changed = true;
            }
            if changed {
                self.render()?;
            }
            Ok(())
        }

        fn disk_config(directory: PathBuf) -> ImageDiskCache {
            ImageDiskCache {
                directory,
                bytes: 8 * 1024 * 1024,
                entries: 32,
                age: Duration::from_secs(86400),
            }
        }

        fn stop_worker(&mut self) -> Result<(), String> {
            self.loader.dispose();
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.loader.inspection()["worker_active"] == true {
                if Instant::now() >= deadline {
                    return Err("image worker did not release disk cache".into());
                }
                self.root.host().pump();
                self.root.host().wait_for_events(10)?;
            }
            Ok(())
        }

        fn await_completion(&mut self, expected: usize, size: [u32; 2]) -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.root.host().pump();
                self.poll()?;
                let image = self
                    .root
                    .host()
                    .image_diagnostics(self.ids[7])
                    .ok_or("missing native image")?;
                if self.completed >= expected && image["decoded_size"] == serde_json::json!(size) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "native image did not complete: {} / {image}",
                        self.status
                    ));
                }
                self.root.host().wait_for_events(1000)?;
            }
        }
    }

    impl Drop for App {
        fn drop(&mut self) {
            self.loader.dispose();
            let _ = std::fs::remove_file(self.directory.join("source.png"));
            if self.disk_directory == self.directory
                && self.loader.inspection()["worker_active"] == false
            {
                let cache = self.directory.join("jadix-images-v1");
                if let Ok(entries) = std::fs::read_dir(&cache) {
                    for entry in entries.flatten() {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
                let _ = std::fs::remove_dir(cache);
            }
            let _ = std::fs::remove_dir(&self.directory);
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
            _ => return Err("Usage: native-images [--probe NONCE NEW_REPORT_JSON]".into()),
        };
        if probe.is_some_and(|(_, output)| output.exists()) {
            return Err("probe output already exists".into());
        }
        let mut app = App::new(probe.is_some())?;
        if let Some((nonce, output)) = probe {
            let image_identity = app
                .root
                .host()
                .control_identity(app.ids[7])
                .ok_or("missing image HWND")?;
            app.root.host().activate(app.ids[4])?;
            app.poll()?;
            app.await_completion(1, [32, 16])?;
            app.root.host().activate(app.ids[5])?;
            app.poll()?;
            app.await_completion(2, [32, 16])?;
            if !app.cache_hit {
                return Err("native cache action did not reuse the source".into());
            }
            app.stop_worker()?;
            std::fs::remove_file(app.directory.join("source.png"))
                .map_err(|error| error.to_string())?;
            app.uri = format!("data:image/png;base64,{HTTP}");
            app.render()?;
            app.await_completion(2, [16, 32])?;
            app.loader = ImageLoader::with_disk_cache(
                app.root.model().lifetime(),
                Default::default(),
                App::disk_config(app.disk_directory.clone()),
                app.root.host().waker(),
            )
            .map_err(|error| format!("{error:?}"))?;
            app.root.host().activate(app.ids[5])?;
            app.poll()?;
            app.await_completion(3, [32, 16])?;
            if !app.disk_hit || app.loader.inspection()["cache_entries"] != 1 {
                return Err("fresh image worker did not decode the offline disk entry".into());
            }
            let server =
                std::net::TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
            server
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
            let address = server.local_addr().map_err(|error| error.to_string())?;
            let fixture = std::thread::spawn(move || -> Result<(), String> {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match server.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                };
                stream
                    .set_nonblocking(false)
                    .map_err(|error| error.to_string())?;
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .map_err(|error| error.to_string())?;
                let mut request = [0u8; 4096];
                let mut received = 0;
                while !request[..received]
                    .windows(4)
                    .any(|bytes| bytes == b"\r\n\r\n")
                {
                    if received == request.len() {
                        return Err("HTTP fixture header limit".into());
                    }
                    let count = stream
                        .read(&mut request[received..])
                        .map_err(|error| error.to_string())?;
                    if count == 0 {
                        return Err("HTTP fixture received incomplete headers".into());
                    }
                    received += count;
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(HTTP)
                    .map_err(|error| error.to_string())?;
                let ticks = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_secs()
                    * 10000000
                    + 116444736000000000;
                let file_time = FILETIME {
                    dwLowDateTime: ticks as u32,
                    dwHighDateTime: (ticks >> 32) as u32,
                };
                let mut time = SYSTEMTIME::default();
                let mut date = [0u16; 62];
                if unsafe { FileTimeToSystemTime(&file_time, &mut time) } == 0
                    || unsafe { WinHttpTimeFromSystemTime(&time, date.as_mut_ptr()) } == 0
                {
                    return Err("HTTP fixture could not format its Date header".into());
                }
                let length = date
                    .iter()
                    .position(|value| *value == 0)
                    .ok_or("HTTP date was not terminated")?;
                let date =
                    String::from_utf16(&date[..length]).map_err(|error| error.to_string())?;
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/png\r\nDate: {date}\r\nCache-Control: public, max-age=60\r\nConnection: close\r\n\r\n",bytes.len()).map_err(|error|error.to_string())?;
                stream.write_all(&bytes).map_err(|error| error.to_string())
            });
            let length = app.source.encode_utf16().count();
            app.root.host().select_range(app.ids[2], [0, length])?;
            app.root
                .host()
                .replace_selection(app.ids[2], &format!("http://{address}/image.png"))?;
            app.poll()?;
            app.root.host().activate(app.ids[4])?;
            app.poll()?;
            let loaded = app.await_completion(4, [16, 32]);
            fixture.join().map_err(|_| "HTTP fixture failed")??;
            loaded?;
            app.stop_worker()?;
            app.uri = format!("data:image/png;base64,{FILE}");
            app.render()?;
            app.await_completion(4, [32, 16])?;
            app.loader = ImageLoader::with_disk_cache(
                app.root.model().lifetime(),
                Default::default(),
                App::disk_config(app.disk_directory.clone()),
                app.root.host().waker(),
            )
            .map_err(|error| format!("{error:?}"))?;
            app.root.host().activate(app.ids[5])?;
            app.poll()?;
            app.await_completion(5, [16, 32])?;
            if !app.disk_hit || app.loader.inspection()["disk_entries"] != 2 {
                return Err("fresh image worker did not decode the offline HTTP disk entry".into());
            }
            if app.root.host().control_identity(app.ids[7]) != Some(image_identity) {
                return Err("resource update replaced the image HWND".into());
            }
            let native = app.root.host().diagnostics();
            let resources = app.loader.inspection();
            let image = app.root.host().image_diagnostics(app.ids[7]);
            app.root
                .present(&mut [], app.viewport, app.environment)
                .map_err(|error| error.to_string())?;
            if !app.loader.poll(app.root.model()).is_empty()
                || app.loader.inspection()["bindings"] != 0
            {
                return Err("removed native image retained an active source binding".into());
            }
            app.stop_worker()?;
            let report = serde_json::json!({"schema":1,"nonce":nonce,"application":"native-images","native_controls":true,"file_load":true,"loopback_http_load":true,"cache_hit":true,"source_worker_wakes_ui":true,"image_hwnd_retained":true,"removed_binding_cancelled":true,"native":native,"resources":resources,"image":image,"https_external_service_accepted":false,"disk_cache":true,"disk_cache_file_sources_only":false,"disk_cache_worker_reopened_source_removed":true,"disk_cache_http_offline_redecoded_after_worker_replacement":true,"disk_cache_http_explicit_max_age":true,"disk_cache_redecoded_after_replacement":true,"disk_cache_cross_process_accepted":false,"android_ohos_network_accepted":false});
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
            println!("{}", serde_json::json!({"output":output,"image":image}));
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
