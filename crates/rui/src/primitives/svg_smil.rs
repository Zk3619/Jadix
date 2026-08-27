//! SMIL animation evaluation for public `<svg>` (baked into emitted SVG before resvg).

use super::svg::SvgNode;
use crate::style::try_parse_color;

/// Wall-clock seconds for SMIL `repeatCount="indefinite"` loops.
pub fn smil_time_secs() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f32()
}

fn parse_dur_secs(raw: &str) -> f32 {
    let s = raw.trim();
    if s.is_empty() {
        return 1.0;
    }
    if let Some(ms) = s.strip_suffix("ms") {
        return ms.trim().parse::<f32>().unwrap_or(1000.0) / 1000.0;
    }
    if let Some(sec) = s.strip_suffix('s') {
        return sec.trim().parse::<f32>().unwrap_or(1.0);
    }
    s.parse::<f32>().unwrap_or(1.0)
}

fn smil_progress(dur: &str, repeat: &str, t: f32) -> f32 {
    let dur_s = parse_dur_secs(dur).max(1e-3);
    let local = if repeat.eq_ignore_ascii_case("indefinite") {
        t % dur_s
    } else if let Ok(n) = repeat.parse::<f32>() {
        if n <= 0.0 {
            0.0
        } else {
            (t % (dur_s * n)).min(dur_s * n)
        }
    } else {
        t.min(dur_s)
    };
    (local / dur_s).clamp(0.0, 1.0)
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp_f32(a[0], b[0], t),
        lerp_f32(a[1], b[1], t),
        lerp_f32(a[2], b[2], t),
        lerp_f32(a[3], b[3], t),
    ]
}

fn color_to_css(c: [f32; 4]) -> String {
    format!(
        "rgba({},{},{},{})",
        (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        c[3].clamp(0.0, 1.0)
    )
}

/// Interpolate SMIL `from`/`to` attribute values (numbers, colors, or snap).
fn lerp_attr(from: &str, to: &str, p: f32) -> String {
    let from = from.trim();
    let to = to.trim();
    if let (Ok(a), Ok(b)) = (from.parse::<f32>(), to.parse::<f32>()) {
        return format!("{}", lerp_f32(a, b, p));
    }
    // Multi-number lists (e.g. transform leftovers): lerp pairwise when lengths match.
    let from_nums: Vec<f32> = from
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    let to_nums: Vec<f32> = to
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    if !from_nums.is_empty() && from_nums.len() == to_nums.len() {
        return from_nums
            .iter()
            .zip(to_nums.iter())
            .map(|(a, b)| format!("{}", lerp_f32(*a, *b, p)))
            .collect::<Vec<_>>()
            .join(" ");
    }
    if let (Some(a), Some(b)) = (try_parse_color(from), try_parse_color(to)) {
        return color_to_css(lerp_color(a, b, p));
    }
    if p >= 0.5 {
        to.to_string()
    } else {
        from.to_string()
    }
}

fn apply_animate_transform(parent_transform: &str, node: &SvgNode, t: f32) -> String {
    let SvgNode::AnimateTransform {
        transform_type,
        from,
        to,
        dur,
        repeat_count,
        ..
    } = node
    else {
        return parent_transform.to_string();
    };
    let p = smil_progress(dur, repeat_count, t);
    let kind = transform_type.to_ascii_lowercase();
    let from_parts: Vec<f32> = from
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    let to_parts: Vec<f32> = to
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    let seg = match kind.as_str() {
        "rotate" if !from_parts.is_empty() && !to_parts.is_empty() => {
            let angle = lerp_f32(from_parts[0], to_parts[0], p);
            if from_parts.len() >= 3 {
                format!("rotate({} {} {})", angle, from_parts[1], from_parts[2])
            } else {
                format!("rotate({angle})")
            }
        }
        "translate" if from_parts.len() >= 2 && to_parts.len() >= 2 => {
            format!(
                "translate({} {})",
                lerp_f32(from_parts[0], to_parts[0], p),
                lerp_f32(from_parts[1], to_parts[1], p)
            )
        }
        "scale" if !from_parts.is_empty() && !to_parts.is_empty() => {
            let sx = lerp_f32(from_parts[0], to_parts[0], p);
            let sy = if from_parts.len() >= 2 && to_parts.len() >= 2 {
                lerp_f32(from_parts[1], to_parts[1], p)
            } else {
                sx
            };
            format!("scale({sx} {sy})")
        }
        _ => return parent_transform.to_string(),
    };
    if parent_transform.trim().is_empty() {
        seg
    } else {
        format!("{parent_transform} {seg}")
    }
}

fn is_group_level_attr(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "opacity" | "transform"
    )
}

