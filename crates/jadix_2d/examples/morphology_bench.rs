//! CPU whole-layer smoke benchmark; no GPU or Vello speed claim.
use jadix_2d::*;
use std::time::Instant;
fn main() {
    let size = 256;
    let bytes: Vec<u8> = (0..size * size)
        .flat_map(|i| {
            let a = ((i * 37 + 19) % 256) as u8;
            [a / 2, a / 3, a / 4, a]
        })
        .collect();
    let mut rows = Vec::new();
    for radius in [1, 8, 32] {
        let mut times = Vec::new();
        let mut checksum = 0u64;
        for frame in 0..9 {
            let mut c = RasterCanvas::new_linear(size, size);
            c.push_layer_with_options(LayerOptions {
                filters: vec![LayerFilter::Morphology {
                    radius: [radius; 2],
                    operator: Morphology::Dilate,
                }],
                ..Default::default()
            });
            c.put_image_data(0, 0, size, size, &bytes);
            let start = Instant::now();
            c.pop_layer();
            let ms = start.elapsed().as_secs_f64() * 1000.;
            if frame >= 2 {
                times.push(ms);
            }
            checksum = c
                .get_image_data(0, 0, size, size)
                .unwrap()
                .iter()
                .map(|v| *v as u64)
                .sum();
        }
        times.sort_by(f64::total_cmp);
        rows.push(serde_json::json!({"size":size,"radius":radius,"samples":7,"p50_ms":times[3],"max_ms":times[6],"checksum":checksum}));
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"method":"CPU pop_layer, dev opt-level=1, 2 warmups + 7 repeats","results":rows})).unwrap());
}
