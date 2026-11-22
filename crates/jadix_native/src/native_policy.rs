//! Bounded policy bridge for native adapters which cannot call Rust types directly.
//! Platform code owns views; parsing, Unicode and interaction decisions stay shared.
use serde::Deserialize;
use serde_json::{json, Value};
use crate::presentation::{interaction::{PanActivation, PanAxis, PanDecision}, viewport::{CullNode, materialization_plan}};
use unicode_segmentation::UnicodeSegmentation;
#[cfg(feature = "android")]
pub(crate) mod terminal;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Html { html: String },
    #[cfg(feature = "android")]
    CompileShader { specification: jadix_core::hosted::HostedContent },
    ImageDimensions { width: Option<f64>, height: Option<f64>, natural: Option<[f64; 2]> },
    Pan { axis: PanAxis, activation: PanActivation, delta: [f64; 2] },
    Viewport { nodes: Vec<CullNode> },
    RefreshRelease { policy: crate::presentation::scroll::RefreshControl, distance: f64, cancelled: bool },
    ScrollSnap { policy: crate::presentation::scroll::ScrollSnap, offset:f64, velocity:f64, viewport:f64, content:f64 },
    DisplayText { text: String, appearance: crate::presentation::style::TextAppearance },
    Gradient { gradient: crate::presentation::style::ViewGradient, size: [f64; 2] },
    ProgressiveFilters { profile: crate::presentation::style::ViewProgressiveBlur, filters: Vec<crate::presentation::style::ViewFilter> },
    FilterMatrices { filters: Vec<crate::presentation::style::ViewFilter> },
    SurfaceTransform { appearance: crate::presentation::style::ViewAppearance, size: [f64;2], motion: [f64;4] },
    RoundedOutline { bounds: [f64;4], radii: [f64;8], continuous: bool },
    #[cfg(feature = "motion")]
    AppearanceMotion { from: crate::presentation::style::ViewAppearance, to: crate::presentation::style::ViewAppearance },
    #[cfg(feature = "motion")]
    KeyframeMotion { animation: crate::presentation::style::ViewAnimation },
    /// UTF-16 boundaries prevent platform filters from splitting a grapheme.
    TextBoundaries { text: String },
    TextSelection { before: String, after: String, selection: [usize; 2] },
    TerminalKey { key: String, shift: bool, alt: bool, control: bool, application_cursor: bool },
}