fn attr_value_at(node: &SvgNode, t: f32) -> Option<(String, String)> {
    match node {
        SvgNode::Animate {
            attribute_name,
            from,
            to,
            dur,
            repeat_count,
            ..
        } => {
            let p = smil_progress(dur, repeat_count, t);
            Some((attribute_name.clone(), lerp_attr(from, to, p)))
        }
        SvgNode::Set {
            attribute_name, to, ..
        } => Some((attribute_name.clone(), to.clone())),
        _ => None,
    }
}

fn apply_group_attr(xf: &mut String, opacity: &mut f32, node: &SvgNode, t: f32) {
    let Some((name, value)) = attr_value_at(node, t) else {
        return;
    };
    match name.trim().to_ascii_lowercase().as_str() {
        "opacity" => {
            if let Ok(v) = value.parse::<f32>() {
                *opacity = v.clamp(0.0, 1.0);
            }
        }
        "transform" => {
            *xf = value;
        }
        _ => {}
    }
}

fn set_shape_transform(mut node: SvgNode, transform: String) -> SvgNode {
    match &mut node {
        SvgNode::Circle { transform: slot, .. }
        | SvgNode::Ellipse { transform: slot, .. }
        | SvgNode::Path { transform: slot, .. }
        | SvgNode::Rect { transform: slot, .. }
        | SvgNode::Line { transform: slot, .. }
        | SvgNode::Polyline { transform: slot, .. }
        | SvgNode::Use { transform: slot, .. }
        | SvgNode::Group { transform: slot, .. } => {
            *slot = transform;
        }
        _ => {}
    }
    node
}

fn apply_attr_to_shape(mut node: SvgNode, attr: &str, value: &str) -> SvgNode {
    let key = attr.trim().to_ascii_lowercase().replace('_', "-");
    match &mut node {
        SvgNode::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "cx" => {
                if let Ok(v) = value.parse::<f32>() {
                    *cx = v;
                }
            }
            "cy" => {
                if let Ok(v) = value.parse::<f32>() {
                    *cy = v;
                }
            }
            "r" => {
                if let Ok(v) = value.parse::<f32>() {
                    *r = v.max(0.0);
                }
            }
            "fill" => *fill = Some(value.to_string()),
            "stroke" => *stroke = Some(value.to_string()),
            "stroke-width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *stroke_width = v;
                }
            }
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        SvgNode::Ellipse {
            cx,
            cy,
            rx,
            ry,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "cx" => {
                if let Ok(v) = value.parse::<f32>() {
                    *cx = v;
                }
            }
            "cy" => {
                if let Ok(v) = value.parse::<f32>() {
                    *cy = v;
                }
            }
            "rx" => {
                if let Ok(v) = value.parse::<f32>() {
                    *rx = v.max(0.0);
                }
            }
            "ry" => {
                if let Ok(v) = value.parse::<f32>() {
                    *ry = v.max(0.0);
                }
            }
            "fill" => *fill = Some(value.to_string()),
            "stroke" => *stroke = Some(value.to_string()),
            "stroke-width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *stroke_width = v;
                }
            }
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        SvgNode::Path {
            d,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "d" => *d = value.to_string(),
            "fill" => *fill = Some(value.to_string()),
            "stroke" => *stroke = Some(value.to_string()),
            "stroke-width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *stroke_width = v;
                }
            }
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        SvgNode::Rect {
            x,
            y,
            w,
            h,
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "x" => {
                if let Ok(v) = value.parse::<f32>() {
                    *x = v;
                }
            }
            "y" => {
                if let Ok(v) = value.parse::<f32>() {
                    *y = v;
                }
            }
            "width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *w = v;
                }
            }
            "height" => {
                if let Ok(v) = value.parse::<f32>() {
                    *h = v;
                }
            }
            "fill" => *fill = Some(value.to_string()),
            "stroke" => *stroke = Some(value.to_string()),
            "stroke-width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *stroke_width = v;
                }
            }
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        SvgNode::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "x1" => {
                if let Ok(v) = value.parse::<f32>() {
                    *x1 = v;
                }
            }
            "y1" => {
                if let Ok(v) = value.parse::<f32>() {
                    *y1 = v;
                }
            }
            "x2" => {
                if let Ok(v) = value.parse::<f32>() {
                    *x2 = v;
                }
            }
            "y2" => {
                if let Ok(v) = value.parse::<f32>() {
                    *y2 = v;
                }
            }
            "stroke" => *stroke = Some(value.to_string()),
            "stroke-width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *stroke_width = v;
                }
            }
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        SvgNode::Polyline {
            fill,
            stroke,
            stroke_width,
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "fill" => *fill = Some(value.to_string()),
            "stroke" => *stroke = Some(value.to_string()),
            "stroke-width" => {
                if let Ok(v) = value.parse::<f32>() {
                    *stroke_width = v;
                }
            }
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        SvgNode::Group {
            opacity,
            transform,
            ..
        } => match key.as_str() {
            "opacity" => {
                if let Ok(v) = value.parse::<f32>() {
                    *opacity = v.clamp(0.0, 1.0);
                }
            }
            "transform" => *transform = value.to_string(),
            _ => {}
        },
        _ => {}
    }
    node
}

