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
use std::collections::{BTreeMap, HashMap};

use super::animatable::{interpolate, AnimatableValue, PropertyId, PropertySet};
use super::element_style::TimingFunction;

/// One stop in a `@keyframes` rule (`offset` ∈ [0, 1]).
///
/// Properties live in [`Self::props`] (ANIM table). Builder methods remain the
/// authoring API — do not add more struct fields for new CSS properties.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeStop {
    pub offset: f32,
    /// CSS `animation-timing-function` declared *inside* a keyframe: eases the segment
    /// that starts at this stop. `None` → inherit the animation's timing function.
    pub timing: Option<TimingFunction>,
    props: BTreeMap<PropertyId, AnimatableValue>,
}

impl KeyframeStop {
    pub fn at(offset: f32) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            timing: None,
            props: BTreeMap::new(),
        }
    }

    pub fn with_props(mut self, props: BTreeMap<PropertyId, AnimatableValue>) -> Self {
        self.props = props;
        self
    }

    pub fn set(mut self, id: PropertyId, v: AnimatableValue) -> Self {
        self.props.insert(id, v);
        self
    }

    pub fn get(&self, id: PropertyId) -> Option<&AnimatableValue> {
        self.props.get(&id)
    }

    pub fn props(&self) -> &BTreeMap<PropertyId, AnimatableValue> {
        &self.props
    }

    /// Per-keyframe `animation-timing-function` (applies to the outgoing segment).
    pub fn timing(mut self, timing: TimingFunction) -> Self {
        self.timing = Some(timing);
        self
    }

    pub fn opacity(self, v: f32) -> Self {
        self.set(PropertyId::Opacity, AnimatableValue::Scalar(v))
    }

    pub fn rotate_deg(self, v: f32) -> Self {
        self.set(PropertyId::Rotate, AnimatableValue::Scalar(v))
    }

    pub fn scale(self, xy: [f32; 2]) -> Self {
        self.set(PropertyId::Scale, AnimatableValue::Vec2(xy))
    }

    pub fn scale_uniform(self, s: f32) -> Self {
        self.scale([s, s])
    }

    pub fn translate(self, xy: [f32; 2]) -> Self {
        self.set(PropertyId::Translate, AnimatableValue::Vec2(xy))
    }

    pub fn translate_pct(self, xy: [f32; 2]) -> Self {
        self.set(PropertyId::TranslatePct, AnimatableValue::Vec2(xy))
    }

    pub fn max_height(self, px: f32) -> Self {
        self.set(PropertyId::MaxHeight, AnimatableValue::Scalar(px.max(0.0)))
    }

    pub fn background_color(self, c: [f32; 4]) -> Self {
        self.set(PropertyId::BackgroundColor, AnimatableValue::Color(c))
    }

    pub fn color(self, c: [f32; 4]) -> Self {
        self.set(PropertyId::Color, AnimatableValue::Color(c))
    }

    pub fn filter(self, f: Option<super::filter::FilterList>) -> Self {
        self.set(PropertyId::Filter, AnimatableValue::Filter(f))
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
    pub props: BTreeMap<PropertyId, AnimatableValue>,
}

impl KeyframeSample {
    fn from_props(props: BTreeMap<PropertyId, AnimatableValue>) -> Self {
        fn scalar(props: &BTreeMap<PropertyId, AnimatableValue>, id: PropertyId) -> Option<f32> {
            match props.get(&id) {
                Some(AnimatableValue::Scalar(x)) => Some(*x),
                _ => None,
            }
        }
        fn vec2(props: &BTreeMap<PropertyId, AnimatableValue>, id: PropertyId) -> Option<[f32; 2]> {
            match props.get(&id) {
                Some(AnimatableValue::Vec2(x)) => Some(*x),
                _ => None,
            }
        }
        Self {
            opacity: scalar(&props, PropertyId::Opacity),
            rotate_deg: scalar(&props, PropertyId::Rotate),
            scale: vec2(&props, PropertyId::Scale),
            translate: vec2(&props, PropertyId::Translate),
            max_height: scalar(&props, PropertyId::MaxHeight),
            props,
        }
    }
}

/// Element values the animation is layered on top of (CSS base value).
///
/// Supplies the implicit `0%` / `100%` keyframes for properties a rule only declares
/// at one end.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeBase {
    pub opacity: f32,
    pub rotate_deg: f32,
    pub scale: [f32; 2],
    pub translate: [f32; 2],
    pub max_height: f32,
    extra: HashMap<PropertyId, AnimatableValue>,
}

impl KeyframeBase {
    pub fn layered(
        opacity: f32,
        rotate_deg: f32,
        scale: [f32; 2],
        translate: [f32; 2],
        max_height: f32,
    ) -> Self {
        Self {
            opacity,
            rotate_deg,
            scale,
            translate,
            max_height,
            extra: HashMap::new(),
        }
    }

