use std::rc::Rc;

use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::path::{
    ATTR_PATH_D, ATTR_PATH_KIND, ATTR_PATH_POINTS, PathCommand, Point2, encode_path_d, flatten_path,
};
use crate::style::AnimatedNumber;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSample {
    pub position: Point2,
    pub tangent_degrees: f32,
}

#[derive(Clone, Debug)]
struct Segment {
    from: Point2,
    to: Point2,
    start: f32,
    length: f32,
    subpath: usize,
}

/// Curves are flattened and measured once. Disconnected subpaths stay disconnected.
#[derive(Clone, Debug)]
pub struct PreparedPath {
    commands: Vec<PathCommand>,
    segments: Vec<Segment>,
    length: f32,
    first: Point2,
}

impl PreparedPath {
    pub fn new(commands: &[PathCommand]) -> Self {
        assert!(
            commands.iter().all(command_is_finite),
            "path coordinates must be finite"
        );
        let mut segments = Vec::new();
        let mut cursor = (0.0, 0.0);
        let mut start = cursor;
        let mut first = None;
        let mut length = 0.0;
        let mut subpath = 0;
        for command in flatten_path(commands) {
            let to = match command {
                PathCommand::MoveTo(point) => {
                    cursor = point;
                    start = point;
                    first.get_or_insert(point);
                    subpath += 1;
                    continue;
                }
                PathCommand::LineTo(point) => point,
                PathCommand::Close => start,
                _ => unreachable!("flatten_path returns only lines and moves"),
            };
            let segment_length = (to.0 - cursor.0).hypot(to.1 - cursor.1);
            if segment_length > 1e-6 {
                segments.push(Segment {
                    from: cursor,
                    to,
                    start: length,
                    length: segment_length,
                    subpath,
                });
                length += segment_length;
            }
            cursor = to;
        }
        assert!(length.is_finite(), "path length must be finite");
        Self {
            commands: commands.to_vec(),
            segments,
            length,
            first: first.unwrap_or((0.0, 0.0)),
        }
    }

    pub fn length(&self) -> f32 {
        self.length
    }

    pub fn sample(&self, progress: f32) -> PathSample {
        let distance = normalized(progress) * self.length;
        let index = self
            .segments
            .partition_point(|segment| segment.start + segment.length < distance);
        let Some(segment) = self.segments.get(index).or_else(|| self.segments.last()) else {
            return PathSample {
                position: self.first,
                tangent_degrees: 0.0,
            };
        };
        PathSample {
            position: point_on(segment, (distance - segment.start) / segment.length),
            tangent_degrees: (segment.to.1 - segment.from.1)
                .atan2(segment.to.0 - segment.from.0)
                .to_degrees(),
        }
    }

    /// A normalized arc-length window, suitable for stroke reveals and dashes.
    pub fn trim(&self, start: f32, end: f32) -> Vec<PathCommand> {
        let start = normalized(start);
        let end = normalized(end);
        if start >= end || self.length <= 1e-6 {
            return Vec::new();
        }
        if start == 0.0 && end == 1.0 {
            return self.commands.clone();
        }
        let from = start * self.length;
        let to = end * self.length;
        let mut output = Vec::new();
        let mut previous_subpath = None;
        let index = self
            .segments
            .partition_point(|segment| segment.start + segment.length <= from);
        for segment in &self.segments[index..] {
            if segment.start >= to {
                break;
            }
            let begin = point_on(segment, (from - segment.start) / segment.length);
            let finish = point_on(segment, (to - segment.start) / segment.length);
            if previous_subpath != Some(segment.subpath) {
                output.push(PathCommand::MoveTo(begin));
            }
            output.push(PathCommand::LineTo(finish));
            previous_subpath = Some(segment.subpath);
        }
        output
    }
}

fn normalized(progress: f32) -> f32 {
    if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn point_on(segment: &Segment, progress: f32) -> Point2 {
    lerp_point(segment.from, segment.to, progress.clamp(0.0, 1.0))
}

fn lerp_point(from: Point2, to: Point2, progress: f32) -> Point2 {
    (
        from.0 + (to.0 - from.0) * progress,
        from.1 + (to.1 - from.1) * progress,
    )
}

fn command_is_finite(command: &PathCommand) -> bool {
    let finite = |point: Point2| point.0.is_finite() && point.1.is_finite();
    match *command {
        PathCommand::MoveTo(point) | PathCommand::LineTo(point) => finite(point),
        PathCommand::QuadTo { ctrl, to } => finite(ctrl) && finite(to),
        PathCommand::CubicTo { c1, c2, to } => finite(c1) && finite(c2) && finite(to),
        PathCommand::Arc {
            radii,
            x_rot_deg,
            to,
            ..
        } => finite(radii) && x_rot_deg.is_finite() && finite(to),
        PathCommand::Close => true,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathMorphError {
    IncompatibleTopology { command: usize },
    NonFiniteCoordinates,
}

impl std::fmt::Display for PathMorphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleTopology { command } => {
                write!(formatter, "incompatible path topology at command {command}")
            }
            Self::NonFiniteCoordinates => formatter.write_str("path coordinates must be finite"),
        }
    }
}

impl std::error::Error for PathMorphError {}

/// Morphs matching command sequences, preserving curve controls rather than a raster.
#[derive(Clone, Debug)]
pub struct PathMorph {
    from: Vec<PathCommand>,
    to: Vec<PathCommand>,
}

