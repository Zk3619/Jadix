//! Conservative compact source domains. Unknown domain-dependent effects stay full-sized.
use crate::*;
type Bounds = [f32; 4];
fn union(a: Bounds, b: Bounds) -> Bounds {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn rect(c: [f32; 4]) -> Bounds {
    [c[0], c[1], c[0] + c[2], c[1] + c[3]]
}
fn expanded(b: Bounds, r: [f32; 2]) -> Bounds {
    [b[0] - r[0], b[1] - r[1], b[2] + r[0], b[3] + r[1]]
}
fn effects(mut b: Bounds, filters: &[LayerFilter], size: [u32; 2]) -> Option<Bounds> {
    let mut all = b;
    for f in filters.iter().filter(|f| f.valid()) {
        if f.creates_content() {
            return None;
        }
        let blur = |sigma: [f32; 2]| {
            [
                (4. * sigma[0]).ceil().min(size[0].saturating_sub(1) as f32),
                (4. * sigma[1]).ceil().min(size[1].saturating_sub(1) as f32),
            ]
        };
        b = match f {
            LayerFilter::Gaussian { sigma } => expanded(b, blur(*sigma)),
            LayerFilter::Morphology { radius, .. } => expanded(
                b,
                [radius[0].min(size[0]) as f32, radius[1].min(size[1]) as f32],
            ),
            LayerFilter::Offset { offset } => expanded(
                [
                    b[0] + offset[0],
                    b[1] + offset[1],
                    b[2] + offset[0],
                    b[3] + offset[1],
                ],
                [1.; 2],
            ),
            LayerFilter::DropShadow { sigma, offset, .. }
            | LayerFilter::DropShadowOnly { sigma, offset, .. } => {
                let shadow = expanded(
                    expanded(
                        [
                            b[0] + offset[0],
                            b[1] + offset[1],
                            b[2] + offset[0],
                            b[3] + offset[1],
                        ],
                        blur(*sigma),
                    ),
                    [1.; 2],
                );
                if matches!(f, LayerFilter::DropShadow { .. }) {
                    union(b, shadow)
                } else {
                    shadow
                }
            }
            LayerFilter::ConvolveMatrix(_) => return None,
            _ => b,
        };
        all = union(all, b);
    }
    Some(all)
}
fn content(ops: &[PathGpuOp], size: [u32; 2]) -> Option<Bounds> {
    let mut all = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for op in ops {
        if op.mask().is_some() {
            return None;
        }
        let mut b = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        let mut point = |p: (f32, f32)| {
            b[0] = b[0].min(p.0);
            b[1] = b[1].min(p.1);
            b[2] = b[2].max(p.0);
            b[3] = b[3].max(p.1);
        };
        match op {
            PathGpuOp::Layer(l) => {
                b = effects(content(&l.ops, size)?, &l.filters, size)?;
            }
            PathGpuOp::Fill(f) => {
                for t in &f.stencil {
                    for p in [t.a, t.b, t.c] {
                        point(p);
                    }
                }
            }
            PathGpuOp::Pattern(p) => {
                for t in &p.fill.stencil {
                    for p in [t.a, t.b, t.c] {
                        point(p);
                    }
                }
            }
            PathGpuOp::Mesh(m) => {
                for t in &m.tris {
                    for p in [t.a, t.b, t.c] {
                        point(p);
                    }
                }
            }
            PathGpuOp::Image(i) => {
                for p in i.corners {
                    point(p);
                }
            }
        }
        if let Some(c) = op.clip().filter(|_| !matches!(op, PathGpuOp::Layer(_))) {
            let c = rect(c);
            b = [
                b[0].max(c[0]),
                b[1].max(c[1]),
                b[2].min(c[2]),
                b[3].min(c[3]),
            ];
        }
        if b[2] > b[0] && b[3] > b[1] {
            all = union(all, b);
        }
    }
    Some(all)
}
pub(super) fn domain(ops: &[PathGpuOp], filters: &[LayerFilter], size: [u32; 2]) -> [u32; 4] {
    let full = [0, 0, size[0], size[1]];
    let Some(b) = content(ops, size).and_then(|b| effects(b, filters, size)) else {
        return full;
    };
    if !b.iter().all(|v| v.is_finite()) {
        return if ops.is_empty() { full } else { [0, 0, 2, 2] };
    }
    // One pixel guards rasterization/float rounding at the inferred domain edges.
    let x0 = (b[0].floor() - 1.)
        .max(0.)
        .min(size[0].saturating_sub(2) as f32) as u32;
    let y0 = (b[1].floor() - 1.)
        .max(0.)
        .min(size[1].saturating_sub(2) as f32) as u32;
    let x1 = (b[2].ceil() + 1.).max((x0 + 2) as f32).min(size[0] as f32) as u32;
    let y1 = (b[3].ceil() + 1.).max((y0 + 2) as f32).min(size[1] as f32) as u32;
    [x0, y0, x1 - x0, y1 - y0]
}
pub(super) fn shift(ops: &[PathGpuOp], origin: [u32; 2]) -> Vec<PathGpuOp> {
    let (dx, dy) = (-(origin[0] as f32), -(origin[1] as f32));
    fn inverse(m: &mut [f32; 6], dx: f32, dy: f32) {
        m[4] -= m[0] * dx + m[2] * dy;
        m[5] -= m[1] * dx + m[3] * dy;
    }
    fn fill(f: &mut PathGpuFill, dx: f32, dy: f32) {
        for t in f.stencil.iter_mut().chain(f.cover.iter_mut()) {
            for p in [&mut t.a, &mut t.b, &mut t.c] {
                p.0 += dx;
                p.1 += dy;
            }
        }
        if let Some(g) = &mut f.grad {
            inverse(&mut g.device_to_brush, dx, dy);
        }
        if let Some(c) = &mut f.clip {
            c[0] += dx;
            c[1] += dy;
        }
    }
    let mut result = ops.to_vec();
    for op in &mut result {
        match op {
            PathGpuOp::Fill(f) => fill(f, dx, dy),
            PathGpuOp::Pattern(p) => {
                fill(&mut p.fill, dx, dy);
                inverse(&mut p.device_to_brush, dx, dy);
            }
            PathGpuOp::Mesh(m) => {
                for t in &mut m.tris {
                    for p in [&mut t.a, &mut t.b, &mut t.c] {
                        p.0 += dx;
                        p.1 += dy;
                    }
                }
                if let Some(c) = &mut m.clip {
                    c[0] += dx;
                    c[1] += dy;
                }
            }
            PathGpuOp::Image(i) => {
                for p in &mut i.corners {
                    p.0 += dx;
                    p.1 += dy;
                }
                if let Some(c) = &mut i.clip {
                    c[0] += dx;
                    c[1] += dy;
                }
            }
            PathGpuOp::Layer(l) => {
                l.ops = shift(&l.ops, origin);
                if let Some(c) = &mut l.clip {
                    c[0] += dx;
                    c[1] += dy;
                }
            }
        }
    }
    result
}
