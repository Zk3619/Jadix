//! Run with an output PPM path; no window or GPU is required.
use jadix_2d::{gradient::parse_gradient, *};
use std::io::Write;

fn main() -> std::io::Result<()> {
    let mut c = RasterCanvas::new(600, 300);
    c.clear("#101827");
    for y in (0..300).step_by(20) {
        for x in (0..600).step_by(20) {
            c.set_fill(if (x + y) / 20 % 2 == 0 {
                "#172236"
            } else {
                "#1d2940"
            });
            c.fill_rect(x as f32, y as f32, 20.0, 20.0);
        }
    }
    c.set_fill_gradient(
        parse_gradient("conic-gradient(#22d3ee, #a78bfa, #f472b6, #22d3ee)").unwrap(),
    );
    c.set_fill_rule(FillRule::EvenOdd);
    let mut donut = filled_circle(105.0, 145.0, 80.0, 64);
    donut.extend(filled_circle(105.0, 145.0, 39.0, 64));
    c.fill_path(&donut);
    c.set_fill_rule(FillRule::NonZero);
    c.save();
    c.translate(315.0, 145.0);
    c.rotate(27.0);
    c.clip_rect(-66.0, -82.0, 132.0, 164.0);
    c.reset_transform();
    c.set_fill_gradient(
        parse_gradient("repeating-linear-gradient(45deg, #38bdf8 0%, #c084fc 12%, #f472b6 24%)")
            .unwrap(),
    );
    c.fill_rect(220.0, 40.0, 190.0, 220.0);
    c.restore();
    c.push_layer(0.8, BlendMode::Screen);
    c.set_fill("#f43f5e");
    c.fill_path(&filled_circle(477.0, 120.0, 49.0, 64));
    c.set_fill("#22c55e");
    c.fill_path(&filled_circle(523.0, 120.0, 49.0, 64));
    c.set_fill("#3b82f6");
    c.fill_path(&filled_circle(500.0, 166.0, 49.0, 64));
    c.pop_layer();
    let rgba = c.get_image_data(0, 0, 600, 300).unwrap();
    let file = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "advanced-scene.ppm".into());
    let mut output = std::fs::File::create(file)?;
    output.write_all(b"P6\n600 300\n255\n")?;
    for p in rgba.chunks_exact(4) {
        output.write_all(&p[..3])?;
    }
    Ok(())
}