impl PathMorph {
    pub fn new(from: &[PathCommand], to: &[PathCommand]) -> Result<Self, PathMorphError> {
        if !from.iter().chain(to).all(command_is_finite) {
            return Err(PathMorphError::NonFiniteCoordinates);
        }
        if from.len() != to.len() {
            return Err(PathMorphError::IncompatibleTopology {
                command: from.len().min(to.len()),
            });
        }
        for (index, (source, destination)) in from.iter().zip(to).enumerate() {
            if interpolate_command(source, destination, 0.0).is_none() {
                return Err(PathMorphError::IncompatibleTopology { command: index });
            }
        }
        Ok(Self {
            from: from.to_vec(),
            to: to.to_vec(),
        })
    }

    pub fn sample(&self, progress: f32) -> Vec<PathCommand> {
        let progress = normalized(progress);
        self.from
            .iter()
            .zip(&self.to)
            .map(|(source, destination)| {
                interpolate_command(source, destination, progress).unwrap()
            })
            .collect()
    }
}

fn interpolate_command(
    source: &PathCommand,
    destination: &PathCommand,
    progress: f32,
) -> Option<PathCommand> {
    let point = |from, to| lerp_point(from, to, progress);
    Some(match (source, destination) {
        (PathCommand::MoveTo(from), PathCommand::MoveTo(to)) => {
            PathCommand::MoveTo(point(*from, *to))
        }
        (PathCommand::LineTo(from), PathCommand::LineTo(to)) => {
            PathCommand::LineTo(point(*from, *to))
        }
        (
            PathCommand::QuadTo {
                ctrl: from_ctrl,
                to: from,
            },
            PathCommand::QuadTo { ctrl: to_ctrl, to },
        ) => PathCommand::QuadTo {
            ctrl: point(*from_ctrl, *to_ctrl),
            to: point(*from, *to),
        },
        (
            PathCommand::CubicTo {
                c1: from_c1,
                c2: from_c2,
                to: from,
            },
            PathCommand::CubicTo {
                c1: to_c1,
                c2: to_c2,
                to,
            },
        ) => PathCommand::CubicTo {
            c1: point(*from_c1, *to_c1),
            c2: point(*from_c2, *to_c2),
            to: point(*from, *to),
        },
        (
            PathCommand::Arc {
                radii: from_radii,
                x_rot_deg: from_rotation,
                large,
                sweep,
                to: from,
            },
            PathCommand::Arc {
                radii: to_radii,
                x_rot_deg: to_rotation,
                large: to_large,
                sweep: to_sweep,
                to,
            },
        ) if large == to_large && sweep == to_sweep => PathCommand::Arc {
            radii: point(*from_radii, *to_radii),
            x_rot_deg: from_rotation + (to_rotation - from_rotation) * progress,
            large: *large,
            sweep: *sweep,
            to: point(*from, *to),
        },
        (PathCommand::Close, PathCommand::Close) => PathCommand::Close,
        _ => return None,
    })
}

fn bind_geometry(
    mut element: Element,
    progress: AnimatedNumber,
    geometry: impl Fn(f32) -> Vec<PathCommand> + 'static,
) -> Element {
    let apply = move |element: &mut Element| {
        if crate::style::prefers_reduced_motion()
            && (progress.is_active() || progress.peek() != 1.0)
        {
            progress.jump(1.0);
        }
        let data = encode_path_d(&geometry(progress.peek()));
        if element.attrs.get(ATTR_PATH_D) == Some(&data) {
            return LayoutStatus::Clean;
        }
        element.set_attr(ATTR_PATH_D, data);
        element.attrs.remove(ATTR_PATH_POINTS);
        element.attrs.remove(ATTR_PATH_KIND);
        LayoutStatus::StyleChanged
    };
    apply(&mut element);
    let previous = element.reactive_sync.take();
    element.reactive_sync(move |element, tree| {
        previous
            .as_ref()
            .map_or(LayoutStatus::Clean, |sync| sync.run(element, tree))
            .merge(apply(element))
    })
}

pub fn bind_path_reveal(
    element: Element,
    path: Rc<PreparedPath>,
    progress: AnimatedNumber,
) -> Element {
    bind_geometry(element, progress, move |progress| path.trim(0.0, progress))
}

pub fn bind_path_morph(
    element: Element,
    morph: Rc<PathMorph>,
    progress: AnimatedNumber,
) -> Element {
    bind_geometry(element, progress, move |progress| morph.sample(progress))
}

/// Compose a measured path offset onto an element's authored translation/rotation.
pub fn bind_path_follow(
    mut element: Element,
    path: Rc<PreparedPath>,
    progress: AnimatedNumber,
    rotate: bool,
) -> Element {
    let base_translate = element.style.translate;
    let base_rotate = element.style.rotate_deg;
    let apply = move |element: &mut Element| {
        if crate::style::prefers_reduced_motion()
            && (progress.is_active() || progress.peek() != 1.0)
        {
            progress.jump(1.0);
        }
        let sample = path.sample(progress.peek());
        let translate = [
            base_translate[0] + sample.position.0,
            base_translate[1] + sample.position.1,
        ];
        let rotation = base_rotate + if rotate { sample.tangent_degrees } else { 0.0 };
        if element.style.translate == translate && element.style.rotate_deg == rotation {
            return LayoutStatus::Clean;
        }
        element.style.translate = translate;
        element.style.rotate_deg = rotation;
        LayoutStatus::StyleChanged
    };
    apply(&mut element);
    let previous = element.reactive_sync.take();
    element.reactive_sync(move |element, tree| {
        previous
            .as_ref()
            .map_or(LayoutStatus::Clean, |sync| sync.run(element, tree))
            .merge(apply(element))
    })
}
