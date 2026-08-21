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

/// Content-stable id: filter list + size + subtree paint fingerprint (relative geometry).
/// Excludes screen `x/y` so scrolling reuses the same capture / filter textures.
fn filter_cache_id(
    filter: &FilterList,
    bounds: [f32; 4],
    instances: &[InstanceInput],
    textured_draws: &[TexturedDraw],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    filter_list_to_css(Some(filter)).hash(&mut hasher);
    (bounds[2].round() as i32).hash(&mut hasher);
    (bounds[3].round() as i32).hash(&mut hasher);
    instances.len().hash(&mut hasher);
    for i in instances {
        ((i.rect_pos[0] - bounds[0]).to_bits()).hash(&mut hasher);
        ((i.rect_pos[1] - bounds[1]).to_bits()).hash(&mut hasher);
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
        ((d.rect[0] - bounds[0]).to_bits()).hash(&mut hasher);
        ((d.rect[1] - bounds[1]).to_bits()).hash(&mut hasher);
        d.rect[2].to_bits().hash(&mut hasher);
        d.rect[3].to_bits().hash(&mut hasher);
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
        textured_draws,
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
    fn cache_id_ignores_screen_origin() {
        let list = parse_filter("grayscale(1)").unwrap();
        let a = filter_cache_id(&list, [10.0, 20.0, 64.0, 32.0], &[], &[]);
        let b = filter_cache_id(&list, [100.0, 200.0, 64.0, 32.0], &[], &[]);
        assert_eq!(a, b, "scroll must not change filter cache id");
        let c = filter_cache_id(&list, [10.0, 20.0, 65.0, 32.0], &[], &[]);
        assert_ne!(a, c, "size change must bust cache");
    }
}
