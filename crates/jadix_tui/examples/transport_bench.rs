//! Encoding/diff CPU and wire-byte benchmark; not terminal scanout latency.
use jadix_tui::{ratatui::layout::Rect, KittyReply, KittyTerminal, RgbaFrame, TerminalViewport};
use std::{
    cell::Cell,
    io,
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
struct Sink(Rc<Cell<u64>>);
impl io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.set(self.0.get() + bytes.len() as u64);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn run(
    width: u32,
    height: u32,
    iterations: u32,
    workload: &str,
    compress: bool,
    tiled: bool,
) -> io::Result<serde_json::Value> {
    let sink = Sink::default();
    let mut host = KittyTerminal::new(
        sink.clone(),
        TerminalViewport::new(Rect::new(0, 0, 80, 32), [width, height], 1.0)?,
        NonZeroU32::new(42).unwrap(),
        NonZeroU32::new(1000).unwrap(),
        Duration::from_secs(30),
    )?;
    if compress {
        host.request_compression()?;
    }
    if tiled {
        host.request_tiling(jadix_tui::ratatui::layout::Size::new(16, 8))?;
    }
    host.start(Instant::now())?;
    let mut fence = 1000;
    while !host.ready() {
        host.acknowledge(
            &KittyReply {
                id: NonZeroU32::new(fence).unwrap(),
                message: "OK".into(),
            },
            Instant::now(),
        )?;
        fence += 1;
    }
    let mut pixels = [20u8, 30, 45, 255].repeat((width * height) as usize);
    host.present(Instant::now(), |_| {
        RgbaFrame::new(width, height, pixels.clone())
    })?;
    host.acknowledge(
        &KittyReply {
            id: NonZeroU32::new(fence).unwrap(),
            message: "OK".into(),
        },
        Instant::now(),
    )?;
    fence += 1;
    let before = sink.0.get();
    let mut samples = Vec::new();
    for index in 0..iterations {
        match workload {
            "point" => pixels[0] = (index % 2) as u8,
            "line" => {
                for pixel in pixels[..width as usize * 4].chunks_exact_mut(4) {
                    pixel[0] = (index % 2) as u8;
                }
            }
            "full" => {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel[0] = (index % 2) as u8;
                }
            }
            _ => (),
        }
        host.invalidate();
        let start = Instant::now();
        let submitted = host.present(Instant::now(), |_| {
            RgbaFrame::new(width, height, pixels.clone())
        })?;
        samples.push(start.elapsed().as_nanos() as u64);
        if submitted {
            host.acknowledge(
                &KittyReply {
                    id: NonZeroU32::new(fence).unwrap(),
                    message: "OK".into(),
                },
                Instant::now(),
            )?;
            fence += 1;
        }
    }
    samples.sort_unstable();
    let metrics = host.damage_metrics();
    let result = serde_json::json!({"size":[width,height],"workload":workload,"compressed":compress,"tiled":tiled,"iterations":iterations,"wire_bytes":sink.0.get()-before,"submitted_frames":host.submitted_frames()-1,"median_ns":samples[samples.len()/2],"p95_ns":samples[(samples.len()*95/100).min(samples.len()-1)],"mean_ns":samples.iter().sum::<u64>() / samples.len() as u64,"encoded_images":metrics.encoded_tiles,"full_frames":metrics.full_frames,"identical_frames":metrics.identical_frames});
    host.close()?;
    Ok(result)
}
fn main() -> io::Result<()> {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(80u32)
        .clamp(1, 2000);
    let mut results = Vec::new();
    let modes = if std::env::args().any(|arg| arg == "--reverse") {
        [true, false]
    } else {
        [false, true]
    };
    for tiled in modes {
        for size in [[480, 384], [960, 640]] {
            for compress in [false, true] {
                for workload in ["stable", "point", "line", "full"] {
                    results.push(run(
                        size[0], size[1], iterations, workload, compress, tiled,
                    )?);
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"profile":"dev opt-level=1; counting writer; synchronous synthetic ACK; no GPU/PTY","results":results})
        )?
    );
    Ok(())
}
