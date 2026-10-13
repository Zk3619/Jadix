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
    let cap_key = format!("rui_filter_cap:{id:016x}");
    let out_key = format!("rui_filter:{id:016x}");
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
mod tests {
    use super::*;
    use crate::renderer::effect::{take_capture_jobs, take_effect_jobs};
    use crate::style::parse_filter;

    #[test]
    fn queue_css_filter_wrap_emits_capture_and_filter() {
        let _ = take_filter_jobs();
        let _ = take_capture_jobs();
        let _ = take_effect_jobs();
        let list = parse_filter("blur(4px)").unwrap();
        let out = queue_css_filter_wrap(&list, [10.0, 20.0, 64.0, 32.0], Vec::new(), Vec::new());
        assert!(out.unwrap().starts_with("rui_filter:"));
        let caps = take_capture_jobs();
        assert_eq!(caps.len(), 1);
        assert!(caps[0].key.starts_with("rui_filter_cap:"));
        let jobs = take_filter_jobs();
        assert_eq!(jobs.len(), 1);
        assert!(!jobs[0].graph.primitives.is_empty());
    }

    #[test]
    #[cfg(feature = "compute")]
    fn css_blur_enriches_compute_pass() {
        let _ = take_filter_jobs();
        let _ = take_capture_jobs();
        let list = parse_filter("blur(4px)").unwrap();
        queue_css_filter_wrap(&list, [0.0, 0.0, 32.0, 16.0], Vec::new(), Vec::new());
        let mut graph = crate::renderer::RenderGraph::default();
        enrich_filter_graph(&mut graph);
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.kind == crate::renderer::PassKind::Compute),
            "CSS blur must dump PassKind::Compute"
        );
        let _ = take_filter_jobs();
        let _ = take_capture_jobs();
    }

    #[test]
    fn cache_id_ignores_screen_origin() {
        let list = parse_filter("grayscale(1)").unwrap();
        let a = filter_cache_id(&list, [10.0, 20.0, 64.0, 32.0], &[], &[]);
        let b = filter_cache_id(&list, [100.0, 200.0, 64.0, 32.0], &[], &[]);
        assert_eq!(a, b, "scroll must not change filter cache id");
        let c = filter_cache_id(&list, [10.0, 20.0, 65.0, 32.0], &[], &[]);
        assert_ne!(a, c, "size change must bust cache");
    }

    #[test]
    fn cache_id_survives_fractional_scroll_with_painted_content() {
        let list = parse_filter("blur(4px)").unwrap();
        let mut instance = <InstanceInput as bytemuck::Zeroable>::zeroed();
        instance.rect_pos = [17.2, 32.2];
        instance.rect_size = [48.0, 16.0];
        let draw =
            TexturedDraw::clipped([17.2, 32.2, 48.0, 16.0], None, "stable-svg", 1.0).unwrap();
        let bounds = [10.0, 20.0, 64.0, 32.0];
        let original = filter_cache_id(&list, bounds, &[instance], &[draw.clone()]);
        for scroll in [0.1, 240.3, 4096.7, 12000.3] {
            let mut shifted = instance;
            shifted.rect_pos[1] -= scroll;
            let mut image = draw.clone();
            image.rect[1] -= scroll;
            let mut region = bounds;
            region[1] -= scroll;
            assert_eq!(
                original,
                filter_cache_id(&list, region, &[shifted], &[image]),
                "unchanged subtree must reuse filter pixels after scroll={scroll}"
            );
        }
        instance.rect_pos[1] += 0.25;
        assert_ne!(
            original,
            filter_cache_id(&list, bounds, &[instance], &[draw])
        );
    }
}