fn apply_attr_smil(shape: SvgNode, smil: &SvgNode, t: f32) -> SvgNode {
    match smil {
        SvgNode::AnimateTransform { .. } => {
            let xf = match &shape {
                SvgNode::Circle { transform, .. }
                | SvgNode::Ellipse { transform, .. }
                | SvgNode::Path { transform, .. }
                | SvgNode::Rect { transform, .. }
                | SvgNode::Line { transform, .. }
                | SvgNode::Polyline { transform, .. }
                | SvgNode::Use { transform, .. }
                | SvgNode::Group { transform, .. } => transform.as_str(),
                _ => "",
            };
            let baked = apply_animate_transform(xf, smil, t);
            set_shape_transform(shape, baked)
        }
        SvgNode::Animate { .. } | SvgNode::Set { .. } => {
            if let Some((name, value)) = attr_value_at(smil, t) {
                apply_attr_to_shape(shape, &name, &value)
            } else {
                shape
            }
        }
        _ => shape,
    }
}

pub(crate) fn node_has_smil(node: &SvgNode) -> bool {
    match node {
        SvgNode::Animate { .. } | SvgNode::AnimateTransform { .. } | SvgNode::Set { .. } => true,
        SvgNode::Elem { tag, children, .. } => {
            matches!(
                tag.as_str(),
                "animate" | "animateTransform" | "animateMotion" | "set" | "mpath"
            ) || children.iter().any(node_has_smil)
        }
        SvgNode::Group { children, .. }
        | SvgNode::Defs { children }
        | SvgNode::ClipPath { children, .. }
        | SvgNode::Symbol { children, .. } => children.iter().any(node_has_smil),
        _ => false,
    }
}

pub(crate) fn scene_has_smil(nodes: &[SvgNode]) -> bool {
    nodes.iter().any(node_has_smil)
}

fn apply_smil_node(node: &SvgNode, t: f32) -> SvgNode {
    match node {
        SvgNode::Group {
            transform,
            opacity,
            children,
        } => {
            let mut xf = transform.clone();
            let mut op = *opacity;
            let mut out_children = Vec::new();
            let mut i = 0;
            while i < children.len() {
                match &children[i] {
                    SvgNode::AnimateTransform { .. } => {
                        xf = apply_animate_transform(&xf, &children[i], t);
                        i += 1;
                    }
                    SvgNode::Animate {
                        attribute_name, ..
                    }
                    | SvgNode::Set {
                        attribute_name, ..
                    } if is_group_level_attr(attribute_name) => {
                        apply_group_attr(&mut xf, &mut op, &children[i], t);
                        i += 1;
                    }
                    smil if matches!(
                        smil,
                        SvgNode::Animate { .. } | SvgNode::Set { .. } | SvgNode::AnimateTransform { .. }
                    ) =>
                    {
                        // Shape-targeted SMIL with no preceding shape in this group — drop.
                        i += 1;
                    }
                    other => {
                        let mut shape = apply_smil_node(other, t);
                        i += 1;
                        while i < children.len()
                            && matches!(
                                &children[i],
                                SvgNode::Animate { .. }
                                    | SvgNode::Set { .. }
                                    | SvgNode::AnimateTransform { .. }
                            )
                        {
                            // Prefer applying shape-level attrs; group-level attrs after a
                            // shape still attach to that shape (SVG parent semantics for wrap).
                            shape = apply_attr_smil(shape, &children[i], t);
                            i += 1;
                        }
                        out_children.push(shape);
                    }
                }
            }
            SvgNode::Group {
                transform: xf,
                opacity: op,
                children: out_children,
            }
        }
        SvgNode::Defs { children } => SvgNode::Defs {
            children: children.iter().map(|c| apply_smil_node(c, t)).collect(),
        },
        SvgNode::ClipPath {
            id,
            clip_rule,
            children,
        } => SvgNode::ClipPath {
            id: id.clone(),
            clip_rule: clip_rule.clone(),
            children: children.iter().map(|c| apply_smil_node(c, t)).collect(),
        },
        SvgNode::Symbol {
            id,
            view_box,
            children,
        } => SvgNode::Symbol {
            id: id.clone(),
            view_box: view_box.clone(),
            children: children.iter().map(|c| apply_smil_node(c, t)).collect(),
        },
        SvgNode::Elem {
            tag,
            attrs,
            children,
            text,
        } => SvgNode::Elem {
            tag: tag.clone(),
            attrs: attrs.clone(),
            children: children.iter().map(|c| apply_smil_node(c, t)).collect(),
            text: text.clone(),
        },
        other => other.clone(),
    }
}

/// Apply SMIL animation specs at time `t` (seconds).
pub(crate) fn apply_smil(nodes: &[SvgNode], t: f32) -> Vec<SvgNode> {
    nodes.iter().map(|n| apply_smil_node(n, t)).collect()
}
