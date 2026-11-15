//! Pixel area of a non-overlapping tessellation, independently integrated in f64.
use crate::*;

pub(crate) fn triangle_area(tri: &PathTri, rect: [f64; 4]) -> f64 {
    let mut points = vec![
        (tri.a.0 as f64, tri.a.1 as f64),
        (tri.b.0 as f64, tri.b.1 as f64),
        (tri.c.0 as f64, tri.c.1 as f64),
    ];
    for (axis, edge, lower) in [
        (0, rect[0], true),
        (0, rect[2], false),
        (1, rect[1], true),
        (1, rect[3], false),
    ] {
        let mut clipped = Vec::with_capacity(7);
        if points.is_empty() {
            return 0.;
        }
        let component = |p: (f64, f64)| if axis == 0 { p.0 } else { p.1 };
        let mut a = *points.last().unwrap();
        for &b in &points {
            let inside = |v| if lower { v >= edge } else { v <= edge };
            let (av, bv) = (component(a), component(b));
            if inside(av) != inside(bv) {
                let t = (edge - av) / (bv - av);
                clipped.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
            if inside(bv) {
                clipped.push(b);
            }
            a = b;
        }
        points = clipped;
    }
    if points.len() < 3 {
        return 0.;
    }
    // Translate to the pixel origin before the shoelace sum to avoid cancellation.
    let origin = points[0];
    points
        .windows(2)
        .skip(1)
        .map(|p| {
            (p[0].0 - origin.0) * (p[1].1 - origin.1) - (p[0].1 - origin.1) * (p[1].0 - origin.0)
        })
        .sum::<f64>()
        .abs()
        * 0.5
}

pub(crate) fn paint(
    buf: &mut [f32],
    width: u32,
    height: u32,
    cmds: &[PathCommand],
    style: &PathStyle,
    clip: Option<[f32; 4]>,
) {
    let Some(fill) = build_fill_op(cmds, style) else {
        return;
    };
    if !fill.direct {
        return;
    }
    let min = fill.cover[0].a;
    let max = fill.cover[0].c;
    let [cx, cy, cw, ch] = clip.unwrap_or([0., 0., width as f32, height as f32]);
    let x0 = min.0.floor().max(cx.floor()).max(0.).min(width as f32) as u32;
    let y0 = min.1.floor().max(cy.floor()).max(0.).min(height as f32) as u32;
    let x1 = max.0.ceil().min((cx + cw).ceil()).max(0.).min(width as f32) as u32;
    let y1 = max
        .1
        .ceil()
        .min((cy + ch).ceil())
        .max(0.)
        .min(height as f32) as u32;
    if x1 <= x0 || y1 <= y0 || cw <= 0. || ch <= 0. {
        return;
    }
    let mut coverage = vec![0f64; ((x1 - x0) * (y1 - y0)) as usize];
    for tri in &fill.stencil {
        let ax = tri.a.0.min(tri.b.0).min(tri.c.0).floor().max(x0 as f32) as u32;
        let ay = tri.a.1.min(tri.b.1).min(tri.c.1).floor().max(y0 as f32) as u32;
        let bx = tri
            .a
            .0
            .max(tri.b.0)
            .max(tri.c.0)
            .ceil()
            .max(0.)
            .min(x1 as f32) as u32;
        let by = tri
            .a
            .1
            .max(tri.b.1)
            .max(tri.c.1)
            .ceil()
            .max(0.)
            .min(y1 as f32) as u32;
        for y in ay..by {
            for x in ax..bx {
                let rect = [
                    (x as f64).max(cx as f64),
                    (y as f64).max(cy as f64),
                    ((x + 1) as f64).min((cx + cw) as f64),
                    ((y + 1) as f64).min((cy + ch) as f64),
                ];
                if rect[2] > rect[0] && rect[3] > rect[1] {
                    coverage[((y - y0) * (x1 - x0) + x - x0) as usize] += triangle_area(tri, rect);
                }
            }
        }
    }
    let solid = style.fill.unwrap_or([1.; 4]);
    let bbox = fill.grad.map_or(
        [
            min.0 + 0.75,
            min.1 + 0.75,
            max.0 - min.0 - 1.5,
            max.1 - min.1 - 1.5,
        ],
        |g| g.bbox,
    );
    let sampler = style
        .fill_gradient
        .as_ref()
        .map(|g| crate::gradient::prepared_gradient_sampler(g, bbox[2], bbox[3]));
    for y in y0..y1 {
        for x in x0..x1 {
            let a = coverage[((y - y0) * (x1 - x0) + x - x0) as usize].clamp(0., 1.) as f32;
            if a == 0. {
                continue;
            }
            let mut c = solid;
            if let Some(sample) = &sampler {
                c = sample(
                    (x as f32 + 0.5 - bbox[0]) / bbox[2],
                    (y as f32 + 0.5 - bbox[1]) / bbox[3],
                );
                c[3] *= solid[3];
            }
            let alpha = (a * c[3]).clamp(0., 1.);
            let at = ((y * width + x) * 4) as usize;
            for k in 0..3 {
                buf[at + k] = c[k] * alpha + buf[at + k] * (1. - alpha);
            }
            buf[at + 3] = alpha + buf[at + 3] * (1. - alpha);
        }
    }
}
