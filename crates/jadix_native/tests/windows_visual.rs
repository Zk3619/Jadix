#![cfg(all(windows, feature = "windows-gpu"))]

use jadix_core::hosted::{CanvasCommand, HostedContent};
use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use std::time::{Duration, Instant};
use windows_sys::Win32::{Foundation::HWND, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

fn node(root: RootId, content: NodeContent, width: f32, height: f32) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            style: style::NodeStyle {
                width: Some(width),
                height: Some(height),
                shrink: 0.,
                ..Default::default()
            },
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: vec![],
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
        },
        children: vec![],
    }
}

fn hosted(spec: &HostedContent) -> NodeContent {
    NodeContent::Hosted {
        specification: serde_json::to_string(spec).unwrap(),
    }
}

fn pump(root: &mut NativeRoot<WindowsHost>, duration: Duration) {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        root.host().pump();
        let events = root.poll_events().unwrap();
        assert!(
            !events.iter().any(
                |e| matches!(&e.payload, EventPayload::Hosted { event, .. } if event == "error")
            ),
            "{events:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pixel(window: HWND, x: i32, y: i32) -> [u8; 3] {
    unsafe {
        let dc = GetDC(window);
        assert!(!dc.is_null());
        let value = GetPixel(dc, x, y);
        ReleaseDC(window, dc);
        assert_ne!(value, u32::MAX);
        [value as u8, (value >> 8) as u8, (value >> 16) as u8]
    }
}

#[test]
#[ignore = "requires an unobscured Windows desktop and DX12"]
fn nested_canvas_pixels_survive_shader_capture_and_exposure() {
    let mut root =
        NativeRoot::new(WindowsHost::new("Jadix nested visual acceptance", [500, 380]).unwrap());
    let id = root.model().id();
    let mut outer = node(id, NodeContent::Container, 480., 340.);
    outer.descriptor.style.padding = [24.; 4];
    let mut container = node(id, NodeContent::Container, 320., 260.);
    container.descriptor.style.padding = [20.; 4];
    let mut canvas = node(
        id,
        hosted(&HostedContent::Canvas {
            commands: vec![
                CanvasCommand::Fill {
                    rect: [0., 0., 100., 100.],
                    color: [255, 0, 0, 128],
                },
                CanvasCommand::Text {
                    text: "嵌套画布 😀".into(),
                    position: [5., 115.],
                    size: 24.,
                    color: [255; 4],
                },
            ],
        }),
        240.,
        180.,
    );
    canvas.descriptor.style.background = [0, 0, 255, 255];
    let canvas_id = canvas.id;
    container.children.push(canvas);
    outer.children.push(container);
    let mut roots = [outer];
    root.present(&mut roots, [500., 380.], 0).unwrap();
    root.host().show();
    unsafe {
        SetForegroundWindow(root.host().native_window() as _);
    }
    pump(&mut root, Duration::from_secs(2));
    let canvas_window = root.host().control_identity(canvas_id).unwrap() as HWND;
    let scale = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(canvas_window) }.max(96)
        as f64
        / 96.;
    let sample = || pixel(canvas_window, (40. * scale) as i32, (40. * scale) as i32);
    let direct = sample();
    assert!(
        direct[0].abs_diff(128) <= 1 && direct[1] == 0 && direct[2].abs_diff(127) <= 1,
        "{direct:?}"
    );
    let shader: HostedContent = serde_json::from_value(serde_json::json!({
        "type":"shader", "language":"wgsl", "source":"fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_content(uv); }",
        "function":"shade", "uniforms":[], "frames_per_second":60, "animated":false, "composition":{"type":"effect"}
    })).unwrap();
    let children = std::mem::take(&mut roots[0].children[0].children);
    let mut effect = node(id, hosted(&shader), 320., 260.);
    effect.descriptor.style.padding = [20.; 4];
    effect.children = children;
    roots[0].children[0] = effect;
    root.present(&mut roots, [500., 380.], 0).unwrap();
    pump(&mut root, Duration::from_secs(2));
    assert_eq!(
        root.host().control_identity(canvas_id),
        Some(canvas_window as usize)
    );
    assert_eq!(sample(), direct);
    unsafe {
        RedrawWindow(
            root.host().native_window() as _,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
    }
    pump(&mut root, Duration::from_secs(1));
    assert_eq!(sample(), direct);
    println!(
        "visible nested canvas / DX12 capture / exposure RGB={direct:?}, retained HWND={canvas_window:?}"
    );
}

#[test]
#[ignore = "requires an unobscured Windows desktop and the system H.264 encoder"]
fn media_foundation_video_decodes_to_visible_native_surface() {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }
    let source = std::env::var("JADIX_ACCEPTANCE_VIDEO").unwrap_or_else(|_| {
        let path =
            std::env::temp_dir().join(format!("jadix-video-{}-中文😀.mp4", std::process::id()));
        encode_red_clip(&path);
        path.to_str().unwrap().to_owned()
    });
    let path = std::path::Path::new(&source);
    let mut host = WindowsHost::new("Jadix video desktop acceptance", [800, 640]).unwrap();
    host.set_asset_root(path.parent().unwrap()).unwrap();
    let mut root = NativeRoot::new(host);
    let specification: HostedContent = serde_json::from_value(serde_json::json!({
        "type":"media", "source":format!("asset:/{}",path.file_name().unwrap().to_str().unwrap()),
        "audio":false, "paused":false, "looping":true, "controls":true, "muted":true,
        "volume":0.5, "rate":1., "seek":null, "seek_revision":0, "fit":"contain"
    }))
    .unwrap();
    let mut roots = [node(root.model().id(), hosted(&specification), 480., 320.)];
    root.present(&mut roots, [500., 380.], 0).unwrap();
    root.host().show();
    unsafe {
        SetForegroundWindow(root.host().native_window() as _);
    }
    let window = root.host().control_identity(roots[0].id).unwrap() as HWND;
    let media_window = window;
    // Media Foundation presents through desktop composition; its pixels are not in the GDI DC.
    let video: Vec<u16> = "Video\0".encode_utf16().collect();
    let window = unsafe {
        FindWindowExW(
            window,
            std::ptr::null_mut(),
            std::ptr::null(),
            video.as_ptr(),
        )
    };
    assert!(!window.is_null());
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        pump(&mut root, Duration::from_millis(100));
        let mut bounds = windows_sys::Win32::Foundation::RECT::default();
        unsafe {
            GetClientRect(window, &mut bounds);
        }
        if bounds.right == 0 || bounds.bottom == 0 {
            assert!(
                Instant::now() < until,
                "video surface has no visible layout: {window:?}"
            );
            continue;
        }
        let mut position = windows_sys::Win32::Foundation::POINT {
            x: bounds.right / 2,
            y: bounds.bottom / 3,
        };
        unsafe {
            ClientToScreen(window, &mut position);
        }
        let color = pixel(std::ptr::null_mut(), position.x, position.y);
        if color[0] > 200 && color[1] < 40 && color[2] < 40 {
            break;
        }
        assert!(
            Instant::now() < until,
            "video did not decode red frame: {color:?}"
        );
    }
    pump(&mut root, Duration::from_secs(2));
    let mut texts = Vec::new();
    unsafe {
        let mut child = GetWindow(media_window, GW_CHILD);
        while !child.is_null() {
            let mut text = [0u16; 128];
            let length = GetWindowTextW(child, text.as_mut_ptr(), text.len() as i32);
            texts.push(String::from_utf16_lossy(&text[..length as usize]));
            child = GetWindow(child, GW_HWNDNEXT);
        }
    }
    assert!(texts.iter().any(|text| text == "Pause"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|text| text.ends_with(" / 0:04") && !text.starts_with("0:00")),
        "{texts:?}"
    );
    println!("Media Foundation H.264 visible frame, controls, progress and disposal passed");
    drop(root);
    if std::env::var_os("JADIX_ACCEPTANCE_VIDEO").is_none() {
        std::fs::remove_file(path).unwrap();
    }
}

