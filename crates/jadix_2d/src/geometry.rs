//! Bounded polygonal Boolean geometry, prepared once and reusable across canvases.
use crate::{flatten_path_with_tolerance, Canvas2D, FillRule, PathCommand};
use std::sync::{Arc, OnceLock};

type Point = [f64; 2];
type Triangle = [Point; 3];

/// Regularized set operations on filled regions, before raster coverage.
/// Zero-area contacts (isolated points/lines) are discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathBooleanOp {
    Union,
    Intersection,
    Difference,
    Xor,
}
/// Explicit preparation budgets. Failure leaves both input geometries unchanged.
#[derive(Clone, Copy, Debug)]
pub struct PathBooleanOptions {
    /// User-space chord tolerance, in [0.001, 1]. Curves become line segments.
    pub tolerance: f32,
    pub max_path_commands: usize,
    pub max_flattened_commands: usize,
    /// Applies to input tessellation, intermediate fragments and output triangles.
    pub max_triangles: usize,
    /// Triangle tests and half-plane vertex tests per Boolean call.
    pub max_work: usize,
}
impl Default for PathBooleanOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.05,
            max_path_commands: 4096,
            max_flattened_commands: 65536,
            max_triangles: 16384,
            max_work: 2_000_000,
        }
    }
}
impl PathBooleanOptions {
    fn validate(self) -> Result<(), PathBooleanError> {
        if !self.tolerance.is_finite()
            || !(0.001..=1.).contains(&self.tolerance)
            || self.max_path_commands == 0
            || self.max_path_commands > 65536
            || self.max_flattened_commands == 0
            || self.max_flattened_commands > 262144
            || self.max_triangles == 0
            || self.max_triangles > 65536
            || self.max_work == 0
            || self.max_work > 16_000_000
        {
            return Err(PathBooleanError::InvalidOptions);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathBooleanError {
    InvalidOptions,
    /// Nonfinite coordinates, or magnitudes greater than 10^7 user units.
    InvalidGeometry,
    PathBudget,
    TriangleBudget,
    WorkBudget,
    Tessellation,
}
impl std::fmt::Display for PathBooleanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PathBooleanError {}
#[derive(Debug)]
struct Geometry {
    triangles: Vec<Triangle>,
    path: OnceLock<Vec<PathCommand>>,
}
/// Immutable filled region, shared on clone. Input winding/holes/self-intersections
/// are resolved by Lyon in f32; Boolean clipping then uses f64 polygons. This is
/// approximate polygon geometry, not exact curve algebra or a stroke outline.
///
/// ```
/// use jadix_2d::*;
/// let shape = PathGeometry2D::from_path(&filled_rect(0., 0., 8., 8.), FillRule::NonZero,
///     PathBooleanOptions::default()).unwrap();
/// let clip = PathGeometry2D::from_path(&filled_circle(4., 4., 4., 64), FillRule::NonZero,
///     PathBooleanOptions::default()).unwrap();
/// let visible = shape.boolean(&clip, PathBooleanOp::Intersection,
///     PathBooleanOptions::default()).unwrap();
/// let mut canvas = RasterCanvas::new_linear(8, 8);
/// canvas.set_fill("white");
/// visible.fill(&mut canvas);
/// ```
#[derive(Clone, Debug)]
pub struct PathGeometry2D(Arc<Geometry>);
impl PathGeometry2D {
    pub fn from_path(
        path: &[PathCommand],
        rule: FillRule,
        options: PathBooleanOptions,
    ) -> Result<Self, PathBooleanError> {
        options.validate()?;
        if path.len() > options.max_path_commands {
            return Err(PathBooleanError::PathBudget);
        }
        let mut flat = Vec::new();
        let mut current = (0., 0.);
        let mut start = current;
        for command in path {
            let mut valid = true;
            command.clone().map_xy(|p| {
                valid &= [p.0, p.1].iter().all(|v| v.is_finite() && v.abs() <= 1e7);
                p
            });
            if let PathCommand::Arc {
                radii, x_rot_deg, ..
            } = command
            {
                valid &= x_rot_deg.is_finite()
                    && [radii.0, radii.1]
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 1e7);
            }
            if !valid {
                return Err(PathBooleanError::InvalidGeometry);
            }
            match command {
                PathCommand::MoveTo(p) => {
                    current = *p;
                    start = *p;
                    flat.push(command.clone());
                }
                PathCommand::Close => {
                    current = start;
                    flat.push(PathCommand::Close);
                }
                PathCommand::LineTo(p) => {
                    current = *p;
                    flat.push(command.clone());
                }
                _ => {
                    // One command at a time bounds the temporary flatten allocation
                    // (the shared flattener caps subdivision at 65536 segments).
                    let piece = flatten_path_with_tolerance(
                        &[PathCommand::MoveTo(current), command.clone()],
                        options.tolerance,
                    );
                    if flat.len() + piece.len() - 1 > options.max_flattened_commands {
                        return Err(PathBooleanError::PathBudget);
                    }
                    flat.extend(piece.into_iter().skip(1));
                    current = command.pen().unwrap();
                }
            }
            if flat.len() > options.max_flattened_commands {
                return Err(PathBooleanError::PathBudget);
            }
        }
        // Bound tessellation output while it is emitted, rather than checking
        // an already allocated unbounded intermediate mesh afterwards.
        let mut output = BoundedFill {
            vertices: Vec::new(),
            triangles: Vec::new(),
            limit: options.max_triangles,
            overflow: false,
            invalid: false,
        };
        let mut tessellator = lyon_tessellation::FillTessellator::new();
        let fill_options = lyon_tessellation::FillOptions::default().with_fill_rule(match rule {
            FillRule::NonZero => lyon_tessellation::FillRule::NonZero,
            FillRule::EvenOdd => lyon_tessellation::FillRule::EvenOdd,
        });
        let result = {
            let mut builder = tessellator.builder(&fill_options, &mut output);
            let mut open = false;
            let mut subpath_start = None;
            for command in flat {
                match command {
                    PathCommand::MoveTo(p) => {
                        if open {
                            builder.end(true);
                        }
                        builder.begin(lyon_tessellation::math::point(p.0, p.1));
                        open = true;
                        subpath_start = Some(lyon_tessellation::math::point(p.0, p.1));
                    }
                    PathCommand::LineTo(p) => {
                        let p = lyon_tessellation::math::point(p.0, p.1);
                        if open {
                            builder.line_to(p);
                        } else {
                            builder.begin(subpath_start.unwrap_or(p));
                            if subpath_start.is_some() {
                                builder.line_to(p);
                            }
                            open = true;
                        }
                    }
                    PathCommand::Close => {
                        if open {
                            builder.end(true);
                            open = false;
                        }
                    }
                    _ => unreachable!("flattener emits only line geometry"),
                }
            }
            if open {
                builder.end(true);
            }
            builder.build()
        };
        if output.invalid {
            return Err(PathBooleanError::InvalidGeometry);
        }
        if output.overflow {
            return Err(PathBooleanError::TriangleBudget);
        }
        result.map_err(|_| PathBooleanError::Tessellation)?;
        Ok(Self::new(output.triangles))
    }

    fn new(triangles: Vec<Triangle>) -> Self {
        Self(Arc::new(Geometry {
            triangles,
            path: OnceLock::new(),
        }))
    }
    /// Operate on already resolved, non-overlapping triangles. Worst-case work
    /// is quadratic plus fragmentation; max_work/max_triangles fail explicitly.
    pub fn boolean(
        &self,
        other: &Self,
        op: PathBooleanOp,
        options: PathBooleanOptions,
    ) -> Result<Self, PathBooleanError> {
        options.validate()?;
        if self.triangle_count() > options.max_triangles
            || other.triangle_count() > options.max_triangles
        {
            return Err(PathBooleanError::TriangleBudget);
        }
        let mut budget = Budget { options, work: 0 };
        let a = &self.0.triangles;
        let b = &other.0.triangles;
        let mut out = Vec::new();
        match op {
            PathBooleanOp::Intersection => {
                for ta in a {
                    for tb in b {
                        budget.spend(1)?;
                        if !overlap(ta, tb) {
                            continue;
                        }
                        let mut polygon = ta.to_vec();
                        for i in 0..3 {
                            polygon = cut(&polygon, tb[i], tb[(i + 1) % 3], true, &mut budget)?;
                        }
                        triangulate(&polygon, &mut out, options.max_triangles)?;
                    }
                }
            }
            PathBooleanOp::Difference => difference(a, b, &mut out, &mut budget)?,
            PathBooleanOp::Union => {
                out.extend_from_slice(a);
                difference(b, a, &mut out, &mut budget)?;
            }
            PathBooleanOp::Xor => {
                difference(a, b, &mut out, &mut budget)?;
                difference(b, a, &mut out, &mut budget)?;
            }
        }
        Ok(Self::new(out))
    }
    /// Transform the resolved mesh without flattening again. Zero-area triangles
    /// are discarded. Reflections preserve positive winding in to_path().
    pub fn transformed(&self, matrix: [f64; 6]) -> Result<Self, PathBooleanError> {
        if matrix.iter().any(|v| !v.is_finite()) {
            return Err(PathBooleanError::InvalidGeometry);
        }
        let mut triangles = Vec::with_capacity(self.triangle_count());
        for t in &self.0.triangles {
            let t = t.map(|p| {
                [
                    matrix[0] * p[0] + matrix[2] * p[1] + matrix[4],
                    matrix[1] * p[0] + matrix[3] * p[1] + matrix[5],
                ]
            });
            add_triangle(&mut triangles, t, self.triangle_count())?;
        }
        Ok(Self::new(triangles))
    }
    pub fn triangle_count(&self) -> usize {
        self.0.triangles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.triangles.is_empty()
    }
    /// Resolved polygon area in user units squared, independent of pixels.
    pub fn area(&self) -> f64 {
        self.0
            .triangles
            .iter()
            .map(|t| cross(t[0], t[1], t[2]) * 0.5)
            .sum()
    }
    /// Boundary points of retained area triangles are included. Zero-area Boolean
    /// contacts are discarded. No raster/AA threshold is applied.
    pub fn contains(&self, point: [f64; 2]) -> bool {
        point.iter().all(|v| v.is_finite())
            && self
                .0
                .triangles
                .iter()
                .any(|t| (0..3).all(|i| cross(t[i], t[(i + 1) % 3], point) >= 0.))
    }
    /// [min_x, min_y, max_x, max_y], or None for an empty region.
    pub fn bounds(&self) -> Option<[f64; 4]> {
        if self.is_empty() {
            return None;
        }
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in self.0.triangles.iter().flatten() {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
        }
        Some(b)
    }
    /// Lazily cached f32 triangle contours with NonZero fill. Use for filling,
    /// brushes, hit testing, clips or Scene2D recording; stroking exposes internal
    /// triangle edges and is not a Boolean boundary-outline operation.
    pub fn to_path(&self) -> &[PathCommand] {
        self.0.path.get_or_init(|| {
            self.0
                .triangles
                .iter()
                .flat_map(|t| {
                    let p = t.map(|p| (p[0] as f32, p[1] as f32));
                    [
                        PathCommand::MoveTo(p[0]),
                        PathCommand::LineTo(p[1]),
                        PathCommand::LineTo(p[2]),
                        PathCommand::Close,
                    ]
                })
                .collect()
        })
    }
    /// Fill once with the current paint/CTM/alpha. Works with scene recorders
    /// and custom canvases implementing the existing fill rule/state contract.
    pub fn fill(&self, canvas: &mut (impl Canvas2D + ?Sized)) {
        canvas.save();
        canvas.set_fill_rule(FillRule::NonZero);
        canvas.fill_path(self.to_path());
        canvas.restore();
    }
}
fn cross(a: Point, b: Point, p: Point) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn add_triangle(
    out: &mut Vec<Triangle>,
    mut t: Triangle,
    max: usize,
) -> Result<(), PathBooleanError> {
    if t.iter().flatten().any(|v| !v.is_finite() || v.abs() > 1e7) {
        return Err(PathBooleanError::InvalidGeometry);
    }
    let area = cross(t[0], t[1], t[2]);
    if area == 0. {
        return Ok(());
    }
    if out.len() >= max {
        return Err(PathBooleanError::TriangleBudget);
    }
    if area < 0. {
        t.swap(1, 2);
    }
    out.push(t);
    Ok(())
}
fn triangulate(p: &[Point], out: &mut Vec<Triangle>, max: usize) -> Result<(), PathBooleanError> {
    for i in 1..p.len().saturating_sub(1) {
        add_triangle(out, [p[0], p[i], p[i + 1]], max)?;
    }
    Ok(())
}
struct Budget {
    options: PathBooleanOptions,
    work: usize,
}
impl Budget {
    fn spend(&mut self, n: usize) -> Result<(), PathBooleanError> {
        self.work += n;
        if self.work > self.options.max_work {
            return Err(PathBooleanError::WorkBudget);
        }
        Ok(())
    }
}
fn overlap(a: &[Point], b: &[Point]) -> bool {
    (0..2).all(|axis| {
        let amin = a.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let amax = a.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
        let bmin = b.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let bmax = b.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
        amax > bmin && bmax > amin
    })
}
fn cut(
    p: &[Point],
    a: Point,
    b: Point,
    inside: bool,
    budget: &mut Budget,
) -> Result<Vec<Point>, PathBooleanError> {
    budget.spend(p.len())?;
    let mut out = Vec::with_capacity(p.len() + 1);
    if p.is_empty() {
        return Ok(out);
    }
    let mut prev = p[p.len() - 1];
    let mut dp = cross(a, b, prev);
    for &next in p {
        let dn = cross(a, b, next);
        let ip = if inside { dp >= 0. } else { dp <= 0. };
        let inn = if inside { dn >= 0. } else { dn <= 0. };
        if ip != inn {
            let t = dp / (dp - dn);
            out.push([
                prev[0] + t * (next[0] - prev[0]),
                prev[1] + t * (next[1] - prev[1]),
            ]);
        }
        if inn {
            out.push(next);
        }
        prev = next;
        dp = dn;
    }
    Ok(out)
}
fn difference(
    a: &[Triangle],
    b: &[Triangle],
    out: &mut Vec<Triangle>,
    budget: &mut Budget,
) -> Result<(), PathBooleanError> {
    // ponytail: bounded polygon fragmentation; replace with a sweep-line engine
    // if measured authoring workloads routinely hit the explicit work budget.
    for triangle in a {
        let mut pieces = vec![triangle.to_vec()];
        for clip in b {
            let mut next = Vec::new();
            for polygon in pieces {
                budget.spend(1)?;
                if !overlap(&polygon, clip) {
                    if next.len() >= budget.options.max_triangles {
                        return Err(PathBooleanError::TriangleBudget);
                    }
                    next.push(polygon);
                    continue;
                }
                let mut remaining = polygon;
                for i in 0..3 {
                    let outside = cut(&remaining, clip[i], clip[(i + 1) % 3], false, budget)?;
                    if outside.len() >= 3 && polygon_area(&outside) > 0. {
                        next.push(outside);
                    }
                    if next.len() > budget.options.max_triangles {
                        return Err(PathBooleanError::TriangleBudget);
                    }
                    remaining = cut(&remaining, clip[i], clip[(i + 1) % 3], true, budget)?;
                    if remaining.len() < 3 {
                        break;
                    }
                }
            }
            pieces = next;
            if pieces.is_empty() {
                break;
            }
        }
        for polygon in pieces {
            triangulate(&polygon, out, budget.options.max_triangles)?;
        }
    }
    Ok(())
}
fn polygon_area(p: &[Point]) -> f64 {
    (1..p.len().saturating_sub(1))
        .map(|i| cross(p[0], p[i], p[i + 1]).abs() * 0.5)
        .sum()
}

struct BoundedFill {
    vertices: Vec<Point>,
    triangles: Vec<Triangle>,
    limit: usize,
    overflow: bool,
    invalid: bool,
}
impl lyon_tessellation::GeometryBuilder for BoundedFill {
    fn add_triangle(
        &mut self,
        a: lyon_tessellation::VertexId,
        b: lyon_tessellation::VertexId,
        c: lyon_tessellation::VertexId,
    ) {
        if self.triangles.len() >= self.limit {
            self.overflow = true;
            return;
        }
        let mut t = [a, b, c].map(|i| self.vertices[i.0 as usize]);
        let area = cross(t[0], t[1], t[2]);
        if area == 0. {
            return;
        }
        if area < 0. {
            t.swap(1, 2);
        }
        self.triangles.push(t);
    }
    fn abort_geometry(&mut self) {
        self.vertices.clear();
        self.triangles.clear();
    }
}
impl lyon_tessellation::FillGeometryBuilder for BoundedFill {
    fn add_fill_vertex(
        &mut self,
        vertex: lyon_tessellation::FillVertex,
    ) -> Result<lyon_tessellation::VertexId, lyon_tessellation::GeometryBuilderError> {
        if self.overflow || self.vertices.len() >= self.limit * 3 {
            self.overflow = true;
            return Err(lyon_tessellation::GeometryBuilderError::TooManyVertices);
        }
        let p = vertex.position();
        if [p.x, p.y].iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            self.invalid = true;
            return Err(lyon_tessellation::GeometryBuilderError::TooManyVertices);
        }
        let id = lyon_tessellation::VertexId(self.vertices.len() as u32);
        self.vertices.push([p.x as f64, p.y as f64]);
        Ok(id)
    }
}
