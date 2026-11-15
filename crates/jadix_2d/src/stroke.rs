//! Shared curve-offset geometry. Rasterization stays in Jadix; Kurbo supplies
//! stroke outlines and Lyon resolves their winding into disjoint triangles.
use crate::path::*;
use kurbo::{BezPath, Cap, Join, PathEl, Point};
use std::cell::RefCell;
thread_local! {
    static STROKER: RefCell<kurbo::StrokeCtx> = RefCell::new(kurbo::StrokeCtx::default());
    static TESSELLATOR: RefCell<(lyon_tessellation::FillTessellator,lyon_tessellation::VertexBuffers<Point2,u32>)> =
        RefCell::new((lyon_tessellation::FillTessellator::new(),lyon_tessellation::VertexBuffers::new()));
}

/// Additional SVG/Canvas stroke parameters, expressed in user-space units.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeOptions {
    pub miter_limit: f32,
    pub dash_array: Vec<f32>,
    pub dash_offset: f32,
}

impl Default for StrokeOptions {
    fn default() -> Self {
        Self {
            miter_limit: 4.0,
            dash_array: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl StrokeOptions {
    /// Invalid arrays are ignored by Canvas setters. Empty/all-zero means solid;
    /// odd-length arrays are repeated, as required by SVG and Canvas.
    pub(crate) fn normalized_dashes(values: &[f32]) -> Option<Vec<f32>> {
        if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return None;
        }
        if !values.iter().any(|v| *v > 0.0) {
            return Some(Vec::new());
        }
        let mut result = values.to_vec();
        if !result.len().is_multiple_of(2) {
            result.extend_from_slice(values);
        }
        Some(result)
    }
}

fn point(p: Point2) -> Point {
    Point::new(p.0 as f64, p.1 as f64)
}

/// Expand a stroke before applying a Canvas transform. `tolerance` is measured
/// in the same units as the path; callers scaling the result should reduce it.
/// Non-finite geometry and non-positive widths produce an empty outline.
pub fn stroke_outline(cmds: &[PathCommand], style: &PathStyle, tolerance: f32) -> Vec<PathCommand> {
    if !style.stroke_width.is_finite() || style.stroke_width <= 0.0 {
        return Vec::new();
    }
    let tolerance = if tolerance.is_finite() && tolerance > 0.0 {
        tolerance
    } else {
        0.05
    };
    let mut path = BezPath::new();
    let mut current = Point::ZERO;
    let mut start = current;
    let mut open = false;
    for cmd in cmds {
        // Validate control points as well as endpoints before entering either library.
        let valid = match *cmd {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                [p.0, p.1].iter().all(|v| v.is_finite())
            }
            PathCommand::QuadTo { ctrl, to } => {
                [ctrl.0, ctrl.1, to.0, to.1].iter().all(|v| v.is_finite())
            }
            PathCommand::CubicTo { c1, c2, to } => [c1.0, c1.1, c2.0, c2.1, to.0, to.1]
                .iter()
                .all(|v| v.is_finite()),
            PathCommand::Arc {
                radii,
                x_rot_deg,
                to,
                ..
            } => [radii.0, radii.1, x_rot_deg, to.0, to.1]
                .iter()
                .all(|v| v.is_finite()),
            PathCommand::Close => true,
        };
        if !valid {
            return Vec::new();
        }
        if !open && !matches!(cmd, PathCommand::MoveTo(_) | PathCommand::Close) {
            path.move_to(current);
            start = current;
            open = true;
        }
        match *cmd {
            PathCommand::MoveTo(p) => {
                current = point(p);
                start = current;
                path.move_to(current);
                open = true;
            }
            PathCommand::LineTo(p) => {
                current = point(p);
                path.line_to(current);
            }
            PathCommand::QuadTo { ctrl, to } => {
                current = point(to);
                path.quad_to(point(ctrl), current);
            }
            PathCommand::CubicTo { c1, c2, to } => {
                current = point(to);
                path.curve_to(point(c1), point(c2), current);
            }
            PathCommand::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => {
                let to = point(to);
                let arc = kurbo::SvgArc {
                    from: current,
                    to,
                    radii: kurbo::Vec2::new(radii.0 as f64, radii.1 as f64),
                    x_rotation: (x_rot_deg as f64).to_radians(),
                    large_arc: large,
                    sweep,
                };
                if let Some(arc) = kurbo::Arc::from_svg_arc(&arc) {
                    arc.to_cubic_beziers(tolerance as f64, |a, b, c| path.curve_to(a, b, c));
                } else if current != to {
                    path.line_to(to);
                }
                current = to;
            }
            PathCommand::Close => {
                if open {
                    path.close_path();
                }
                current = start;
                open = false;
            }
        }
    }
    let cap = match style.stroke_linecap {
        StrokeLinecap::Butt => Cap::Butt,
        StrokeLinecap::Round => Cap::Round,
        StrokeLinecap::Square => Cap::Square,
    };
    let join = match style.stroke_linejoin {
        StrokeLinejoin::Miter => Join::Miter,
        StrokeLinejoin::Round => Join::Round,
        StrokeLinejoin::Bevel => Join::Bevel,
    };
    let options = &style.stroke_options;
    let Some(dashes) = StrokeOptions::normalized_dashes(&options.dash_array) else {
        return Vec::new();
    };
    if !options.dash_offset.is_finite()
        || !options.miter_limit.is_finite()
        || options.miter_limit < 1.0
    {
        return Vec::new();
    }
    let stroke = kurbo::Stroke::new(style.stroke_width as f64)
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(options.miter_limit as f64)
        .with_dashes(options.dash_offset as f64, dashes.iter().map(|v| *v as f64));
    STROKER.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        kurbo::stroke_with(
            path.iter(),
            &stroke,
            &kurbo::StrokeOpts::default(),
            tolerance as f64,
            &mut ctx,
        );
        ctx.output()
            .iter()
            .map(|el| {
                let p = |p: Point| (p.x as f32, p.y as f32);
                match el {
                    PathEl::MoveTo(a) => PathCommand::MoveTo(p(a)),
                    PathEl::LineTo(a) => PathCommand::LineTo(p(a)),
                    PathEl::QuadTo(a, b) => PathCommand::QuadTo {
                        ctrl: p(a),
                        to: p(b),
                    },
                    PathEl::CurveTo(a, b, c) => PathCommand::CubicTo {
                        c1: p(a),
                        c2: p(b),
                        to: p(c),
                    },
                    PathEl::ClosePath => PathCommand::Close,
                }
            })
            .collect()
    })
}