fn encode_red_clip(path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{Media::MediaFoundation::*, System::Com::*},
        core::PCWSTR,
    };
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).unwrap();
        MFStartup(MF_VERSION, MFSTARTUP_FULL).unwrap();
        {
            let url: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let writer = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None, None).unwrap();
            let output = MFCreateMediaType().unwrap();
            output
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .unwrap();
            output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).unwrap();
            output.SetUINT32(&MF_MT_AVG_BITRATE, 128_000).unwrap();
            output
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .unwrap();
            output
                .SetUINT64(&MF_MT_FRAME_SIZE, (320u64 << 32) | 180)
                .unwrap();
            output
                .SetUINT64(&MF_MT_FRAME_RATE, (10u64 << 32) | 1)
                .unwrap();
            output
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
                .unwrap();
            let stream = writer.AddStream(&output).unwrap();
            let input = MFCreateMediaType().unwrap();
            input
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .unwrap();
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).unwrap();
            input
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .unwrap();
            input
                .SetUINT64(&MF_MT_FRAME_SIZE, (320u64 << 32) | 180)
                .unwrap();
            input
                .SetUINT64(&MF_MT_FRAME_RATE, (10u64 << 32) | 1)
                .unwrap();
            input
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
                .unwrap();
            writer.SetInputMediaType(stream, &input, None).unwrap();
            writer.BeginWriting().unwrap();
            for frame in 0..40 {
                let buffer = MFCreateMemoryBuffer(320 * 180 * 4).unwrap();
                let mut bytes = std::ptr::null_mut();
                buffer.Lock(&mut bytes, None, None).unwrap();
                for pixel in
                    std::slice::from_raw_parts_mut(bytes, 320 * 180 * 4).chunks_exact_mut(4)
                {
                    pixel.copy_from_slice(&[0, 0, 255, 255]);
                }
                buffer.Unlock().unwrap();
                buffer.SetCurrentLength(320 * 180 * 4).unwrap();
                let sample = MFCreateSample().unwrap();
                sample.AddBuffer(&buffer).unwrap();
                sample.SetSampleTime(frame * 1_000_000).unwrap();
                sample.SetSampleDuration(1_000_000).unwrap();
                writer.WriteSample(stream, &sample).unwrap();
            }
            writer.Finalize().unwrap();
        }
        MFShutdown().unwrap();
        CoUninitialize();
    }
}