    pub fn get(&self, id: PropertyId) -> Option<AnimatableValue> {
        if let Some(v) = self.extra.get(&id) {
            return Some(v.clone());
        }
        Some(match id {
            PropertyId::Opacity => AnimatableValue::Scalar(self.opacity),
            PropertyId::Rotate => AnimatableValue::Scalar(self.rotate_deg),
            PropertyId::Scale => AnimatableValue::Vec2(self.scale),
            PropertyId::Translate => AnimatableValue::Vec2(self.translate),
            PropertyId::MaxHeight => AnimatableValue::Scalar(self.max_height),
            PropertyId::Transform => AnimatableValue::Transform([
                self.translate[0],
                self.translate[1],
                self.scale[0],
                self.scale[1],
                self.rotate_deg,
            ]),
            _ => return None,
        })
    }

    pub fn set_extra(&mut self, id: PropertyId, v: AnimatableValue) {
        self.extra.insert(id, v);
    }

    /// Specified + used-layout base for implicit 0%/100% endpoints.
    pub fn from_specified(
        spec: &super::specified::SpecifiedPresent,
        style: &super::Style,
        text: &super::text_style::TextStyle,
        used: &super::used_layout::UsedLayout,
        fallback: impl FnOnce() -> Self,
    ) -> Self {
        let mut kf = fallback();
        for id in PropertyId::ALL {
            kf.set_extra(*id, spec.resolved_for_base(*id, style, text, used));
        }
        if let Some(AnimatableValue::Scalar(x)) = kf.get(PropertyId::Opacity) {
            kf.opacity = x;
        }
        if let Some(AnimatableValue::Scalar(x)) = kf.get(PropertyId::Rotate) {
            kf.rotate_deg = x;
        }
        if let Some(AnimatableValue::Vec2(x)) = kf.get(PropertyId::Scale) {
            kf.scale = x;
        }
        if let Some(AnimatableValue::Vec2(x)) = kf.get(PropertyId::Translate) {
            kf.translate = x;
        }
        match kf.get(PropertyId::MaxHeight) {
            Some(AnimatableValue::Scalar(x)) if x > 0.5 => kf.max_height = x,
            _ => {
                let u = used.anim_height();
                if u > 0.5 {
                    kf.max_height = u;
                    kf.set_extra(PropertyId::MaxHeight, AnimatableValue::Scalar(u));
                }
            }
        }
        kf
    }
}

impl Default for KeyframeBase {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            rotate_deg: 0.0,
            scale: [1.0, 1.0],
            translate: [0.0, 0.0],
            max_height: 0.0,
            extra: HashMap::new(),
        }
    }
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
    pub ids: PropertySet,
}

