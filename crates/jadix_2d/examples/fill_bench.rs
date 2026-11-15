use jadix_2d::{filled_circle, rasterize_path_rgba, PathStyle};
use std::{hint::black_box, time::Instant};

fn main() {
    let path = filled_circle(128.0, 128.0, 120.0, 128);
    let style = PathStyle::fill([0.2, 0.5, 0.8, 1.0]);
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..10 {
        let pixels = rasterize_path_rgba(black_box(&path), &style, &[], 256, 256, 2);
        checksum += pixels.iter().map(|&v| u64::from(v)).sum::<u64>();
        black_box(&pixels);
    }
    println!(
        "10 x 256x256 circle, 2x2 SSAA: {:.3} ms; checksum={checksum}",
        start.elapsed().as_secs_f64() * 1000.0
    );
}
