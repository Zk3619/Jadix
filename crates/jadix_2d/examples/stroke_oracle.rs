//! Independent numeric coverage oracle for the benchmark's monotone cubic.
//! No Jadix/Kurbo/Lyon geometry is used. Newton minimizes distance to the exact
//! cubic at each 16x16 subpixel; finite-difference verification bounds convergence.
use std::{error::Error, path::PathBuf};
fn curve(t: f64) -> ([f64; 2], [f64; 2], [f64; 2]) {
    let u = 1. - t;
    let a = [8., 0.];
    let b = [70., -20.];
    let c = [180., 20.];
    let d = [248., 0.];
    let mut p = [0.; 2];
    let mut v = [0.; 2];
    let mut w = [0.; 2];
    for i in 0..2 {
        p[i] = u * u * u * a[i] + 3. * u * u * t * b[i] + 3. * u * t * t * c[i] + t * t * t * d[i];
        v[i] = 3. * u * u * (b[i] - a[i]) + 6. * u * t * (c[i] - b[i]) + 3. * t * t * (d[i] - c[i]);
        w[i] = 6. * u * (c[i] - 2. * b[i] + a[i]) + 6. * t * (d[i] - 2. * c[i] + b[i]);
    }
    (p, v, w)
}
fn distance(x: f64, y: f64) -> f64 {
    let mut t = ((x - 8.) / 240.).clamp(0., 1.);
    for _ in 0..10 {
        let (p, v, w) = curve(t);
        let dx = p[0] - x;
        let dy = p[1] - y;
        let den = v[0] * v[0] + v[1] * v[1] + dx * w[0] + dy * w[1];
        let next = (t - (dx * v[0] + dy * v[1]) / den).clamp(0., 1.);
        if (t - next).abs() < 1e-12 {
            break;
        }
        t = next;
    }
    let (p, _, _) = curve(t);
    (p[0] - x).hypot(p[1] - y)
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    let samples = std::env::args()
        .nth(2)
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(16);
    if !(1..=64).contains(&samples) {
        return Err("samples in 1..=64".into());
    }
    std::fs::create_dir_all(&output)?;
    // Compare minimization against an independent dense search for sampled points.
    let mut max_error = 0f64;
    for i in 0..1000 {
        let x = (i * 719 % 25600) as f64 / 100.;
        let y = (i * 337 % 4000) as f64 / 100. - 20.;
        let dense = (0..=10000)
            .map(|n| {
                let (p, _, _) = curve(n as f64 / 10000.);
                (p[0] - x).hypot(p[1] - y)
            })
            .fold(f64::INFINITY, f64::min);
        max_error = max_error.max((distance(x, y) - dense).max(0.));
    }
    assert!(max_error < 1e-7, "closest-point solver: {max_error}");
    for size in [256usize, 512] {
        let scale = size as f64 / 256.;
        let mut rgba = vec![0u8; size * size * 4];

        for y in 0..size {
            for x in 0..size {
                let mut coverage = 0u32;
                for sy in 0..samples {
                    for sx in 0..samples {
                        let px = (x as f64 + (sx as f64 + 0.5) / samples as f64) / scale;
                        let py = (y as f64 + (sy as f64 + 0.5) / samples as f64) / scale;
                        let (p, _, _) = curve(((px - 8.) / 240.).clamp(0., 1.));
                        let row = ((py - 8. - p[1]) / 7.5).round().clamp(0., 31.);
                        coverage += u32::from(distance(px, py - 8. - row * 7.5) <= 0.75);
                    }
                }
                if coverage > 0 {
                    rgba[(y * size + x) * 4..][..4].copy_from_slice(&[
                        68,
                        170,
                        255,
                        (coverage as f64 * 255. / (samples * samples) as f64).round() as u8,
                    ]);
                }
            }
        }
        image::save_buffer(
            output.join(format!("dynamic-curves-{size}-oracle.png")),
            &rgba,
            size as u32,
            size as u32,
            image::ColorType::Rgba8,
        )?;
    }
    std::fs::write(output.join("oracle.json"),format!("{{\"samples_per_pixel\":{},\"closest_point_positive_error_vs_10001_samples\":{max_error},\"validation_points\":1000}}",samples*samples))?;
    Ok(())
}