/// Properties declared anywhere in `stops`.
pub fn keyframe_coverage(stops: &[KeyframeStop]) -> KeyframeCoverage {
    let mut ids = PropertySet::default();
    for s in stops {
        for id in s.props().keys() {
            ids.insert(*id);
        }
    }
    KeyframeCoverage {
        opacity: ids.contains(PropertyId::Opacity),
        transform: ids.contains(PropertyId::Rotate)
            || ids.contains(PropertyId::Scale)
            || ids.contains(PropertyId::Translate)
            || ids.contains(PropertyId::TranslatePct)
            || ids.contains(PropertyId::Transform),
        max_height: ids.contains(PropertyId::MaxHeight),
        ids,
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
                KeyframeStop::at(0.0).translate_pct([0.0, 0.0]),
                KeyframeStop::at(0.4).translate_pct([0.0, 0.0]),
                KeyframeStop::at(0.5).translate_pct([0.0, -0.25]),
                KeyframeStop::at(0.6).translate_pct([0.0, 0.0]),
                KeyframeStop::at(0.75).translate_pct([0.0, -0.12]),
                KeyframeStop::at(0.9).translate_pct([0.0, 0.0]),
                KeyframeStop::at(1.0).translate_pct([0.0, 0.0]),
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
        register_keyframes(
            "fade-in-blur",
            vec![
                KeyframeStop::at(0.0)
                    .opacity(0.0)
                    .filter(super::parse_filter("blur(8px)")),
                KeyframeStop::at(1.0)
                    .opacity(1.0)
                    .filter(super::parse_filter("blur(0px)")),
            ],
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
        // Sheet / Drawer exit: CSS `translate: 100%` of the panel box, not viewport px.
        // Pixel distances registered at seed time jumped the first frame off-screen.
        for (name, pct) in [
            ("top", [0.0, -1.0]),
            ("bottom", [0.0, 1.0]),
            ("left", [-1.0, 0.0]),
            ("right", [1.0, 0.0]),
        ] {
            register_keyframes(
                format!("jadix-modal-slide-in-{name}"),
                vec![
                    KeyframeStop::at(0.0).translate_pct(pct),
                    KeyframeStop::at(1.0).translate_pct([0.0, 0.0]),
                ],
            );
            register_keyframes(
                format!("jadix-modal-slide-out-{name}"),
                vec![
                    KeyframeStop::at(0.0).translate_pct([0.0, 0.0]),
                    KeyframeStop::at(1.0).translate_pct(pct),
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

/// Sample keyframes at local progress `t` ∈ [0, 1] (after `animation-direction`).
///
/// Legacy entry point: linear segments, and a property declared at only one end holds
/// that value. Prefer [`sample_keyframes_eased`], which applies the timing function per
/// segment and honours implicit keyframes.
pub fn sample_keyframes(stops: &[KeyframeStop], t: f32) -> KeyframeSample {
    sample_keyframes_eased(stops, t, TimingFunction::Linear, None)
}

fn resolve_sample_value(
    id: PropertyId,
    v: AnimatableValue,
    base: Option<&KeyframeBase>,
) -> AnimatableValue {
    let AnimatableValue::Dim(_) = &v else {
        return v;
    };
    let Some(base) = base else {
        return v;
    };
    let mut used = super::used_layout::UsedLayout::default();
    let pick = |id: PropertyId, floor: f32| match base.get(id) {
        Some(AnimatableValue::Scalar(x)) if x > 0.5 => x,
        _ => floor,
    };
    let h = pick(PropertyId::Height, 0.0)
        .max(pick(PropertyId::MaxHeight, 0.0))
        .max(base.max_height);
    let w = pick(PropertyId::Width, 0.0).max(pick(PropertyId::MaxWidth, 0.0));
    used.content_height = h;
    used.intrinsic_height = h;
    used.border_box[3] = h;
    used.content_width = w;
    used.intrinsic_width = w;
    used.border_box[2] = w;
    super::animatable::resolve_used(id, v, &used)
}

fn stop_value(
    stop: Option<&KeyframeStop>,
    id: PropertyId,
    base: Option<&KeyframeBase>,
) -> Option<AnimatableValue> {
    if let Some(s) = stop {
        if let Some(v) = s.get(id) {
            return Some(v.clone());
        }
    }
    base.and_then(|b| b.get(id))
}

fn implicit_ids(
    stops: &[KeyframeStop],
    a: Option<&KeyframeStop>,
    b: Option<&KeyframeStop>,
) -> PropertySet {
    let mut ids = keyframe_coverage(stops).ids;
    for stop in [a, b].into_iter().flatten() {
        for id in stop.props().keys() {
            ids.insert(*id);
        }
    }
    ids
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

    let (a_stop, b_stop, a_off, b_off, seg_timing) = if t <= stops[0].offset {
        (None, Some(&stops[0]), 0.0, stops[0].offset, timing)
    } else {
        match stops.iter().position(|s| t <= s.offset).filter(|&i| i > 0) {
            Some(i) => (
                Some(&stops[i - 1]),
                Some(&stops[i]),
                stops[i - 1].offset,
                stops[i].offset,
                stops[i - 1].timing.unwrap_or(timing),
            ),
            None => {
                let last = stops.last().unwrap();
                (
                    Some(last),
                    None,
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

    let mut props = BTreeMap::new();
    for id in implicit_ids(stops, a_stop, b_stop).iter() {
        let av = stop_value(a_stop, id, base);
        let bv = stop_value(b_stop, id, base);
        match (av, bv) {
            (Some(x), Some(y)) => {
                let x = resolve_sample_value(id, x, base);
                let y = resolve_sample_value(id, y, base);
                if let Some(m) = interpolate(&x, &y, u) {
                    props.insert(id, m);
                }
            }
            (Some(x), None) | (None, Some(x)) => {
                props.insert(id, resolve_sample_value(id, x, base));
            }
            (None, None) => {}
        }
    }
    KeyframeSample::from_props(props)
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

    #[test]
    fn bounce_peak_is_percent_not_pixels() {
        // Tailwind `animate-bounce` is translateY(-25%) of the box. A 2rem gallery
        // demo with -25px jumped over the caption (dest-reset then punched it).
        let stops = lookup_keyframes("bounce").expect("bounce");
        let peak = sample_keyframes(&stops, 0.5);
        assert_eq!(peak.translate, None, "bounce must not use px translate");
        let pct = match peak.props.get(&PropertyId::TranslatePct) {
            Some(AnimatableValue::Vec2(xy)) => *xy,
            other => panic!("bounce peak must be translate_pct, got {other:?}"),
        };
        assert!(
            (pct[1] + 0.25).abs() < 0.001,
            "bounce peak must be -25% of self, got {pct:?}"
        );
    }

    fn base() -> KeyframeBase {
        KeyframeBase {
            max_height: 48.0,
            ..Default::default()
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
        assert_eq!(
            slide[0].get(PropertyId::Translate),
            Some(&AnimatableValue::Vec2([0.0, SLIDE_2_PX]))
        );
        assert_eq!(
            slide[1].get(PropertyId::Translate),
            Some(&AnimatableValue::Vec2([0.0, 0.0]))
        );
    }
}
