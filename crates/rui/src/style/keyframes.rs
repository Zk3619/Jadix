//! CSS `@keyframes` registry + sampling.
//!
//! Built-ins align with common Tailwind `animate-*` names (`spin` / `ping` / `pulse` /
//! `bounce` / `fade-in` / `fade-out`) plus the `tailwindcss-animate` names shadcn uses
//! (`zoom-in-95` / `slide-in-from-*`). Apps can [`register_keyframes`] for custom names.
//!
//! Sampling follows CSS in two ways that a naive implementation gets wrong:
//!
//! - **Easing is per segment.** `animation-timing-function` eases the interval between
//!   each adjacent pair of stops, not the whole iteration. Easing global progress before
//!   the lookup would shift where multi-stop rules (`bounce` / `ping`) place their stops.
//! - **Missing endpoints fall back to the element's base value** (CSS "implicit keyframes"):
//!   `from { opacity: 0 }` alone animates 0 → the cascaded opacity, not a constant 0.

use std::cell::RefCell;
use std::collections::HashMap;

use super::element_style::TimingFunction;

/// One stop in a `@keyframes` rule (`offset` ∈ [0, 1]).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeStop {
    pub offset: f32,
    pub opacity: Option<f32>,
    pub rotate_deg: Option<f32>,
    pub scale: Option<[f32; 2]>,
    pub translate: Option<[f32; 2]>,
    /// Layout clip height (maps to `Style::max_size.height` length px).
    pub max_height: Option<f32>,
    /// CSS `animation-timing-function` declared *inside* a keyframe: eases the segment
    /// that starts at this stop. `None` → inherit the animation's timing function.
    pub timing: Option<TimingFunction>,
}

impl KeyframeStop {
    pub fn at(offset: f32) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            opacity: None,
            rotate_deg: None,
            scale: None,
            translate: None,
            max_height: None,
            timing: None,
        }
    }

    /// Per-keyframe `animation-timing-function` (applies to the outgoing segment).
    pub fn timing(mut self, timing: TimingFunction) -> Self {
        self.timing = Some(timing);
        self
    }

    pub fn opacity(mut self, v: f32) -> Self {
        self.opacity = Some(v);
        self
    }

    pub fn rotate_deg(mut self, v: f32) -> Self {
        self.rotate_deg = Some(v);
        self
    }

    pub fn scale(mut self, xy: [f32; 2]) -> Self {
        self.scale = Some(xy);
        self
    }

    pub fn scale_uniform(mut self, s: f32) -> Self {
        self.scale = Some([s, s]);
        self
    }

    pub fn translate(mut self, xy: [f32; 2]) -> Self {
        self.translate = Some(xy);
        self
    }

    pub fn max_height(mut self, px: f32) -> Self {
        self.max_height = Some(px.max(0.0));
        self
    }
}

/// Interpolated sample at a local progress `t` ∈ [0, 1].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyframeSample {
    pub opacity: Option<f32>,
    pub rotate_deg: Option<f32>,
    pub scale: Option<[f32; 2]>,
    pub translate: Option<[f32; 2]>,
    pub max_height: Option<f32>,
}

/// Element values the animation is layered on top of (CSS base value).
///
/// Supplies the implicit `0%` / `100%` keyframes for properties a rule only declares
/// at one end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyframeBase {
    pub opacity: f32,
    pub rotate_deg: f32,
    pub scale: [f32; 2],
    pub translate: [f32; 2],
    pub max_height: f32,
}

/// Which properties a rule touches anywhere in its stops.
///
/// CSS gives a running animation ownership of the properties it declares; transitions
/// must not fight it. Derived from the authored stops, not from a base-filled sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyframeCoverage {
    pub opacity: bool,
    pub transform: bool,
    pub max_height: bool,
}

/// Properties declared anywhere in `stops`.
pub fn keyframe_coverage(stops: &[KeyframeStop]) -> KeyframeCoverage {
    KeyframeCoverage {
        opacity: stops.iter().any(|s| s.opacity.is_some()),
        transform: stops
            .iter()
            .any(|s| s.rotate_deg.is_some() || s.scale.is_some() || s.translate.is_some()),
        max_height: stops.iter().any(|s| s.max_height.is_some()),
    }
}