pub(crate) fn fill_mesh(
    cmds: &[PathCommand],
    color: [f32; 4],
    rule: FillRule,
) -> Option<Vec<PathTri>> {
    use lyon_tessellation::math::point;
    use lyon_tessellation::{BuffersBuilder, FillOptions, FillVertex};
    // Endpoint arcs are uncommon here (stroke outlines already contain Beziers).
    // Retain Jadix's arc expansion; feed Beziers directly to Lyon's event builder.
    let flat;
    let cmds = if cmds
        .iter()
        .any(|cmd| matches!(cmd, PathCommand::Arc { .. }))
    {
        flat = flatten_path_with_tolerance(cmds, 0.05);
        &flat[..]
    } else {
        cmds
    };
    TESSELLATOR.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let (tessellator, buffers) = &mut *ctx;
        buffers.vertices.clear();
        buffers.indices.clear();
        let options = FillOptions::default()
            .with_tolerance(0.05)
            .with_fill_rule(match rule {
                FillRule::NonZero => lyon_tessellation::FillRule::NonZero,
                FillRule::EvenOdd => lyon_tessellation::FillRule::EvenOdd,
            });
        {
            let mut output = BuffersBuilder::new(buffers, |v: FillVertex| {
                let p = v.position();
                (p.x, p.y)
            });
            let mut builder = tessellator.builder(&options, &mut output);
            let mut open = false;
            let mut subpath_start = None;
            for cmd in cmds {
                let point2 = |p: Point2| point(p.0, p.1);
                let valid = match *cmd {
                    PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                        [p.0, p.1].iter().all(|v| v.is_finite())
                    }
                    PathCommand::QuadTo { ctrl, to } => {
                        [ctrl.0, ctrl.1, to.0, to.1].iter().all(|v| v.is_finite())
                    }
                    PathCommand::CubicTo { c1, c2, to } => [c1.0, c1.1, c2.0, c2.1, to.0, to.1]
                        .iter()
                        .all(|v| v.is_finite()),
                    PathCommand::Close => true,
                    PathCommand::Arc { .. } => false,
                };
                if !valid {
                    return None;
                }
                match *cmd {
                    PathCommand::MoveTo(p) => {
                        if open {
                            builder.end(true);
                        }
                        builder.begin(point2(p));
                        open = true;
                        subpath_start = Some(p);
                    }
                    PathCommand::LineTo(p) => {
                        if open {
                            builder.line_to(point2(p));
                        } else {
                            builder.begin(point2(subpath_start.unwrap_or(p)));
                            open = true;
                            if subpath_start.is_some() {
                                builder.line_to(point2(p));
                            }
                        }
                    }
                    PathCommand::QuadTo { ctrl, to } => {
                        if !open {
                            builder.begin(point2(subpath_start.unwrap_or((0., 0.))));
                            open = true;
                        }
                        builder.quadratic_bezier_to(point2(ctrl), point2(to));
                    }
                    PathCommand::CubicTo { c1, c2, to } => {
                        if !open {
                            builder.begin(point2(subpath_start.unwrap_or((0., 0.))));
                            open = true;
                        }
                        builder.cubic_bezier_to(point2(c1), point2(c2), point2(to));
                    }
                    PathCommand::Close => {
                        if open {
                            builder.end(true);
                            open = false;
                        }
                    }
                    PathCommand::Arc { .. } => unreachable!(),
                }
            }
            if open {
                builder.end(true);
            }
            builder.build().ok()?;
        }
        Some(
            buffers
                .indices
                .chunks_exact(3)
                .map(|ids| PathTri {
                    a: buffers.vertices[ids[0] as usize],
                    b: buffers.vertices[ids[1] as usize],
                    c: buffers.vertices[ids[2] as usize],
                    color,
                })
                .collect(),
        )
    })
}
