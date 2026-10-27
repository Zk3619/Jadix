pub use jadix_animation::icon_morph::{
    alloc_outputs, build_plan, canonical_d, cubics_to_path_d, fit_icon, interp_linear,
    interp_plan_d_kind, interp_polar, markup_to_icon, morph_d_at, morph_d_at_kind, resample_icon,
    resample_path, sampled_from_outputs, serialize, svg_to_icon, CubicPath, IconInput, IconNode,
    MorphError, MorphPlan, PlanItem, Sampled, SAMPLE_N,
};

/// Resolve a Lucide name, raw path data, or SVG markup into morph input.
pub fn icon_input_from_src(src: &str) -> Option<IconInput> {
    let source = src.trim();
    if source.is_empty() {
        return None;
    }
    if source.starts_with('<') {
        return markup_to_icon(source, true).ok();
    }
    if looks_like_path_d(source) {
        return Some(IconInput::D(source.to_owned()));
    }
    crate::utils::icon_provider::resolve_icon_svg(source)
        .and_then(|inner| markup_to_icon(&inner, true).ok())
}

pub fn morph_src_at(from: &str, to: &str, progress: f64) -> Option<String> {
    let from = icon_input_from_src(from)?;
    let to = icon_input_from_src(to)?;
    morph_d_at(&from, &to, progress).ok()
}

fn looks_like_path_d(source: &str) -> bool {
    let source = source.trim_start();
    let bytes = source.as_bytes();
    if bytes.is_empty() || (bytes[0] != b'M' && bytes[0] != b'm') {
        return false;
    }
    source[1..]
        .trim_start()
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_raw_path_data_without_an_icon_provider() {
        let input = icon_input_from_src("M4 6h16").expect("path data");
        assert!(matches!(input, IconInput::D(_)));
        assert!(icon_input_from_src("jadix-test-nonexistent-icon-3c8b").is_none());
    }
}
