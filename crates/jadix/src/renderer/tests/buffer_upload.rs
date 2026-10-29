use super::*;

#[test]
fn changed_spans_reconstruct_arbitrary_frame_history() {
    let mut old = Vec::new();
    let mut seed = 42u64;
    for frame in 0..1200 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut new = old.clone();
        if frame % 7 == 0 {
            new.resize((seed as usize % 1000) * 4, 0);
        }
        if !new.is_empty() && frame % 3 != 0 {
            let index = seed as usize % new.len();
            new[index] = (seed >> 32) as u8;
        }
        let span = changed_range(&old, &new);
        assert_eq!(span.start % 4, 0);
        assert_eq!(span.end % 4, 0);
        let mut actual = old.clone();
        actual.resize(new.len(), 0);
        actual[span.clone()].copy_from_slice(&new[span]);
        assert_eq!(actual, new, "frame {frame}");
        old = new;
    }
    assert_eq!(changed_range(&[1; 1024], &[1; 1024]), 0..0);
    let mut new = [1; 1024];
    new[515] = 2;
    assert_eq!(changed_range(&[1; 1024], &new), 512..516);
}

#[test]
#[ignore = "native GPU upload readback and benchmark; run explicitly"]
fn retained_scene_upload_history_and_budget() {
    use super::super::offscreen::OffscreenGpu;
    use std::time::Instant;
    let backend = match std::env::var("JADIX_UPLOAD_BACKEND").as_deref() {
        Ok("gl") => wgpu::Backends::GL,
        Ok("vulkan") => wgpu::Backends::VULKAN,
        _ => wgpu::Backends::DX12,
    };
    let gpu = OffscreenGpu::with_backends(backend).expect("requested GPU adapter");
    eprintln!(
        "adapter={:?} profile=dev optimized-jadix samples=120 warmup=20",
        gpu.adapter_info
    );
    let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_SRC;
    let mut cached = RetainedBuffer::new(&gpu.device, "history", usage, &[]);
    let mut words = vec![0u32; 1024];
    for frame in 0..32 {
        match frame {
            2 => words[511] = 0x01020304,
            4 => words.truncate(17),
            6 => words.resize(1000, 0x11223344),
            8 => words.clear(),
            9 => words.resize(2049, 42),
            11 => words.resize(2050, 43),
            13 => words.reverse(),
            _ => {}
        }
        let bytes = bytemuck::cast_slice(&words);
        let stats = cached.upload_with_mode(&gpu.device, &gpu.queue, bytes, true);
        if frame == 2 {
            assert_eq!(stats.uploaded_bytes, 4);
        }
        if frame == 3 || frame == 4 || frame == 8 {
            assert_eq!(stats.write_calls, 0);
        }
        if frame == 11 {
            assert_eq!(stats.reallocations, 0, "reuse geometric capacity");
        }
        if bytes.is_empty() {
            continue;
        }
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("upload oracle"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(&cached.buffer, 0, &readback, 0, bytes.len() as u64);
        gpu.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        assert_eq!(
            &*readback.slice(..).get_mapped_range().unwrap(),
            bytes,
            "frame {frame}"
        );
        readback.unmap();
    }

    // Deterministic workload: same full-size stream for each mode; queue
    // completion stays outside upload+submit timing and prevents a backlog.
    for workload in ["idle", "one-word", "all"] {
        for retained in [false, true] {
            let mut words = vec![0u32; 1_048_576];
            let mut stream =
                RetainedBuffer::new(&gpu.device, "budget", usage, bytemuck::cast_slice(&words));
            let mut times = Vec::new();
            let mut total = SceneUploads::default();
            for sample in 0..140 {
                if workload == "one-word" {
                    words[511] = sample + 1;
                }
                if workload == "all" {
                    words.fill(sample + 1);
                }
                let start = Instant::now();
                let stats = stream.upload_with_mode(
                    &gpu.device,
                    &gpu.queue,
                    bytemuck::cast_slice(&words),
                    retained,
                );
                gpu.queue.submit([]);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                gpu.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                if sample >= 20 {
                    times.push(elapsed);
                    total.source_bytes += stats.source_bytes;
                    total.uploaded_bytes += stats.uploaded_bytes;
                    total.write_calls += stats.write_calls;
                    total.reallocations += stats.reallocations;
                }
            }
            times.sort_by(f64::total_cmp);
            if retained && workload == "idle" {
                assert_eq!(total.write_calls, 0);
            }
            if retained && workload == "one-word" {
                assert_eq!(total.uploaded_bytes, 120 * 4);
            }
            assert_eq!(total.reallocations, 0);
            eprintln!("upload-budget workload={workload} retained={retained} p50_ms={:.4} p95_ms={:.4} source_bytes={} uploaded_bytes={} writes={}",
                    times[60], times[114], total.source_bytes, total.uploaded_bytes, total.write_calls);
        }
    }
}
