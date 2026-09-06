//! Minimal SVG `<filter>` / `fe*` markup parser (enough for Filter Effects L1 primitives).

use crate::layout::dimension::{parse_css_dim, CssDim};
use crate::style::color::try_parse_color;

use super::graph::{
    BlendMode, ChannelSelector, ColorMatrixType, CompositeOperator, EdgeMode, FilterGraph,
    FilterInput, FilterPrimitive, FilterUnits, LightSource, MorphologyOperator, PrimitiveRegion,
    PrimitiveUnits, TransferFunction, TransferType,
};

#[derive(Debug, Clone)]
pub enum FilterParseError {
    Message(String),
}

impl std::fmt::Display for FilterParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for FilterParseError {}

/// Parse all `<filter id="...">...</filter>` blocks from an SVG document or fragment.
pub fn parse_svg_filters_document(markup: &str) -> Result<Vec<FilterGraph>, FilterParseError> {
    let mut graphs = Vec::new();
    let lower = markup.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(rel) = lower[search_from..].find("<filter") {
        let start = search_from + rel;
        let after = &markup[start..];
        let open_end = after
            .find('>')
            .ok_or_else(|| FilterParseError::Message("unclosed <filter".into()))?;
        let open_tag = &after[..=open_end];
        let self_closing = open_tag.trim_end().ends_with("/>");
        let attrs = parse_attrs(open_tag);

        let (body, next) = if self_closing {
            ("", start + open_end + 1)
        } else {
            let close_rel = lower[start + open_end..]
                .find("</filter>")
                .ok_or_else(|| FilterParseError::Message("missing </filter>".into()))?;
            let body_start = start + open_end + 1;
            let body_end = start + open_end + close_rel;
            let next = body_end + "</filter>".len();
            (&markup[body_start..body_end], next)
        };

        let id = attrs.get("id").cloned().unwrap_or_default();
        let mut graph = FilterGraph {
            id,
            filter_units: match attrs.get("filterunits").map(|s| s.as_str()) {
                Some("userspaceonuse") => FilterUnits::UserSpaceOnUse,
                _ => FilterUnits::ObjectBoundingBox,
            },
            primitive_units: match attrs.get("primitiveunits").map(|s| s.as_str()) {
                Some("objectboundingbox") => PrimitiveUnits::ObjectBoundingBox,
                _ => PrimitiveUnits::UserSpaceOnUse,
            },
            x: attrs
                .get("x")
                .and_then(|s| parse_css_dim(s))
                .unwrap_or(CssDim::Percent(-0.10)),
            y: attrs
                .get("y")
                .and_then(|s| parse_css_dim(s))
                .unwrap_or(CssDim::Percent(-0.10)),
            width: attrs
                .get("width")
                .and_then(|s| parse_css_dim(s))
                .unwrap_or(CssDim::Percent(1.20)),
            height: attrs
                .get("height")
                .and_then(|s| parse_css_dim(s))
                .unwrap_or(CssDim::Percent(1.20)),
            primitives: Vec::new(),
        };

        parse_primitives(body, &mut graph.primitives)?;
        graphs.push(graph);
        search_from = next;
    }
    Ok(graphs)
}

fn parse_attrs(open_tag: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    // Strip tag name / trailing
    let s = open_tag
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/')
        .trim();
    let mut chars = s.chars().peekable();
    // skip element name
    while let Some(c) = chars.peek() {
        if c.is_whitespace() {
            break;
        }
        chars.next();
    }
    let rest: String = chars.collect();
    let mut i = 0;
    let bytes = rest.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let name = rest[name_start..i].trim().to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let quote = bytes[i];
        let value = if quote == b'"' || quote == b'\'' {
            i += 1;
            let vs = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let v = rest[vs..i].to_string();
            if i < bytes.len() {
                i += 1;
            }
            v
        } else {
            let vs = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            rest[vs..i].to_string()
        };
        if !name.is_empty() {
            map.insert(name, value);
        }
    }
    map
}