thread_local! {
    static REGISTRY: RefCell<HashMap<String, Vec<KeyframeStop>>> = RefCell::new(HashMap::new());
    static BUILTINS_READY: RefCell<bool> = const { RefCell::new(false) };
}

fn ensure_builtins() {
    BUILTINS_READY.with(|ready| {
        if *ready.borrow() {
            return;
        }
        *ready.borrow_mut() = true;
        register_keyframes(
            "spin",
            vec![
                KeyframeStop::at(0.0).rotate_deg(0.0),
                KeyframeStop::at(1.0).rotate_deg(360.0),
            ],
        );
        register_keyframes(
            "ping",
            vec![
                KeyframeStop::at(0.0).scale_uniform(1.0).opacity(1.0),
                KeyframeStop::at(0.75).scale_uniform(2.0).opacity(0.0),
                KeyframeStop::at(1.0).scale_uniform(2.0).opacity(0.0),
            ],
        );
        register_keyframes(
            "pulse",
            vec![
                KeyframeStop::at(0.0).opacity(1.0),
                KeyframeStop::at(0.5).opacity(0.5),
                KeyframeStop::at(1.0).opacity(1.0),
            ],
        );
        register_keyframes(
            "bounce",
            vec![
                KeyframeStop::at(0.0).translate([0.0, 0.0]),
                KeyframeStop::at(0.4).translate([0.0, 0.0]),
                KeyframeStop::at(0.5).translate([0.0, -25.0]),
                KeyframeStop::at(0.6).translate([0.0, 0.0]),
                KeyframeStop::at(0.75).translate([0.0, -12.0]),
                KeyframeStop::at(0.9).translate([0.0, 0.0]),
                KeyframeStop::at(1.0).translate([0.0, 0.0]),
            ],
        );
        register_keyframes(
            "fade-in",
            vec![
                KeyframeStop::at(0.0).opacity(0.0),
                KeyframeStop::at(1.0).opacity(1.0),
            ],
        );
        register_keyframes(
            "fade-out",
            vec![
                KeyframeStop::at(0.0).opacity(1.0),
                KeyframeStop::at(1.0).opacity(0.0),
            ],
        );
        // shadcn / tailwindcss-animate (`zoom-in-95` / `zoom-out-95`)
        register_keyframes(
            "zoom-in-95",
            vec![
                KeyframeStop::at(0.0).scale_uniform(0.95),
                KeyframeStop::at(1.0).scale_uniform(1.0),
            ],
        );
        register_keyframes(
            "zoom-out-95",
            vec![
                KeyframeStop::at(0.0).scale_uniform(1.0),
                KeyframeStop::at(1.0).scale_uniform(0.95),
            ],
        );
        register_keyframes(
            "zoom-in-90",
            vec![
                KeyframeStop::at(0.0).scale_uniform(0.90),
                KeyframeStop::at(1.0).scale_uniform(1.0),
            ],
        );
        register_keyframes(
            "zoom-out-90",
            vec![
                KeyframeStop::at(0.0).scale_uniform(1.0),
                KeyframeStop::at(1.0).scale_uniform(0.90),
            ],
        );
        // `*-0` suffixes are the shadcn spellings of a plain fade.
        register_keyframes(
            "fade-in-0",
            vec![
                KeyframeStop::at(0.0).opacity(0.0),
                KeyframeStop::at(1.0).opacity(1.0),
            ],
        );
        register_keyframes(
            "fade-out-0",
            vec![
                KeyframeStop::at(0.0).opacity(1.0),
                KeyframeStop::at(1.0).opacity(0.0),
            ],
        );
        // shadcn accordion — height 0 ↔ cascaded content height (see KeyframeBase::max_height).
        register_keyframes(
            "accordion-down",
            vec![KeyframeStop::at(0.0).max_height(0.0)],
        );
        register_keyframes("accordion-up", vec![KeyframeStop::at(1.0).max_height(0.0)]);
        // shadcn collapsible — same shape as accordion, separate names (community / tw-animate-css).
        register_keyframes(
            "collapsible-down",
            vec![KeyframeStop::at(0.0).max_height(0.0)],
        );
        register_keyframes(
            "collapsible-up",
            vec![KeyframeStop::at(1.0).max_height(0.0)],
        );
        // shadcn `shimmer` utility (`animation: tw-shimmer`). Visual sweep is painted
        // in prepare_text; this rule keeps the CSS animation pump alive.
        register_keyframes(
            "tw-shimmer",
            vec![
                KeyframeStop::at(0.0).opacity(1.0),
                KeyframeStop::at(1.0).opacity(1.0),
            ],
        );
        // `slide-in-from-*-2` / `slide-out-to-*-2` — Tailwind `-2` is 0.5rem.
        for (name, dxy) in [
            ("top", [0.0, -SLIDE_2_PX]),
            ("bottom", [0.0, SLIDE_2_PX]),
            ("left", [-SLIDE_2_PX, 0.0]),
            ("right", [SLIDE_2_PX, 0.0]),
        ] {
            register_keyframes(
                format!("slide-in-from-{name}-2"),
                vec![
                    KeyframeStop::at(0.0).translate(dxy),
                    KeyframeStop::at(1.0).translate([0.0, 0.0]),
                ],
            );
            register_keyframes(
                format!("slide-out-to-{name}-2"),
                vec![
                    KeyframeStop::at(0.0).translate([0.0, 0.0]),
                    KeyframeStop::at(1.0).translate(dxy),
                ],
            );
        }
    });
}

