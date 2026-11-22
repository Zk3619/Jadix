use base64::Engine;
use jadix_native::{images::*, presentation::*, HeadlessRoot};
use std::{
    path::PathBuf,
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPQqPjwHwAE7AKQVV27NwAAAABJRU5ErkJggg==";

fn setup(
    limits: ImageLimits,
) -> (
    HeadlessRoot,
    Vec<PresentationNode>,
    ImageLoader,
    Receiver<()>,
) {
    let mut model = HeadlessRoot::new();
    let nodes: Vec<_> = (0..4)
        .map(|_| PresentationNode {
            id: NodeId::new(model.id()),
            descriptor: NodeDescriptor {
                content: NodeContent::Image {
                    source: format!("data:image/png;base64,{PNG}"),
                    alt: "pixel".into(),
                },
                frame: Rect::default(),
                style: Default::default(),
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                interaction: Default::default(),
                keyboard: Default::default(),
                navigation_keys: Vec::new(),
                accessibility: Default::default(),
                accessibility_relations: Default::default(),
                accessible_description: None,
                accessible_name: None,
            },
            children: vec![],
        })
        .collect();
    model.reconcile(&nodes).unwrap();
    let (wake, receive) = channel();
    let loader = ImageLoader::new(model.lifetime(), limits, move || {
        let _ = wake.send(());
    })
    .unwrap();
    (model, nodes, loader, receive)
}

fn wait(loader: &mut ImageLoader, model: &HeadlessRoot, wake: &Receiver<()>) -> Vec<ImageResult> {
    for _ in 0..8 {
        let results = loader.poll(model);
        if !results.is_empty() {
            return results;
        }
        wake.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    panic!("image worker did not complete")
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "jadix-images-{}-{}",
            std::process::id(),
            RootId::new().token()
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn disk_loader(model: &HeadlessRoot, config: ImageDiskCache) -> (ImageLoader, Receiver<()>) {
    let (sender, receiver) = channel();
    let loader = ImageLoader::with_disk_cache(
        model.lifetime(),
        ImageLimits {
            cache_entries: 0,
            cache_bytes: 0,
            ..Default::default()
        },
        config,
        move || {
            let _ = sender.send(());
        },
    )
    .unwrap();
    (loader, receiver)
}

fn stop(loader: &mut ImageLoader) {
    loader.dispose();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while loader.inspection()["worker_active"] == true {
        assert!(
            std::time::Instant::now() < deadline,
            "image worker did not release its cache lock"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn disk_images_reopen_offline_report_corruption_and_invalidate_failed_refresh() {
    let directory = Directory::new();
    let path = directory.0.join("private-source.png");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let (model, nodes, mut original, _) = setup(Default::default());
    stop(&mut original);
    let config = ImageDiskCache {
        directory: directory.0.clone(),
        bytes: 1024,
        entries: 2,
        age: Duration::from_secs(60),
    };
    let source = ImageSource::File(path.clone());
    let (mut loader, wake) = disk_loader(&model, config.clone());
    loader.request(&model, nodes[0].id, source.clone()).unwrap();
    let result = wait(&mut loader, &model, &wake).remove(0);
    assert!(
        result.result.is_ok()
            && !result.cache_hit
            && !result.disk_hit
            && result.disk_error.is_none()
    );
    assert_eq!(loader.inspection()["disk_entries"], 1);
    assert_eq!(loader.inspection()["disk_bytes"], bytes.len() + 56);
    assert!(!loader.inspection().to_string().contains("private-source"));
    let (mut busy, busy_wake) = disk_loader(&model, config.clone());
    busy.request(&model, nodes[1].id, source.clone()).unwrap();
    let result = wait(&mut busy, &model, &busy_wake).remove(0);
    assert!(result.result.is_ok() && !result.disk_hit);
    assert_eq!(result.disk_error, Some(ImageError::Busy));
    stop(&mut busy);
    stop(&mut loader);
    std::fs::remove_file(&path).unwrap();
    let (mut loader, wake) = disk_loader(&model, config.clone());
    loader.request(&model, nodes[0].id, source.clone()).unwrap();
    let result = wait(&mut loader, &model, &wake).remove(0);
    assert!(
        result.result.is_ok() && result.cache_hit && result.disk_hit && result.disk_error.is_none()
    );
    stop(&mut loader);
    let records: Vec<_> = std::fs::read_dir(directory.0.join("jadix-images-v1"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jic"))
        .collect();
    assert_eq!(records.len(), 1);
    assert!(!records[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("private-source"));
    let mut corrupt = std::fs::read(&records[0]).unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    std::fs::write(&records[0], corrupt).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let (mut loader, wake) = disk_loader(&model, config.clone());
    loader.request(&model, nodes[0].id, source.clone()).unwrap();
    let result = wait(&mut loader, &model, &wake).remove(0);
    assert!(result.result.is_ok() && !result.disk_hit);
    assert_eq!(result.disk_error, Some(ImageError::Format));
    std::fs::remove_file(path).unwrap();
    loader.reload(&model, nodes[0].id, source.clone()).unwrap();
    let result = wait(&mut loader, &model, &wake).remove(0);
    assert_eq!(
        result.result,
        Err(ImageError::Io(std::io::ErrorKind::NotFound))
    );
    assert_eq!(loader.inspection()["disk_entries"], 0);
    stop(&mut loader);
    let (mut loader, wake) = disk_loader(&model, config);
    loader.request(&model, nodes[0].id, source).unwrap();
    assert_eq!(
        wait(&mut loader, &model, &wake).remove(0).result,
        Err(ImageError::Io(std::io::ErrorKind::NotFound))
    );
    stop(&mut loader);
}

#[test]
fn disk_images_enforce_byte_entry_and_expiry_limits_across_workers() {
    let directory = Directory::new();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap();
    let paths = [
        directory.0.join("first.png"),
        directory.0.join("second.png"),
    ];
    for path in &paths {
        std::fs::write(path, &bytes).unwrap();
    }
    let (model, nodes, mut original, _) = setup(Default::default());
    stop(&mut original);
    let mut config = ImageDiskCache {
        directory: directory.0.clone(),
        bytes: bytes.len() + 56,
        entries: 1,
        age: Duration::from_secs(60),
    };
    let (mut loader, wake) = disk_loader(&model, config.clone());
    for (index, path) in paths.iter().enumerate() {
        loader
            .request(&model, nodes[index].id, ImageSource::File(path.clone()))
            .unwrap();
        let result = wait(&mut loader, &model, &wake).remove(0);
        assert!(result.result.is_ok() && result.disk_error.is_none());
        assert_eq!(loader.inspection()["disk_entries"], 1);
        assert_eq!(loader.inspection()["disk_bytes"], config.bytes);
    }
    stop(&mut loader);
    for path in &paths {
        std::fs::remove_file(path).unwrap();
    }
    let (mut loader, wake) = disk_loader(&model, config.clone());
    loader
        .request(&model, nodes[0].id, ImageSource::File(paths[0].clone()))
        .unwrap();
    assert_eq!(
        wait(&mut loader, &model, &wake).remove(0).result,
        Err(ImageError::Io(std::io::ErrorKind::NotFound))
    );
    loader
        .request(&model, nodes[1].id, ImageSource::File(paths[1].clone()))
        .unwrap();
    assert!(wait(&mut loader, &model, &wake).remove(0).disk_hit);
    stop(&mut loader);
    config.age = Duration::from_millis(1);
    std::fs::write(&paths[0], bytes).unwrap();
    let (mut loader, wake) = disk_loader(&model, config.clone());
    loader
        .reload(&model, nodes[0].id, ImageSource::File(paths[0].clone()))
        .unwrap();
    assert!(wait(&mut loader, &model, &wake).remove(0).result.is_ok());
    stop(&mut loader);
    std::fs::remove_file(&paths[0]).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    let (mut loader, wake) = disk_loader(&model, config);
    loader
        .request(&model, nodes[0].id, ImageSource::File(paths[0].clone()))
        .unwrap();
    let result = wait(&mut loader, &model, &wake).remove(0);
    assert!(!result.disk_hit && result.disk_error.is_none());
    assert_eq!(
        result.result,
        Err(ImageError::Io(std::io::ErrorKind::NotFound))
    );
    assert_eq!(loader.inspection()["disk_entries"], 0);
    stop(&mut loader);
}

#[test]
fn disk_images_survive_producer_process_exit_and_release_the_os_lock() {
    const PHASE: &str = "JADIX_DISK_IMAGE_PROCESS_PHASE";
    const DIRECTORY: &str = "JADIX_DISK_IMAGE_PROCESS_DIRECTORY";
    if let Some(phase) = std::env::var_os(PHASE) {
        let directory = PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
        let (model, nodes, mut original, _) = setup(Default::default());
        stop(&mut original);
        let (mut loader, wake) = disk_loader(
            &model,
            ImageDiskCache {
                directory: directory.clone(),
                bytes: 1024,
                entries: 2,
                age: Duration::from_secs(60),
            },
        );
        loader
            .request(
                &model,
                nodes[0].id,
                ImageSource::File(directory.join("source.png")),
            )
            .unwrap();
        let result = wait(&mut loader, &model, &wake).remove(0);
        assert!(result.result.is_ok() && result.disk_error.is_none());
        assert_eq!(result.disk_hit, phase == "consumer");
        if phase == "producer" {
            assert_eq!(loader.inspection()["worker_active"], true);
            std::process::exit(0);
        }
        stop(&mut loader);
        return;
    }
    let directory = Directory::new();
    let path = directory.0.join("source.png");
    std::fs::write(
        &path,
        base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap(),
    )
    .unwrap();
    for phase in ["producer", "consumer"] {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "disk_images_survive_producer_process_exit_and_release_the_os_lock",
                "--nocapture",
            ])
            .env(PHASE, phase)
            .env(DIRECTORY, &directory.0)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{phase}: {} / {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        if phase == "producer" {
            std::fs::remove_file(&path).unwrap();
        }
    }
}

#[test]
fn file_images_cache_refresh_and_byte_limits_are_bounded_without_changing_the_tree() {
    let directory = Directory::new();
    let path = directory.0.join("pixel.png");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let limits = ImageLimits {
        source_bytes: 128,
        cache_entries: 1,
        cache_bytes: 512,
        ..Default::default()
    };
    let (model, nodes, mut loader, wake) = setup(limits);
    let source = ImageSource::File(path.clone());
    let first = loader.request(&model, nodes[0].id, source.clone()).unwrap();
    let loaded = wait(&mut loader, &model, &wake).remove(0);
    assert_eq!(loaded.request, first);
    assert!(!loaded.cache_hit);
    assert_eq!(
        loaded.result.unwrap().as_ref(),
        format!("data:image/png;base64,{PNG}")
    );
    assert_eq!(model.revision(), 1);
    std::fs::remove_file(&path).unwrap();
    loader.request(&model, nodes[1].id, source.clone()).unwrap();
    let loaded = loader.poll(&model).remove(0);
    assert!(loaded.cache_hit && loaded.result.is_ok());
    loader.reload(&model, nodes[1].id, source.clone()).unwrap();
    assert!(matches!(
        wait(&mut loader, &model, &wake).remove(0).result,
        Err(ImageError::Io(std::io::ErrorKind::NotFound))
    ));
    std::fs::write(&path, [0u8; 129]).unwrap();
    loader.reload(&model, nodes[0].id, source).unwrap();
    assert_eq!(
        wait(&mut loader, &model, &wake).remove(0).result,
        Err(ImageError::Limits)
    );
    assert_eq!(loader.inspection()["cache_entries"], 0);
    assert!(loader.inspection()["cache_bytes"].as_u64().unwrap() <= 512);
    assert!(loader.inspection().to_string().find("pixel.png").is_none());
}

#[test]
fn failed_reload_cannot_recache_an_older_completion_for_another_node() {
    let directory = Directory::new();
    let path = directory.0.join("pixel.png");
    std::fs::write(
        &path,
        base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap(),
    )
    .unwrap();
    let (model, nodes, mut loader, wake) = setup(Default::default());
    let source = ImageSource::File(path.clone());
    loader.request(&model, nodes[0].id, source.clone()).unwrap();
    wake.recv_timeout(Duration::from_secs(3)).unwrap();
    std::fs::remove_file(&path).unwrap();
    loader.reload(&model, nodes[1].id, source.clone()).unwrap();
    wake.recv_timeout(Duration::from_secs(3)).unwrap();
    let results = loader.poll(&model);
    assert_eq!(results.len(), 2);
    assert!(results[0].result.is_ok());
    assert!(results[1].result.is_err());
    assert_eq!(loader.inspection()["cache_entries"], 0);
    loader.request(&model, nodes[2].id, source).unwrap();
    assert!(wait(&mut loader, &model, &wake).remove(0).result.is_err());
    stop(&mut loader);
}

#[test]
fn cancelled_removed_and_foreign_bindings_cannot_publish_ready_or_late_results() {
    let directory = Directory::new();
    let path = directory.0.join("pixel.png");
    std::fs::write(
        &path,
        base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap(),
    )
    .unwrap();
    let (mut model, mut nodes, mut loader, wake) = setup(Default::default());
    let source = ImageSource::File(path);
    loader.request(&model, nodes[0].id, source.clone()).unwrap();
    assert!(wait(&mut loader, &model, &wake).remove(0).result.is_ok());
    loader.request(&model, nodes[1].id, source.clone()).unwrap();
    let foreign = HeadlessRoot::new();
    assert!(loader.poll(&foreign).is_empty());
    assert_eq!(
        loader.request(&foreign, nodes[2].id, source.clone()),
        Err(ImageError::Node)
    );
    assert_eq!(loader.inspection()["bindings"], 2);
    nodes.remove(1);
    model.reconcile(&nodes).unwrap();
    assert!(loader.poll(&model).is_empty());
    loader.request(&model, nodes[1].id, source.clone()).unwrap();
    loader.cancel(nodes[1].id);
    assert!(loader.poll(&model).is_empty());
    model.dispose();
    assert!(loader.poll(&model).is_empty());
    assert_eq!(loader.inspection()["bindings"], 0);
    assert_eq!(loader.inspection()["cache_bytes"], 0);
    assert_eq!(
        loader.request(&model, nodes[0].id, source),
        Err(ImageError::Disposed)
    );
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn actual_winhttp_fetch_rejects_redirects_credentials_and_oversized_bodies() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        for stream in server.incoming().take(3) {
            let mut stream = stream.unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0u8; 4096];
            let mut count = 0;
            while !request[..count]
                .windows(4)
                .any(|bytes| bytes == b"\r\n\r\n")
            {
                assert!(count < request.len());
                let received = stream.read(&mut request[count..]).unwrap();
                assert!(received > 0);
                count += received;
            }
            let request = String::from_utf8_lossy(&request[..count]);
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(!request.to_ascii_lowercase().contains("cookie:"));
            let path = request.split_whitespace().nth(1).unwrap();
            let body = if path == "/oversized" {
                vec![0u8; 512]
            } else {
                base64::engine::general_purpose::STANDARD
                    .decode(PNG)
                    .unwrap()
            };
            let status = if path == "/redirect" {
                "302 Found"
            } else {
                "200 OK"
            };
            write!(stream,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: image/png\r\nLocation: /pixel\r\nConnection: close\r\n\r\n",body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    let (model, nodes, mut loader, wake) = setup(ImageLimits {
        source_bytes: 128,
        ..Default::default()
    });
    for (index, path) in ["/pixel", "/redirect", "/oversized"]
        .into_iter()
        .enumerate()
    {
        loader
            .request(
                &model,
                nodes[index].id,
                ImageSource::Url(format!("http://{address}{path}")),
            )
            .unwrap();
        let result = wait(&mut loader, &model, &wake).remove(0).result;
        match index {
            0 => assert!(result.is_ok(), "{result:?}"),
            1 => assert_eq!(result, Err(ImageError::Http(302))),
            2 => assert_eq!(result, Err(ImageError::Limits)),
            _ => unreachable!(),
        }
    }
    worker.join().unwrap();
    assert_eq!(
        loader.request(
            &model,
            nodes[3].id,
            ImageSource::Url(format!("http://user:pass@{address}/private")),
        ),
        Err(ImageError::Source)
    );
}

#[cfg(all(windows, feature = "windows"))]
fn http_fixture(
    headers: Vec<String>,
) -> (
    std::net::SocketAddr,
    Receiver<String>,
    std::thread::JoinHandle<()>,
) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        Networking::WinHttp::WinHttpTimeFromSystemTime,
        System::Time::FileTimeToSystemTime,
    };
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let address = server.local_addr().unwrap();
    let (sent, received) = channel();
    let worker = std::thread::spawn(move || {
        for headers in headers {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match server.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("HTTP fixture: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0u8; 4096];
            let mut count = 0;
            while !request[..count]
                .windows(4)
                .any(|bytes| bytes == b"\r\n\r\n")
            {
                assert!(count < request.len());
                let received = stream.read(&mut request[count..]).unwrap();
                assert!(received > 0);
                count += received;
            }
            let ticks = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                * 10000000
                + 116444736000000000;
            let file_time = FILETIME {
                dwLowDateTime: ticks as u32,
                dwHighDateTime: (ticks >> 32) as u32,
            };
            let mut time = SYSTEMTIME::default();
            let mut date = [0u16; 62];
            assert_ne!(unsafe { FileTimeToSystemTime(&file_time, &mut time) }, 0);
            assert_ne!(
                unsafe { WinHttpTimeFromSystemTime(&time, date.as_mut_ptr()) },
                0
            );
            let date =
                String::from_utf16(&date[..date.iter().position(|value| *value == 0).unwrap()])
                    .unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(PNG)
                .unwrap();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nDate: {date}\r\n{headers}Connection: close\r\n\r\n", bytes.len()).unwrap();
            stream.write_all(&bytes).unwrap();
            sent.send(String::from_utf8(request[..count].to_vec()).unwrap())
                .unwrap();
        }
    });
    (address, received, worker)
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn actual_winhttp_disk_cache_survives_process_exit_offline_and_invalidates_failed_reload() {
    const PHASE: &str = "JADIX_URL_IMAGE_CACHE_PHASE";
    const DIRECTORY: &str = "JADIX_URL_IMAGE_CACHE_DIRECTORY";
    const URL: &str = "JADIX_URL_IMAGE_CACHE_SOURCE";
    if let Ok(phase) = std::env::var(PHASE) {
        let directory = PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
        let source = ImageSource::Url(std::env::var(URL).unwrap());
        let (model, nodes, mut original, _) = setup(Default::default());
        stop(&mut original);
        let (mut loader, wake) = disk_loader(
            &model,
            ImageDiskCache {
                directory,
                bytes: 1024,
                entries: 2,
                age: Duration::from_secs(60),
            },
        );
        loader.request(&model, nodes[0].id, source.clone()).unwrap();
        let result = wait(&mut loader, &model, &wake).remove(0);
        assert!(result.result.is_ok() && result.disk_error.is_none());
        assert_eq!(result.disk_hit, phase == "consumer");
        assert_eq!(loader.inspection()["disk_entries"], 1);
        assert!(!loader.inspection().to_string().contains("private-token"));
        if phase == "producer" {
            std::process::exit(0);
        }
        loader.reload(&model, nodes[0].id, source.clone()).unwrap();
        let result = wait(&mut loader, &model, &wake).remove(0);
        assert!(result.result.is_err() && !result.cache_hit && !result.disk_hit);
        assert_eq!(loader.inspection()["disk_entries"], 0);
        loader.request(&model, nodes[1].id, source).unwrap();
        assert!(wait(&mut loader, &model, &wake).remove(0).result.is_err());
        stop(&mut loader);
        return;
    }
    let directory = Directory::new();
    let (address, requests, fixture) =
        http_fixture(vec!["Cache-Control: public, max-age=60\r\n".into()]);
    let mut fixture = Some(fixture);
    for phase in ["producer", "consumer"] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "actual_winhttp_disk_cache_survives_process_exit_offline_and_invalidates_failed_reload", "--nocapture"])
            .env(PHASE, phase).env(DIRECTORY, &directory.0).env(URL, format!("http://{address}/pixel.png?private-token=redacted"))
            .output().unwrap();
        assert!(
            result.status.success(),
            "{phase}: {} / {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        if phase == "producer" {
            let request = requests.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            fixture.take().unwrap().join().unwrap();
        }
    }
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn actual_winhttp_cache_policy_rejects_private_variant_stale_and_ambiguous_responses() {
    let cases = [
        "Cache-Control: max-age=60, no-store\r\n",
        "Cache-Control: private, max-age=60\r\n",
        "Cache-Control: max-age=60, no-cache=\"Set-Cookie\"\r\n",
        "Cache-Control: max-age=60\r\nVary: Accept-Encoding\r\n",
        "Cache-Control: max-age=60\r\nSet-Cookie: session=secret\r\n",
        "Cache-Control: max-age=60\r\nAge: 60\r\n",
        "Cache-Control: max-age=60, max-age=60\r\n",
        "Cache-Control: max-age=\"60\r\n",
        "Cache-Control: max-age=60\r\nPragma: no-cache\r\n",
        "",
    ];
    let (address, requests, fixture) =
        http_fixture(cases.iter().map(|case| case.to_string()).collect());
    let directory = Directory::new();
    let (model, nodes, mut original, _) = setup(Default::default());
    stop(&mut original);
    let (sent, wake) = channel();
    let mut loader = ImageLoader::with_disk_cache(
        model.lifetime(),
        Default::default(),
        ImageDiskCache {
            directory: directory.0.clone(),
            bytes: 1024,
            entries: 2,
            age: Duration::from_secs(60),
        },
        move || {
            let _ = sent.send(());
        },
    )
    .unwrap();
    for (index, _) in cases.iter().enumerate() {
        loader
            .reload(
                &model,
                nodes[0].id,
                ImageSource::Url(format!("http://{address}/{index}.png")),
            )
            .unwrap();
        let result = wait(&mut loader, &model, &wake).remove(0);
        assert!(
            result.result.is_ok() && !result.cache_hit && result.disk_error.is_none(),
            "case {index}"
        );
        assert_eq!(loader.inspection()["cache_entries"], 0, "case {index}");
        assert_eq!(loader.inspection()["disk_entries"], 0, "case {index}");
        requests.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    fixture.join().unwrap();
    stop(&mut loader);
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn actual_winhttp_cache_deadlines_include_queue_delay_and_fresh_memory_hits() {
    let (address, requests, fixture) = http_fixture(vec![
        "Cache-Control: max-age=60\r\n".into(),
        "Cache-Control: max-age=1\r\n".into(),
    ]);
    let (model, nodes, mut loader, wake) = setup(Default::default());
    let source = ImageSource::Url(format!("http://{address}/fresh.png"));
    loader.request(&model, nodes[0].id, source.clone()).unwrap();
    assert!(!wait(&mut loader, &model, &wake).remove(0).cache_hit);
    requests.recv_timeout(Duration::from_secs(3)).unwrap();
    loader.request(&model, nodes[1].id, source).unwrap();
    assert!(loader.poll(&model).remove(0).cache_hit);
    loader
        .reload(
            &model,
            nodes[0].id,
            ImageSource::Url(format!("http://{address}/expires.png")),
        )
        .unwrap();
    requests.recv_timeout(Duration::from_secs(3)).unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    let result = wait(&mut loader, &model, &wake).remove(0);
    assert!(result.result.is_ok() && !result.cache_hit);
    assert_eq!(loader.inspection()["cache_entries"], 1);
    fixture.join().unwrap();
    stop(&mut loader);
}

#[test]
fn disposal_closes_full_completion_channels_without_joining_the_worker() {
    let directory = Directory::new();
    let path = directory.0.join("pixel.png");
    std::fs::write(
        &path,
        base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap(),
    )
    .unwrap();
    let (model, nodes, mut loader, wake) = setup(ImageLimits {
        pending: 1,
        ..Default::default()
    });
    loader
        .request(&model, nodes[0].id, ImageSource::File(path.clone()))
        .unwrap();
    wake.recv_timeout(Duration::from_secs(5)).unwrap();
    loader
        .request(&model, nodes[1].id, ImageSource::File(path))
        .unwrap();
    loader.dispose();
    loop {
        match wake.recv_timeout(Duration::from_secs(1)) {
            Ok(()) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(error) => panic!("disposed image worker retained its wake callback: {error}"),
        }
    }
    assert!(loader.poll(&model).is_empty());
    assert_eq!(loader.inspection()["bindings"], 0);
}

#[cfg(all(windows, feature = "windows"))]
#[test]
fn cancelling_stalled_headers_or_body_closes_the_request_and_unblocks_the_single_worker() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::Instant,
    };
    for send_headers in [false, true] {
        let directory = Directory::new();
        let path = directory.0.join("next.png");
        std::fs::write(
            &path,
            base64::engine::general_purpose::STANDARD
                .decode(PNG)
                .unwrap(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (ready, accepted) = channel();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("stalled response fixture did not accept: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 4096];
            let mut count = 0;
            while !request[..count]
                .windows(4)
                .any(|bytes| bytes == b"\r\n\r\n")
            {
                let received = stream.read(&mut request[count..]).unwrap();
                assert!(received > 0);
                count += received;
                assert!(count < request.len());
            }
            if send_headers {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n12345678").unwrap();
            }
            ready.send(()).unwrap();
            match stream.read(&mut [0u8; 1]) {
                Ok(0) => (),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                result => {
                    panic!("cancelled WinHTTP request kept the stalled socket alive: {result:?}")
                }
            }
        });
        let (model, nodes, mut loader, wake) = setup(ImageLimits {
            pending: 1,
            ..Default::default()
        });
        loader
            .request(
                &model,
                nodes[0].id,
                ImageSource::Url(format!("http://{address}/stalled")),
            )
            .unwrap();
        accepted.recv_timeout(Duration::from_secs(5)).unwrap();
        let cancelled = Instant::now();
        loader.cancel(nodes[0].id);
        loader
            .request(&model, nodes[1].id, ImageSource::File(path))
            .unwrap();
        wake.recv_timeout(Duration::from_millis(1500)).unwrap();
        let results = loader.poll(&model);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node, nodes[1].id);
        assert!(results[0].result.is_ok());
        assert!(cancelled.elapsed() < Duration::from_millis(1500));
        loader.dispose();
        server.join().unwrap();
    }
}

#[test]
fn encoded_cache_limits_allow_disabling_retention_but_reject_unbounded_values() {
    let mut limits = ImageLimits::default();
    assert!(limits.is_valid());
    limits.cache_bytes = 0; limits.cache_entries = 0; limits.cache_age = Duration::ZERO;
    assert!(limits.is_valid());
    limits.cache_bytes = 32 * 1024 * 1024 + 1;
    assert!(!limits.is_valid());
    limits.cache_bytes = 0; limits.cache_entries = 129;
    assert!(!limits.is_valid());
    limits.cache_entries = 0; limits.cache_age = Duration::from_secs(86401);
    assert!(!limits.is_valid());
}