fn parse_primitives(body: &str, out: &mut Vec<FilterPrimitive>) -> Result<(), FilterParseError> {
    let lower = body.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(rel) = lower[search_from..].find('<') {
        let start = search_from + rel;
        if lower[start..].starts_with("</") {
            search_from = start + 2;
            continue;
        }
        let after = &body[start..];
        let name_end = after[1..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|i| i + 1)
            .unwrap_or(after.len());
        let raw_name = after[1..name_end].trim();
        let name = raw_name.to_ascii_lowercase();
        // strip svg: prefix
        let name = name.strip_prefix("svg:").unwrap_or(name.as_str());

        let open_end = after
            .find('>')
            .ok_or_else(|| FilterParseError::Message(format!("unclosed <{name}")))?;
        let open_tag = &after[..=open_end];
        let self_closing = open_tag.trim_end().ends_with("/>");
        let attrs = parse_attrs(open_tag);

        let (inner, next) = if self_closing {
            ("", start + open_end + 1)
        } else {
            let close_pat = format!("</{raw_name}>");
            let close_pat_l = close_pat.to_ascii_lowercase();
            let close_rel = lower[start + open_end..]
                .find(&close_pat_l)
                .ok_or_else(|| FilterParseError::Message(format!("missing {close_pat}")))?;
            let body_start = start + open_end + 1;
            let body_end = start + open_end + close_rel;
            (&body[body_start..body_end], body_end + close_pat.len())
        };

        if let Some(prim) = build_primitive(name, &attrs, inner)? {
            out.push(prim);
        }
        search_from = next;
    }
    Ok(())
}

fn attr_f32(attrs: &std::collections::HashMap<String, String>, key: &str, default: f32) -> f32 {
    attrs
        .get(key)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

fn attr_in(attrs: &std::collections::HashMap<String, String>, key: &str) -> FilterInput {
    attrs
        .get(key)
        .map(|s| FilterInput::parse(s))
        .unwrap_or(FilterInput::Result(String::new()))
}

fn attr_result(attrs: &std::collections::HashMap<String, String>) -> String {
    attrs.get("result").cloned().unwrap_or_default()
}

fn attr_region(attrs: &std::collections::HashMap<String, String>) -> PrimitiveRegion {
    PrimitiveRegion {
        x: attrs.get("x").and_then(|s| parse_css_dim(s)),
        y: attrs.get("y").and_then(|s| parse_css_dim(s)),
        width: attrs.get("width").and_then(|s| parse_css_dim(s)),
        height: attrs.get("height").and_then(|s| parse_css_dim(s)),
    }
}

fn parse_std_deviation(s: &str) -> (f32, f32) {
    let parts: Vec<f32> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|t| t.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [a, b, ..] => (*a, *b),
        [a] => (*a, *a),
        _ => (0.0, 0.0),
    }
}

fn parse_float_list(s: &str) -> Vec<f32> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|t| t.trim().parse().ok())
        .collect()
}

fn parse_transfer_fn(tag: &str, body: &str) -> Result<TransferFunction, FilterParseError> {
    // Find feFuncX inside body
    let lower = body.to_ascii_lowercase();
    let needle = format!("<{tag}");
    let Some(rel) = lower.find(&needle) else {
        return Ok(TransferFunction::default());
    };
    let after = &body[rel..];
    let open_end = after.find('>').unwrap_or(after.len().saturating_sub(1));
    let open_tag = &after[..=open_end.min(after.len().saturating_sub(1))];
    let attrs = parse_attrs(open_tag);
    let ty = match attrs.get("type").map(|s| s.as_str()) {
        Some("table") => TransferType::Table,
        Some("discrete") => TransferType::Discrete,
        Some("linear") => TransferType::Linear,
        Some("gamma") => TransferType::Gamma,
        _ => TransferType::Identity,
    };
    Ok(TransferFunction {
        ty,
        table_values: attrs
            .get("tablevalues")
            .map(|s| parse_float_list(s))
            .unwrap_or_default(),
        slope: attr_f32(&attrs, "slope", 1.0),
        intercept: attr_f32(&attrs, "intercept", 0.0),
        amplitude: attr_f32(&attrs, "amplitude", 1.0),
        exponent: attr_f32(&attrs, "exponent", 1.0),
        offset: attr_f32(&attrs, "offset", 0.0),
    })
}

