//! CPU preparation comparison only; excludes GPU work and window presentation.
use jadix_2d::*;
use serde_json::json;
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font = Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )?;
    let positions: Vec<_> = "Jadix vector glyphs / Jadix vector glyphs"
        .chars()
        .enumerate()
        .map(|(i, ch)| PositionedGlyph {
            id: font.glyph_id(ch).unwrap(),
            x: 4. + i as f32 * 12.,
            y: 32.,
        })
        .collect();
    let prepared = GlyphRun::new(&font, 24., &positions)?;
    let mut rows = Vec::new();
    for tessellate in [false, true] {
        let count = if tessellate { 100 } else { 2000 };
        for reuse in [false, true] {
            let mut measurements = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                for _ in 0..count {
                    let run = if reuse {
                        prepared.clone()
                    } else {
                        GlyphRun::new(&font, 24., &positions)?
                    };
                    let mut canvas = GpuCanvas::new_linear(512, 48, None);
                    canvas.set_fill("#4080c0");
                    canvas.fill_glyph_run(&run);
                    if tessellate {
                        black_box(canvas.into_ops());
                    } else {
                        black_box(canvas);
                    }
                }
                measurements.push(start.elapsed().as_secs_f64() * 1e6 / count as f64);
            }
            measurements.sort_by(f64::total_cmp);
            rows.push(json!({"reuse_run":reuse,"include_tessellation":tessellate,"iterations_per_repeat":count,"repeats":7,"median_us":measurements[3],"samples_us":measurements}));
        }
    }
    let report = json!({"status":"complete","glyphs":prepared.glyph_count(),"unique_glyphs":prepared.unique_glyph_count(),"scope":"CPU font outline preparation + Canvas recording, optionally tessellation; no GPU timing, shaping or frame-rate claim","rows":rows});
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, &json)?;
    }
    println!("{json}");
    Ok(())
}
