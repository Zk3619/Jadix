//! SVG Filter Effects: graph model, CSS lowering, markup parse, registry.

mod graph;
mod lower_css;
mod parse;
mod registry;

pub use graph::*;
pub use lower_css::{LowerSegment, LoweredFilter, lower_css_filters};
pub use parse::{FilterParseError, parse_svg_filters_document};
pub use registry::{
    lookup_svg_filter, register_svg_filter, register_svg_filter_graph, resolve_svg_filter_url,
};

use crate::style::filter::FilterList;

/// Resolve a full CSS `filter` list into a single concatenated [`FilterGraph`]
/// (url segments spliced from the registry / files).
pub fn resolve_filter_list_to_graph(list: &FilterList) -> Option<FilterGraph> {
    let lowered = lower_css_filters(list);
    if lowered.is_empty() {
        return None;
    }
    let mut out = FilterGraph {
        id: "__resolved_filter".into(),
        ..FilterGraph::default()
    };
    out.filter_units = FilterUnits::UserSpaceOnUse;
    out.primitive_units = PrimitiveUnits::UserSpaceOnUse;

    let mut prim_i = 0usize;
    for seg in lowered.segments {
        match seg {
            LowerSegment::Url(u) => {
                let Some(g) = resolve_svg_filter_url(&u) else {
                    continue;
                };
                for mut p in g.primitives {
                    // uniquify result names
                    let r = p.result_name().to_string();
                    if !r.is_empty() {
                        let nr = format!("u{prim_i}_{r}");
                        rename_result(&mut p, &nr);
                    }
                    out.primitives.push(p);
                    prim_i += 1;
                }
            }
            LowerSegment::Graph(g) => {
                for p in g.primitives {
                    out.primitives.push(p);
                    prim_i += 1;
                }
            }
        }
    }
    if out.primitives.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn rename_result(p: &mut FilterPrimitive, new_name: &str) {
    match p {
        FilterPrimitive::Blend { result, .. }
        | FilterPrimitive::ColorMatrix { result, .. }
        | FilterPrimitive::ComponentTransfer { result, .. }
        | FilterPrimitive::Composite { result, .. }
        | FilterPrimitive::ConvolveMatrix { result, .. }
        | FilterPrimitive::DiffuseLighting { result, .. }
        | FilterPrimitive::DisplacementMap { result, .. }
        | FilterPrimitive::Flood { result, .. }
        | FilterPrimitive::GaussianBlur { result, .. }
        | FilterPrimitive::Image { result, .. }
        | FilterPrimitive::Merge { result, .. }
        | FilterPrimitive::Morphology { result, .. }
        | FilterPrimitive::Offset { result, .. }
        | FilterPrimitive::SpecularLighting { result, .. }
        | FilterPrimitive::Tile { result, .. }
        | FilterPrimitive::Turbulence { result, .. } => {
            *result = new_name.to_string();
        }
    }
}