fn parse_light(body: &str) -> LightSource {
    let lower = body.to_ascii_lowercase();
    if let Some(rel) = lower.find("<fedistantlight") {
        let after = &body[rel..];
        let end = after.find('>').unwrap_or(0);
        let attrs = parse_attrs(&after[..=end]);
        return LightSource::Distant {
            azimuth: attr_f32(&attrs, "azimuth", 0.0),
            elevation: attr_f32(&attrs, "elevation", 0.0),
        };
    }
    if let Some(rel) = lower.find("<fepointlight") {
        let after = &body[rel..];
        let end = after.find('>').unwrap_or(0);
        let attrs = parse_attrs(&after[..=end]);
        return LightSource::Point {
            x: attr_f32(&attrs, "x", 0.0),
            y: attr_f32(&attrs, "y", 0.0),
            z: attr_f32(&attrs, "z", 0.0),
        };
    }
    if let Some(rel) = lower.find("<fespotlight") {
        let after = &body[rel..];
        let end = after.find('>').unwrap_or(0);
        let attrs = parse_attrs(&after[..=end]);
        return LightSource::Spot {
            x: attr_f32(&attrs, "x", 0.0),
            y: attr_f32(&attrs, "y", 0.0),
            z: attr_f32(&attrs, "z", 0.0),
            points_at_x: attr_f32(&attrs, "pointsatx", 0.0),
            points_at_y: attr_f32(&attrs, "pointsaty", 0.0),
            points_at_z: attr_f32(&attrs, "pointsatz", 0.0),
            specular_exponent: attr_f32(&attrs, "specularexponent", 1.0),
            limiting_cone_angle: attrs.get("limitingconeangle").and_then(|s| s.parse().ok()),
        };
    }
    LightSource::Distant {
        azimuth: 0.0,
        elevation: 0.0,
    }
}

fn channel(s: &str) -> ChannelSelector {
    match s.trim().to_ascii_uppercase().as_str() {
        "G" => ChannelSelector::G,
        "B" => ChannelSelector::B,
        "A" => ChannelSelector::A,
        _ => ChannelSelector::R,
    }
}

