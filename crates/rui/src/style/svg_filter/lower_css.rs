//! Lower CSS `filter` function lists into SVG [`FilterGraph`]s (Filter Effects L1).

use crate::layout::dimension::{CSS_ROOT_FONT_SIZE_PX, CssDim};
use crate::style::filter::{FilterFunction, FilterList};

use super::graph::{
    ColorMatrixType, CompositeOperator, FilterGraph, FilterInput, FilterPrimitive, FilterUnits,
    PrimitiveRegion, PrimitiveUnits,
};

fn dim_to_px(d: &CssDim) -> f32 {
    match d {
        CssDim::Px(v) => *v,
        CssDim::Rem(v) | CssDim::Em(v) => *v * CSS_ROOT_FONT_SIZE_PX,
        CssDim::Percent(v) => *v * CSS_ROOT_FONT_SIZE_PX, // rare for blur; treat as rem-ish
        _ => 0.0,
    }
}

fn factor(v: Option<f32>, default_when_omitted: f32) -> f32 {
    v.unwrap_or(default_when_omitted)
}

fn auto_result(i: usize) -> String {
    format!("__css_f{i}")
}

/// Expand a CSS filter list into an SVG filter graph.
///
/// `url()` entries are returned separately so the caller can splice registered graphs.
pub fn lower_css_filters(list: &FilterList) -> LoweredFilter {
    let mut segments: Vec<LowerSegment> = Vec::new();
    let mut current = FilterGraph {
        id: "__css_filter".into(),
        filter_units: FilterUnits::UserSpaceOnUse,
        primitive_units: PrimitiveUnits::UserSpaceOnUse,
        x: CssDim::Percent(-0.10),
        y: CssDim::Percent(-0.10),
        width: CssDim::Percent(1.20),
        height: CssDim::Percent(1.20),
        primitives: Vec::new(),
    };
    let mut idx = 0usize;
    let mut prev = FilterInput::SourceGraphic;

    let flush = |g: &mut FilterGraph, segs: &mut Vec<LowerSegment>| {
        if !g.primitives.is_empty() {
            segs.push(LowerSegment::Graph(std::mem::take(g)));
            *g = FilterGraph {
                id: "__css_filter".into(),
                filter_units: FilterUnits::UserSpaceOnUse,
                primitive_units: PrimitiveUnits::UserSpaceOnUse,
                x: CssDim::Percent(-0.10),
                y: CssDim::Percent(-0.10),
                width: CssDim::Percent(1.20),
                height: CssDim::Percent(1.20),
                primitives: Vec::new(),
            };
        }
    };

    for func in list {
        if let FilterFunction::Url(u) = func {
            flush(&mut current, &mut segments);
            segments.push(LowerSegment::Url(u.clone()));
            // Next CSS funcs continue from the url segment's output (spliced at resolve).
            prev = FilterInput::Result(String::new());
            continue;
        }

        let result = auto_result(idx);
        idx += 1;
        let region = PrimitiveRegion::default();

        match func {
            FilterFunction::Blur(d) => {
                let px = d.as_ref().map(dim_to_px).unwrap_or(0.0).max(0.0);
                current.primitives.push(FilterPrimitive::GaussianBlur {
                    in1: prev.clone(),
                    std_deviation: (px, px),
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Brightness(v) => {
                let a = factor(*v, 1.0);
                // Spec: matrix that scales RGB by a, A unchanged.
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values: vec![
                        a, 0.0, 0.0, 0.0, 0.0, //
                        0.0, a, 0.0, 0.0, 0.0, //
                        0.0, 0.0, a, 0.0, 0.0, //
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Contrast(v) => {
                let a = factor(*v, 1.0);
                let intercept = 0.5 - 0.5 * a; // (1 - a) / 2
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values: vec![
                        a, 0.0, 0.0, 0.0, intercept, //
                        0.0, a, 0.0, 0.0, intercept, //
                        0.0, 0.0, a, 0.0, intercept, //
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Grayscale(v) => {
                let amount = factor(*v, 1.0).clamp(0.0, 1.0);
                // Interpolate identity → luminance grayscale by `amount`.
                let inv = 1.0 - amount;
                let r = 0.2126;
                let g = 0.7152;
                let b = 0.0722;
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values: vec![
                        inv + amount * r,
                        amount * g,
                        amount * b,
                        0.0,
                        0.0,
                        amount * r,
                        inv + amount * g,
                        amount * b,
                        0.0,
                        0.0,
                        amount * r,
                        amount * g,
                        inv + amount * b,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        1.0,
                        0.0,
                    ],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::HueRotate(a) => {
                let deg = a.map(|x| x.to_deg()).unwrap_or(0.0);
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::HueRotate,
                    values: vec![deg],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Invert(v) => {
                let a = factor(*v, 1.0).clamp(0.0, 1.0);
                // lerp(identity, invert): value' = (1-2a)*value + a
                let s = 1.0 - 2.0 * a;
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values: vec![
                        s, 0.0, 0.0, 0.0, a, //
                        0.0, s, 0.0, 0.0, a, //
                        0.0, 0.0, s, 0.0, a, //
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Opacity(v) => {
                let a = factor(*v, 1.0).clamp(0.0, 1.0);
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values: vec![
                        1.0, 0.0, 0.0, 0.0, 0.0, //
                        0.0, 1.0, 0.0, 0.0, 0.0, //
                        0.0, 0.0, 1.0, 0.0, 0.0, //
                        0.0, 0.0, 0.0, a, 0.0,
                    ],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Saturate(v) => {
                let s = factor(*v, 1.0);
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Saturate,
                    values: vec![s],
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::Sepia(v) => {
                let amount = factor(*v, 1.0).clamp(0.0, 1.0);
                let inv = 1.0 - amount;
                // Sepia matrix from Filter Effects, lerped with identity.
                let m = [
                    0.393, 0.769, 0.189, 0.0, 0.0, //
                    0.349, 0.686, 0.168, 0.0, 0.0, //
                    0.272, 0.534, 0.131, 0.0, 0.0, //
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                let mut values = Vec::with_capacity(20);
                let ident = [
                    1.0, 0.0, 0.0, 0.0, 0.0, //
                    0.0, 1.0, 0.0, 0.0, 0.0, //
                    0.0, 0.0, 1.0, 0.0, 0.0, //
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ];
                for i in 0..20 {
                    values.push(ident[i] * inv + m[i] * amount);
                }
                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values,
                    result: result.clone(),
                    region,
                });
            }
            FilterFunction::DropShadow {
                color,
                offset_x,
                offset_y,
                blur,
            } => {
                // SourceAlpha → offset → blur → flood color → composite → merge with input
                let shadow_a = format!("{result}_sa");
                let shadow_off = format!("{result}_off");
                let shadow_blur = format!("{result}_blur");
                let shadow_flood = format!("{result}_flood");
                let shadow_comp = format!("{result}_comp");

                current.primitives.push(FilterPrimitive::ColorMatrix {
                    in1: prev.clone(),
                    ty: ColorMatrixType::Matrix,
                    values: vec![
                        0.0, 0.0, 0.0, 0.0, 0.0, //
                        0.0, 0.0, 0.0, 0.0, 0.0, //
                        0.0, 0.0, 0.0, 0.0, 0.0, //
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ],
                    result: shadow_a.clone(),
                    region: PrimitiveRegion::default(),
                });
                // Better: use SourceAlpha of prev — encode as luminance-to-alpha on alpha channel.
                // Replace last with proper SourceAlpha extraction via LuminanceToAlpha on alpha-only:
                if let Some(p) = current.primitives.last_mut() {
                    *p = FilterPrimitive::ColorMatrix {
                        in1: prev.clone(),
                        ty: ColorMatrixType::LuminanceToAlpha,
                        values: Vec::new(),
                        result: shadow_a.clone(),
                        region: PrimitiveRegion::default(),
                    };
                    // LuminanceToAlpha uses RGB; for drop-shadow we need SourceAlpha.
                    // Use matrix that keeps alpha as RGB+A:
                    *p = FilterPrimitive::ColorMatrix {
                        in1: prev.clone(),
                        ty: ColorMatrixType::Matrix,
                        values: vec![
                            0.0, 0.0, 0.0, 1.0, 0.0, //
                            0.0, 0.0, 0.0, 1.0, 0.0, //
                            0.0, 0.0, 0.0, 1.0, 0.0, //
                            0.0, 0.0, 0.0, 1.0, 0.0,
                        ],
                        result: shadow_a.clone(),
                        region: PrimitiveRegion::default(),
                    };
                }

                let dx = dim_to_px(offset_x);
                let dy = dim_to_px(offset_y);
                current.primitives.push(FilterPrimitive::Offset {
                    in1: FilterInput::Result(shadow_a),
                    dx,
                    dy,
                    result: shadow_off.clone(),
                    region: PrimitiveRegion::default(),
                });

                let blur_px = blur.as_ref().map(dim_to_px).unwrap_or(0.0).max(0.0);
                // CSS blur radius ≈ stdDeviation
                current.primitives.push(FilterPrimitive::GaussianBlur {
                    in1: FilterInput::Result(shadow_off),
                    std_deviation: (blur_px, blur_px),
                    result: shadow_blur.clone(),
                    region: PrimitiveRegion::default(),
                });

                let c = color.unwrap_or([0.0, 0.0, 0.0, 1.0]);
                current.primitives.push(FilterPrimitive::Flood {
                    color: [c[0], c[1], c[2], 1.0],
                    opacity: c[3],
                    result: shadow_flood.clone(),
                    region: PrimitiveRegion::default(),
                });
                current.primitives.push(FilterPrimitive::Composite {
                    in1: FilterInput::Result(shadow_flood),
                    in2: FilterInput::Result(shadow_blur),
                    operator: CompositeOperator::In,
                    k1: 0.0,
                    k2: 0.0,
                    k3: 0.0,
                    k4: 0.0,
                    result: shadow_comp.clone(),
                    region: PrimitiveRegion::default(),
                });
                current.primitives.push(FilterPrimitive::Merge {
                    nodes: vec![FilterInput::Result(shadow_comp), prev.clone()],
                    result: result.clone(),
                    region: PrimitiveRegion::default(),
                });
            }
            FilterFunction::Url(_) => unreachable!(),
        }

        prev = FilterInput::Result(result);
    }

    flush(&mut current, &mut segments);
    LoweredFilter { segments }
}

#[derive(Clone, Debug)]
pub enum LowerSegment {
    Graph(FilterGraph),
    Url(String),
}

#[derive(Clone, Debug, Default)]
pub struct LoweredFilter {
    pub segments: Vec<LowerSegment>,
}

impl LoweredFilter {
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::parse_filter;

    #[test]
    fn lower_blur_grayscale() {
        let list = parse_filter("blur(5px) grayscale(0.4)").unwrap();
        let lowered = lower_css_filters(&list);
        assert_eq!(lowered.segments.len(), 1);
        if let LowerSegment::Graph(g) = &lowered.segments[0] {
            assert_eq!(g.primitives.len(), 2);
            assert!(matches!(
                &g.primitives[0],
                FilterPrimitive::GaussianBlur { .. }
            ));
            assert!(matches!(
                &g.primitives[1],
                FilterPrimitive::ColorMatrix { .. }
            ));
        } else {
            panic!("expected graph");
        }
    }

    #[test]
    fn lower_url_splits() {
        let list = parse_filter("url(#x) brightness(1.2)").unwrap();
        let lowered = lower_css_filters(&list);
        assert!(matches!(&lowered.segments[0], LowerSegment::Url(u) if u == "#x"));
        assert!(matches!(&lowered.segments[1], LowerSegment::Graph(_)));
    }
}