pub fn dispatch(input: &str) -> Result<String, String> {
    if input.len() > 16 * 1024 * 1024 { return Err("native policy request exceeds budget".into()); }
    let request: Request = serde_json::from_str(input).map_err(|error| error.to_string())?;
    let result: Value = match request {
        #[cfg(feature = "android")]
        Request::CompileShader { specification } => json!(jadix_core::hosted::compile_shader_gles(&specification)?),
        Request::Html { html } => serde_json::to_value(jadix_document::native_html::prepare_native_html(&html)
            .map_err(str::to_owned)?).map_err(|error| error.to_string())?,
        Request::ImageDimensions { width, height, natural } => json!(jadix_document::native_html::image_dimensions(width, height, natural)),
        Request::Pan { axis, activation, delta } => json!(match activation.decide(axis, delta) {
            PanDecision::Wait => "wait", PanDecision::Activate => "activate", PanDecision::Fail => "fail",
        }),
        Request::RefreshRelease { policy, distance, cancelled } => json!(policy.release(distance,cancelled)),
        Request::ScrollSnap { policy, offset, velocity, viewport, content } => json!(policy.target(offset,velocity,viewport,content).ok_or("invalid scroll snapping geometry")?),
        Request::Viewport { nodes } => json!(materialization_plan(&nodes).map_err(str::to_owned)?),
        Request::Gradient { gradient, size } => json!(gradient.resolve(size).ok_or("invalid gradient geometry")?),
        Request::SurfaceTransform { appearance, size, motion } => json!(crate::presentation::style::surface_transform(&appearance,size,motion).ok_or("invalid surface transform")?),
        Request::RoundedOutline { bounds, radii, continuous } => json!(crate::presentation::style::rounded_outline(bounds,radii,continuous).ok_or("invalid rounded outline")?),
        Request::ProgressiveFilters { profile, filters } => {
            if filters.len() > 32 || filters.iter().any(|f| !f.is_valid()) { return Err("invalid progressive filters".into()); }
            let masks = profile.sample_masks().ok_or("invalid progressive profile")?;
            json!(masks.into_iter().map(|(strength, stops)| {
                let filters: Vec<_> = if strength == 0. { Vec::new() }
                    else { filters.iter().copied().map(|filter| filter.at_strength(strength)).collect() };
                let matrices: Vec<_> = filters.iter().map(|filter| filter.color_matrix()).collect();
                json!({"strength":strength,"stops":stops,"filters":filters,"matrices":matrices})
            }).collect::<Vec<_>>())
        }
        Request::FilterMatrices { filters } => {
            if filters.len() > 32 || filters.iter().any(|f| !f.is_valid()) { return Err("invalid native filters".into()); }
            json!(filters.into_iter().map(|filter| filter.color_matrix()).collect::<Vec<_>>())
        }
        #[cfg(feature = "motion")]
        Request::AppearanceMotion { from, to } => {
            if !from.is_valid() || !to.is_valid() { return Err("invalid appearance motion".into()); }
            if let Some(transition)=to.transition {
                let (duration,curve)=jadix_animation::presentation::transition_curve(&transition).ok_or("invalid transition")?;
                json!({"duration":duration,"delay":transition.delay,"curve":curve,"channels":from.transition_channels(&to,transition.properties)})
            } else { Value::Null }
        }
        #[cfg(feature = "motion")]
        Request::KeyframeMotion { animation } => json!(jadix_animation::presentation::keyframe_curve(&animation).ok_or("invalid animation")?),
        Request::DisplayText { text, appearance } => {
            if !appearance.is_valid() { return Err("invalid native text appearance".into()); }
            json!(appearance.display_text(&text))
        }
        Request::TextBoundaries { text } => {
            if text.len() > 1024 * 1024 { return Err("native text exceeds budget".into()); }
            let mut boundaries = vec![0usize]; let mut offset = 0;
            for cluster in text.graphemes(true) { offset += cluster.encode_utf16().count(); boundaries.push(offset); }
            json!(boundaries)
        }
        Request::TerminalKey { key, shift, alt, control, application_cursor } => json!(jadix_core::terminal::special_key(&key, shift, alt, control, application_cursor)),
        Request::TextSelection { before, after, selection } => {
            if before.len() > 1024 * 1024 || after.len() > 1024 * 1024 { return Err("native text exceeds budget".into()); }
            json!(jadix_core::text_selection::remap_utf16(&before, &after, selection))
        }
    };
    serde_json::to_string(&result).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_preserves_core_policies_and_rejects_invalid_requests() {
        assert_eq!(dispatch(r#"{"operation":"textBoundaries","text":"a👨‍👩‍👧‍👦é"}"#).unwrap(), "[0,1,12,14]");
        assert_eq!(dispatch(r#"{"operation":"pan","axis":"horizontal","activation":{"activeOffsetX":12,"failOffsetY":8},"delta":[14,9]}"#).unwrap(), r#""fail""#);
        assert_eq!(dispatch(r#"{"operation":"imageDimensions","width":40,"natural":[20,10]}"#).unwrap(), "[40.0,20.0]");
        let plan: Value = serde_json::from_str(&dispatch(r#"{"operation":"html","html":"<script>bad()</script><p>a<img src='https://example.com/x' width='40'></p>"}"#).unwrap()).unwrap();
        assert!(!plan["html"].as_str().unwrap().contains("bad()"));
        assert_eq!(plan["images"][0]["width"], 40.0);
        assert!(dispatch(r#"{"operation":"html","html":"x","extra":true}"#).is_err());
    }
}
