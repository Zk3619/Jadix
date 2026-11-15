//! CPU capability gallery. Run with an optional output PNG path.
use jadix_2d::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "geometry-imaging.png".into());
    let mut c = RasterCanvas::new_hdr(640, 320);
    c.set_area_antialiasing(true);
    c.clear("#102030");
    let a = PathGeometry2D::from_path(
        &rounded_rect(16., 20., 88., 108., [5., 18., 8., 24.]),
        FillRule::NonZero,
        Default::default(),
    )?;
    let b = PathGeometry2D::from_path(
        &filled_circle(102., 78., 48., 96),
        FillRule::NonZero,
        Default::default(),
    )?;
    for (index, op) in [
        PathBooleanOp::Intersection,
        PathBooleanOp::Union,
        PathBooleanOp::Difference,
        PathBooleanOp::Xor,
    ]
    .into_iter()
    .enumerate()
    {
        c.save();
        c.translate(index as f32 * 160., 0.);
        c.set_fill("#ffb84f");
        a.boolean(&b, op, Default::default())?.fill(&mut c);
        c.restore();
    }
    let pixels = (0..64)
        .flat_map(|i| {
            let x = i % 8;
            let y = i / 8;
            let on = (x + y) % 3 != 0;
            [
                if on { 240 } else { 20 },
                if x < 4 { 60 } else { 200 },
                if y < 4 { 200 } else { 40 },
                if on { 255 } else { 0 },
            ]
        })
        .collect::<Vec<u8>>();
    let image = Image2D::from_rgba8(8, 8, &pixels).unwrap();
    for (index, quality) in [
        ImageSampling::Nearest,
        ImageSampling::Bilinear,
        ImageSampling::Bicubic,
        ImageSampling::Lanczos3,
    ]
    .into_iter()
    .enumerate()
    {
        c.save();
        c.set_image_sampling(quality);
        c.draw_image(
            &CanvasImageSource::Image(&image),
            index as f32 * 160. + 16.,
            174.,
            128.,
            128.,
        );
        c.restore();
    }
    image::save_buffer(
        output,
        c.get_image_data(0, 0, 640, 320).unwrap().as_slice(),
        640,
        320,
        image::ColorType::Rgba8,
    )?;
    Ok(())
}