/// Tailwind spacing `2` (0.5rem) in logical px — the shadcn slide offset.
const SLIDE_2_PX: f32 = 8.0;

/// True when `name` resolves to a registered rule (loads builtins on first call).
pub fn keyframes_exist(name: &str) -> bool {
    ensure_builtins();
    REGISTRY.with(|reg| reg.borrow().contains_key(name))
}

/// Register or replace a named `@keyframes` rule.
pub fn register_keyframes(name: impl Into<String>, mut stops: Vec<KeyframeStop>) {
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let name = name.into();
    REGISTRY.with(|reg| {
        reg.borrow_mut().insert(name, stops);
    });
}

/// Look up keyframes by name (loads builtins on first call).
pub fn lookup_keyframes(name: &str) -> Option<Vec<KeyframeStop>> {
    ensure_builtins();
    REGISTRY.with(|reg| reg.borrow().get(name).cloned())
}

fn lerp_f(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_opt_f(a: Option<f32>, b: Option<f32>, t: f32) -> Option<f32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(lerp_f(x, y, t)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn lerp_opt_xy(a: Option<[f32; 2]>, b: Option<[f32; 2]>, t: f32) -> Option<[f32; 2]> {
    match (a, b) {
        (Some(x), Some(y)) => Some([lerp_f(x[0], y[0], t), lerp_f(x[1], y[1], t)]),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// One endpoint of a segment; an undeclared property falls back to the base value.
#[derive(Clone, Copy, Default)]
struct Endpoint {
    opacity: Option<f32>,
    rotate_deg: Option<f32>,
    scale: Option<[f32; 2]>,
    translate: Option<[f32; 2]>,
    max_height: Option<f32>,
}

impl Endpoint {
    fn from_stop(stop: &KeyframeStop, base: Option<&KeyframeBase>) -> Self {
        Self {
            opacity: stop.opacity.or(base.map(|b| b.opacity)),
            rotate_deg: stop.rotate_deg.or(base.map(|b| b.rotate_deg)),
            scale: stop.scale.or(base.map(|b| b.scale)),
            translate: stop.translate.or(base.map(|b| b.translate)),
            max_height: stop.max_height.or(base.map(|b| b.max_height)),
        }
    }

    /// CSS implicit `0%` / `100%` keyframe: every property comes from the base value.
    fn implicit(base: Option<&KeyframeBase>) -> Self {
        Self {
            opacity: base.map(|b| b.opacity),
            rotate_deg: base.map(|b| b.rotate_deg),
            scale: base.map(|b| b.scale),
            translate: base.map(|b| b.translate),
            max_height: base.map(|b| b.max_height),
        }
    }
}

/// Sample keyframes at local progress `t` ∈ [0, 1] (after `animation-direction`).
///
/// Legacy entry point: linear segments, and a property declared at only one end holds
/// that value. Prefer [`sample_keyframes_eased`], which applies the timing function per
/// segment and honours implicit keyframes.
pub fn sample_keyframes(stops: &[KeyframeStop], t: f32) -> KeyframeSample {
    sample_keyframes_eased(stops, t, TimingFunction::Linear, None)
}

/// Sample keyframes the way CSS does.
///
/// `linear_t` is *un-eased* local progress: the segment is located first, then
/// `timing` (or the segment's own [`KeyframeStop::timing`]) eases the position within it.
/// `base` supplies implicit endpoints; pass `None` to hold single-ended properties.
pub fn sample_keyframes_eased(
    stops: &[KeyframeStop],
    linear_t: f32,
    timing: TimingFunction,
    base: Option<&KeyframeBase>,
) -> KeyframeSample {
    if stops.is_empty() {
        return KeyframeSample::default();
    }
    let t = linear_t.clamp(0.0, 1.0);

    let (a, b, a_off, b_off, seg_timing) = if t <= stops[0].offset {
        // Implicit 0% keyframe → first authored stop.
        (
            Endpoint::implicit(base),
            Endpoint::from_stop(&stops[0], base),
            0.0,
            stops[0].offset,
            timing,
        )
    } else {
        match stops.iter().position(|s| t <= s.offset).filter(|&i| i > 0) {
            Some(i) => (
                Endpoint::from_stop(&stops[i - 1], base),
                Endpoint::from_stop(&stops[i], base),
                stops[i - 1].offset,
                stops[i].offset,
                stops[i - 1].timing.unwrap_or(timing),
            ),
            // Past the last authored stop → implicit 100% keyframe.
            None => {
                let last = stops.last().unwrap();
                (
                    Endpoint::from_stop(last, base),
                    Endpoint::implicit(base),
                    last.offset,
                    1.0,
                    last.timing.unwrap_or(timing),
                )
            }
        }
    };

    let span = b_off - a_off;
    let u = if span <= 1e-6 {
        1.0
    } else {
        seg_timing.eval((t - a_off) / span)
    };

    KeyframeSample {
        opacity: lerp_opt_f(a.opacity, b.opacity, u),
        rotate_deg: lerp_opt_f(a.rotate_deg, b.rotate_deg, u),
        scale: lerp_opt_xy(a.scale, b.scale, u),
        translate: lerp_opt_xy(a.translate, b.translate, u),
        max_height: lerp_opt_f(a.max_height, b.max_height, u),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spin_samples_rotate() {
        let stops = lookup_keyframes("spin").expect("spin");
        let mid = sample_keyframes(&stops, 0.5);
        assert!((mid.rotate_deg.unwrap() - 180.0).abs() < 0.01);
    }

    #[test]
    fn pulse_samples_opacity() {
        let stops = lookup_keyframes("pulse").expect("pulse");
        let mid = sample_keyframes(&stops, 0.5);
        assert!((mid.opacity.unwrap() - 0.5).abs() < 0.01);
    }

    fn base() -> KeyframeBase {
        KeyframeBase {
            opacity: 1.0,
            rotate_deg: 0.0,
            scale: [1.0, 1.0],
            translate: [0.0, 0.0],
            max_height: 48.0,
        }
    }

    #[test]
    fn easing_applies_per_segment_not_to_global_progress() {
        // Two equal segments, `ease-in` on each. At the segment boundary (t=0.5) the
        // value must be exactly the authored stop — easing global t would overshoot it.
        let stops = vec![
            KeyframeStop::at(0.0).opacity(0.0),
            KeyframeStop::at(0.5).opacity(1.0),
            KeyframeStop::at(1.0).opacity(0.0),
        ];
        let at_stop = sample_keyframes_eased(&stops, 0.5, TimingFunction::EaseIn, None);
        assert!(
            (at_stop.opacity.unwrap() - 1.0).abs() < 1e-3,
            "authored stop must be hit exactly, got {:?}",
            at_stop.opacity
        );
        // Inside the first segment, ease-in lags linear.
        let quarter = sample_keyframes_eased(&stops, 0.25, TimingFunction::EaseIn, None);
        assert!(
            quarter.opacity.unwrap() < 0.5,
            "ease-in should lag linear inside the segment, got {:?}",
            quarter.opacity
        );
    }

    #[test]
    fn per_keyframe_timing_overrides_animation_timing() {
        let stops = vec![
            KeyframeStop::at(0.0)
                .opacity(0.0)
                .timing(TimingFunction::Linear),
            KeyframeStop::at(1.0).opacity(1.0),
        ];
        let mid = sample_keyframes_eased(&stops, 0.5, TimingFunction::EaseIn, None);
        assert!(
            (mid.opacity.unwrap() - 0.5).abs() < 1e-3,
            "stop-level linear must win over entry-level ease-in, got {:?}",
            mid.opacity
        );
    }

    #[test]
    fn missing_endpoint_falls_back_to_base_value() {
        // `from { opacity: 0 }` with no `to` → animates 0 → base (1.0), like CSS.
        let stops = vec![KeyframeStop::at(0.0).opacity(0.0)];
        let b = base();
        let mid = sample_keyframes_eased(&stops, 0.5, TimingFunction::Linear, Some(&b));
        assert!(
            (mid.opacity.unwrap() - 0.5).abs() < 1e-3,
            "expected interpolation toward base 1.0, got {:?}",
            mid.opacity
        );
        let end = sample_keyframes_eased(&stops, 1.0, TimingFunction::Linear, Some(&b));
        assert!((end.opacity.unwrap() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn without_base_single_ended_property_holds() {
        let stops = vec![KeyframeStop::at(0.0).opacity(0.0)];
        let mid = sample_keyframes_eased(&stops, 0.5, TimingFunction::Linear, None);
        assert!((mid.opacity.unwrap() - 0.0).abs() < 1e-3);
    }

    #[test]
    fn undeclared_property_is_left_alone_without_base() {
        let stops = lookup_keyframes("fade-in").expect("fade-in");
        let mid = sample_keyframes(&stops, 0.5);
        assert!(mid.scale.is_none(), "fade-in must not touch scale");
        assert!(mid.translate.is_none());
    }

    #[test]
    fn coverage_reports_declared_properties_only() {
        let fade = keyframe_coverage(&lookup_keyframes("fade-in").expect("fade-in"));
        assert!(fade.opacity && !fade.transform);
        let spin = keyframe_coverage(&lookup_keyframes("spin").expect("spin"));
        assert!(!spin.opacity && spin.transform);
    }

    #[test]
    fn accordion_height_keyframes_interpolate_toward_base() {
        let down = lookup_keyframes("accordion-down").expect("accordion-down");
        let b = base();
        let mid = sample_keyframes_eased(&down, 0.5, TimingFunction::Linear, Some(&b));
        assert!((mid.max_height.unwrap() - 24.0).abs() < 1e-3);
        let up = lookup_keyframes("accordion-up").expect("accordion-up");
        let mid_up = sample_keyframes_eased(&up, 0.5, TimingFunction::Linear, Some(&b));
        assert!((mid_up.max_height.unwrap() - 24.0).abs() < 1e-3);
    }

    #[test]
    fn collapsible_height_keyframes_match_accordion_shape() {
        let down = lookup_keyframes("collapsible-down").expect("collapsible-down");
        let b = base();
        let mid = sample_keyframes_eased(&down, 0.5, TimingFunction::Linear, Some(&b));
        assert!((mid.max_height.unwrap() - 24.0).abs() < 1e-3);
        let up = lookup_keyframes("collapsible-up").expect("collapsible-up");
        let mid_up = sample_keyframes_eased(&up, 0.5, TimingFunction::Linear, Some(&b));
        assert!((mid_up.max_height.unwrap() - 24.0).abs() < 1e-3);
    }

    #[test]
    fn shadcn_slide_names_are_registered() {
        for name in [
            "fade-in-0",
            "fade-out-0",
            "accordion-down",
            "accordion-up",
            "collapsible-down",
            "collapsible-up",
            "slide-in-from-top-2",
            "slide-out-to-right-2",
            "zoom-in-90",
        ] {
            assert!(keyframes_exist(name), "{name} should be a builtin");
        }
        let slide = lookup_keyframes("slide-in-from-bottom-2").expect("slide");
        assert_eq!(slide[0].translate, Some([0.0, SLIDE_2_PX]));
        assert_eq!(slide[1].translate, Some([0.0, 0.0]));
    }
}