fn build_primitive(
    name: &str,
    attrs: &std::collections::HashMap<String, String>,
    inner: &str,
) -> Result<Option<FilterPrimitive>, FilterParseError> {
    let region = attr_region(attrs);
    let result = attr_result(attrs);
    Ok(Some(match name {
        "fegaussianblur" => FilterPrimitive::GaussianBlur {
            in1: attr_in(attrs, "in"),
            std_deviation: attrs
                .get("stddeviation")
                .map(|s| parse_std_deviation(s))
                .unwrap_or((0.0, 0.0)),
            result,
            region,
        },
        "fecolormatrix" => {
            let ty = match attrs.get("type").map(|s| s.as_str()) {
                Some("saturate") => ColorMatrixType::Saturate,
                Some("huerotate") => ColorMatrixType::HueRotate,
                Some("luminancetoalpha") => ColorMatrixType::LuminanceToAlpha,
                _ => ColorMatrixType::Matrix,
            };
            FilterPrimitive::ColorMatrix {
                in1: attr_in(attrs, "in"),
                ty,
                values: attrs
                    .get("values")
                    .map(|s| parse_float_list(s))
                    .unwrap_or_default(),
                result,
                region,
            }
        }
        "feoffset" => FilterPrimitive::Offset {
            in1: attr_in(attrs, "in"),
            dx: attr_f32(attrs, "dx", 0.0),
            dy: attr_f32(attrs, "dy", 0.0),
            result,
            region,
        },
        "feflood" => {
            let color = attrs
                .get("flood-color")
                .or_else(|| attrs.get("floodcolor"))
                .and_then(|s| try_parse_color(s))
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let opacity = attrs
                .get("flood-opacity")
                .or_else(|| attrs.get("floodopacity"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            FilterPrimitive::Flood {
                color,
                opacity,
                result,
                region,
            }
        }
        "feblend" => FilterPrimitive::Blend {
            in1: attr_in(attrs, "in"),
            in2: attr_in(attrs, "in2"),
            mode: attrs
                .get("mode")
                .map(|s| BlendMode::parse(s))
                .unwrap_or(BlendMode::Normal),
            result,
            region,
        },
        "fecomposite" => FilterPrimitive::Composite {
            in1: attr_in(attrs, "in"),
            in2: attr_in(attrs, "in2"),
            operator: attrs
                .get("operator")
                .map(|s| CompositeOperator::parse(s))
                .unwrap_or(CompositeOperator::Over),
            k1: attr_f32(attrs, "k1", 0.0),
            k2: attr_f32(attrs, "k2", 0.0),
            k3: attr_f32(attrs, "k3", 0.0),
            k4: attr_f32(attrs, "k4", 0.0),
            result,
            region,
        },
        "femerge" => {
            let mut nodes = Vec::new();
            let l = inner.to_ascii_lowercase();
            let mut from = 0usize;
            while let Some(rel) = l[from..].find("<femergenode") {
                let s = from + rel;
                let after = &inner[s..];
                let end = after.find('>').unwrap_or(0);
                let a = parse_attrs(&after[..=end]);
                nodes.push(attr_in(&a, "in"));
                from = s + end + 1;
            }
            FilterPrimitive::Merge {
                nodes,
                result,
                region,
            }
        }
        "femorphology" => FilterPrimitive::Morphology {
            in1: attr_in(attrs, "in"),
            operator: match attrs.get("operator").map(|s| s.as_str()) {
                Some("erode") => MorphologyOperator::Erode,
                _ => MorphologyOperator::Dilate,
            },
            radius: attrs
                .get("radius")
                .map(|s| parse_std_deviation(s))
                .unwrap_or((0.0, 0.0)),
            result,
            region,
        },
        "feconvolvematrix" => {
            let order = attrs
                .get("order")
                .map(|s| parse_float_list(s))
                .unwrap_or_else(|| vec![3.0]);
            let order_x = order.first().copied().unwrap_or(3.0) as u32;
            let order_y = order.get(1).copied().unwrap_or(order_x as f32) as u32;
            FilterPrimitive::ConvolveMatrix {
                in1: attr_in(attrs, "in"),
                order_x,
                order_y,
                kernel: attrs
                    .get("kernelmatrix")
                    .map(|s| parse_float_list(s))
                    .unwrap_or_default(),
                divisor: attrs.get("divisor").and_then(|s| s.parse().ok()),
                bias: attr_f32(attrs, "bias", 0.0),
                target_x: attrs.get("targetx").and_then(|s| s.parse().ok()),
                target_y: attrs.get("targety").and_then(|s| s.parse().ok()),
                edge_mode: match attrs.get("edgemode").map(|s| s.as_str()) {
                    Some("wrap") => EdgeMode::Wrap,
                    Some("none") => EdgeMode::None,
                    _ => EdgeMode::Duplicate,
                },
                kernel_unit_length: attrs.get("kernelunitlength").map(|s| {
                    let p = parse_std_deviation(s);
                    p
                }),
                preserve_alpha: attrs
                    .get("preservealpha")
                    .map(|s| s == "true")
                    .unwrap_or(false),
                result,
                region,
            }
        }
        "fedisplacementmap" => FilterPrimitive::DisplacementMap {
            in1: attr_in(attrs, "in"),
            in2: attr_in(attrs, "in2"),
            scale: attr_f32(attrs, "scale", 0.0),
            x_channel: attrs
                .get("xchannelselector")
                .map(|s| channel(s))
                .unwrap_or(ChannelSelector::A),
            y_channel: attrs
                .get("ychannelselector")
                .map(|s| channel(s))
                .unwrap_or(ChannelSelector::A),
            result,
            region,
        },
        "feimage" => FilterPrimitive::Image {
            href: attrs
                .get("href")
                .or_else(|| attrs.get("xlink:href"))
                .cloned()
                .unwrap_or_default(),
            result,
            region,
        },
        "fetile" => FilterPrimitive::Tile {
            in1: attr_in(attrs, "in"),
            result,
            region,
        },
        "feturbulence" => FilterPrimitive::Turbulence {
            base_frequency: attrs
                .get("basefrequency")
                .map(|s| parse_std_deviation(s))
                .unwrap_or((0.0, 0.0)),
            num_octaves: attr_f32(attrs, "numoctaves", 1.0) as u32,
            seed: attr_f32(attrs, "seed", 0.0),
            stitch_tiles: attrs
                .get("stitchtiles")
                .map(|s| s == "stitch")
                .unwrap_or(false),
            fractal_noise: attrs
                .get("type")
                .map(|s| s == "fractalNoise")
                .unwrap_or(false),
            result,
            region,
        },
        "fecomponenttransfer" => FilterPrimitive::ComponentTransfer {
            in1: attr_in(attrs, "in"),
            func_r: parse_transfer_fn("fefuncr", inner)?,
            func_g: parse_transfer_fn("fefuncg", inner)?,
            func_b: parse_transfer_fn("fefuncb", inner)?,
            func_a: parse_transfer_fn("fefunca", inner)?,
            result,
            region,
        },
        "fediffuselighting" => FilterPrimitive::DiffuseLighting {
            in1: attr_in(attrs, "in"),
            surface_scale: attr_f32(attrs, "surfacescale", 1.0),
            diffuse_constant: attr_f32(attrs, "diffuseconstant", 1.0),
            kernel_unit_length: attrs
                .get("kernelunitlength")
                .map(|s| parse_std_deviation(s)),
            lighting_color: attrs
                .get("lighting-color")
                .or_else(|| attrs.get("lightingcolor"))
                .and_then(|s| try_parse_color(s))
                .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            light: parse_light(inner),
            result,
            region,
        },
        "fespecularlighting" => FilterPrimitive::SpecularLighting {
            in1: attr_in(attrs, "in"),
            surface_scale: attr_f32(attrs, "surfacescale", 1.0),
            specular_constant: attr_f32(attrs, "specularconstant", 1.0),
            specular_exponent: attr_f32(attrs, "specularexponent", 1.0),
            kernel_unit_length: attrs
                .get("kernelunitlength")
                .map(|s| parse_std_deviation(s)),
            lighting_color: attrs
                .get("lighting-color")
                .or_else(|| attrs.get("lightingcolor"))
                .and_then(|s| try_parse_color(s))
                .unwrap_or([1.0, 1.0, 1.0, 1.0]),
            light: parse_light(inner),
            result,
            region,
        },
        // skip descriptive / unknown
        "title" | "desc" | "metadata" | "femergenode" | "fefuncr" | "fefuncg" | "fefuncb"
        | "fefunca" | "fedistantlight" | "fepointlight" | "fespotlight" => return Ok(None),
        _ => return Ok(None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_blur_filter() {
        let markup = r##"
        <filter id="blurMe">
          <feGaussianBlur stdDeviation="5"/>
        </filter>
        "##;
        let graphs = parse_svg_filters_document(markup).unwrap();
        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].id, "blurMe");
        assert!(matches!(
            &graphs[0].primitives[0],
            FilterPrimitive::GaussianBlur { std_deviation: (a, b), .. }
                if (*a - 5.0).abs() < 1e-6 && (*b - 5.0).abs() < 1e-6
        ));
    }
}
