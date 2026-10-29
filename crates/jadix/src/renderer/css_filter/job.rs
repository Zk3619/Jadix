//! Filter job queue + layout helper.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::renderer::effect::{queue_capture, CaptureJob};
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::svg_filter::FilterGraph;
use crate::style::{filter_list_to_css, FilterList};

#[derive(Clone, Debug)]
pub struct FilterJob {
    pub graph: FilterGraph,
    pub input_key: String,
    pub output_key: String,
    pub bounds: [f32; 4],
}

thread_local! {
    static FILTER_JOBS: RefCell<Vec<FilterJob>> = const { RefCell::new(Vec::new()) };
}

pub fn queue_filter_job(job: FilterJob) {
    FILTER_JOBS.with(|j| j.borrow_mut().push(job));
}

pub fn take_filter_jobs() -> Vec<FilterJob> {
    FILTER_JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()))
}

pub fn enrich_filter_graph(graph: &mut crate::renderer::RenderGraph) {
    FILTER_JOBS.with(|j| {
        for job in j.borrow().iter() {
            graph.push(
                crate::renderer::RenderPassNode::new(
                    crate::renderer::PassKind::Effect,
                    format!("filter-{}", job.output_key),
                )
                .read(job.input_key.clone())
                .write(job.output_key.clone()),
            );
            let has_blur = job.graph.primitives.iter().any(|p| {
                matches!(
                    p,
                    crate::style::svg_filter::FilterPrimitive::GaussianBlur { .. }
                )
            });
            if has_blur && cfg!(feature = "compute") {
                graph.push(
                    crate::renderer::RenderPassNode::new(
                        crate::renderer::PassKind::Compute,
                        format!("blur-cs-{}", job.output_key),
                    )
                    .read(job.input_key.clone())
                    .write(job.output_key.clone()),
                );
            }
        }
    });
}

/// Content-stable id: filter list + size + subtree paint fingerprint (relative geometry).
/// Excludes screen `x/y` so scrolling reuses the same capture / filter textures.
fn filter_cache_id(
    filter: &FilterList,
    bounds: [f32; 4],
    instances: &[InstanceInput],
    textured_draws: &[TexturedDraw],
) -> u64 {
    // World-coordinate subtraction loses low bits on long pages. Quantize only
    // relative positions, below a device pixel, so translation keeps the key.
    let position_key = |value: f32| (value * 64.0).round() as i64;
    let mut hasher = DefaultHasher::new();
    filter_list_to_css(Some(filter)).hash(&mut hasher);
    (bounds[2].round() as i32).hash(&mut hasher);
    (bounds[3].round() as i32).hash(&mut hasher);
    instances.len().hash(&mut hasher);
    for i in instances {
        position_key(i.rect_pos[0] - bounds[0]).hash(&mut hasher);
        position_key(i.rect_pos[1] - bounds[1]).hash(&mut hasher);
        i.rect_size[0].to_bits().hash(&mut hasher);
        i.rect_size[1].to_bits().hash(&mut hasher);
        for c in i.color {
            c.to_bits().hash(&mut hasher);
        }
        for c in i.border_color {
            c.to_bits().hash(&mut hasher);
        }
        i.border_width.to_bits().hash(&mut hasher);
        for r in i.radii {
            r.to_bits().hash(&mut hasher);
        }
    }
    textured_draws.len().hash(&mut hasher);
    for d in textured_draws {
        d.texture_key.hash(&mut hasher);
        position_key(d.rect[0] - bounds[0]).hash(&mut hasher);
        position_key(d.rect[1] - bounds[1]).hash(&mut hasher);
        d.rect[2].to_bits().hash(&mut hasher);
        d.rect[3].to_bits().hash(&mut hasher);
        for (i, layer) in d.rounded_clips.iter().enumerate() {
            if d.rounded_clips[i & !1][2] <= 0.0 || d.rounded_clips[i & !1][3] <= 0.0 {
                0u8.hash(&mut hasher);
                continue;
            }
            1u8.hash(&mut hasher);
            for (axis, value) in layer.iter().enumerate() {
                let relative = if i % 2 == 0 && axis < 2 {
                    value - bounds[axis]
                } else {
                    *value
                };
                if i % 2 == 0 && axis < 2 {
                    position_key(relative).hash(&mut hasher);
                } else {
                    relative.to_bits().hash(&mut hasher);
                }
            }
        }
        d.opacity.to_bits().hash(&mut hasher);
        for value in d
            .uv_offset
            .into_iter()
            .chain(d.uv_scale)
            .chain(d.tint_rgb)
            .chain(d.transform)
        {
            value.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Capture subtree then apply CSS `filter` graph. Returns textured output key.
pub fn queue_css_filter_wrap(
    filter: &FilterList,
    bounds: [f32; 4],
    instances: Vec<InstanceInput>,
    textured_draws: Vec<TexturedDraw>,
) -> Option<String> {
    let graph = crate::style::resolve_filter_list_to_graph(filter)?;
    if graph.is_empty() {
        return None;
    }
    let id = filter_cache_id(filter, bounds, &instances, &textured_draws);
    // SourceGraphic is independent of the graph applied afterward. Image-only
    // captures have a complete textured fingerprint; keep SDF captures on their
    // existing per-filter key until their full paint state is fingerprinted.
    let cap_id = if instances.is_empty() && !textured_draws.is_empty() {
        filter_cache_id(&FilterList::new(), bounds, &instances, &textured_draws)
    } else {
        id
    };
    let cap_key = format!("jadix_filter_cap:{cap_id:016x}");
    let out_key = format!("jadix_filter:{id:016x}");
    queue_capture(CaptureJob {
        key: cap_key.clone(),
        bounds,
        instances: instances
            .into_iter()
            .map(|mut i| {
                // Capture RT is already the filter region — disable page-space overflow clips.
                i.clip_pos = [0.0, 0.0];
                i.clip_size = InstanceInput::NO_CLIP_SIZE;
                i
            })
            .collect(),
        textured_draws: textured_draws
            .into_iter()
            .map(|mut d| {
                d.clip_pos = [0.0, 0.0];
                d.clip_size = TexturedDraw::NO_CLIP_SIZE;
                d
            })
            .collect(),
        text_nodes: Vec::new(),
    });
    queue_filter_job(FilterJob {
        graph,
        input_key: cap_key,
        output_key: out_key.clone(),
        bounds,
    });
    Some(out_key)
}

#[cfg(test)]
#[path = "tests/job.rs"]
mod tests;
