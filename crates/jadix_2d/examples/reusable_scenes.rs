//! cargo run -p jadix_2d --example reusable_scenes -- target/reusable-scenes.png
use jadix_2d::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or("target/reusable-scenes.png".into());
    let stops = GradientStops::new(&[
        (0., [0.05, 0.8, 1., 1.]),
        (0.5, [0.35, 0.3, 1., 0.9]),
        (1., [1., 0.2, 0.5, 1.]),
    ])?;
    let icon = Scene2D::record([48, 48], |c| {
        c.set_fill_brush(Brush2D::vector(
            VectorGradient::linear([4., 4.], [44., 44.], stops).unwrap(),
        ));
        c.fill_path(&filled_circle(24., 24., 21., 48));
        c.set_line_width(3.);
        c.set_stroke("#ffffff");
        c.stroke_path(&polyline(&[(13., 25.), (21., 33.), (36., 15.)]));
    })?;
    let pair = Scene2D::record([100, 56], |c| {
        for x in [0., 45.] {
            icon.draw(
                c,
                SceneOptions {
                    transform: [1., 0., 0., 1., x, 4.],
                    ..Default::default()
                },
            )
            .unwrap();
        }
    })?;
    let mut canvas = RasterCanvas::new_linear(384, 240);
    canvas.clear("#101a30");
    let transforms = [
        [1., 0., 0., 1., 12., 20.],
        [0.9, 0.25, -0.25, 0.9, 148., 12.],
        [-1., 0., 0., 1., 374., 20.],
        [1., 0., 0., 1., 12., 142.],
        [1., 0., 0.25, 0.8, 136., 150.],
        [1., 0., 0., 1., 268., 142.],
    ];
    for (i, transform) in transforms.into_iter().enumerate() {
        pair.draw(
            &mut canvas,
            SceneOptions {
                transform,
                layer: Some(LayerOptions {
                    opacity: if i == 4 { 0.5 } else { 1. },
                    filters: match i {
                        3 => vec![LayerFilter::Gaussian { sigma: [1.5, 1.5] }],
                        5 => vec![LayerFilter::DropShadow {
                            sigma: [3., 3.],
                            offset: [4., 5.],
                            color: [0., 0.7, 1., 0.8],
                        }],
                        _ => vec![],
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
        )?;
    }
    let path = std::path::Path::new(&output);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(
        path,
        &canvas.get_image_data(0, 0, 384, 240).unwrap(),
        384,
        240,
        image::ColorType::Rgba8,
    )?;
    println!("{}; shared pair: {:?}", path.display(), pair.stats());
    Ok(())
}
