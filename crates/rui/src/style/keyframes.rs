//! CSS `@keyframes` registry + sampling (animation MVP).
//!
//! Built-ins align with common Tailwind `animate-*` names (`spin` / `ping` / `pulse` /
//! `bounce` / `fade-in` / `fade-out`). Apps can [`register_keyframes`] for custom names.

use std::cell::RefCell;
use std::collections::HashMap;

/// One stop in a `@keyframes` rule (`offset` ∈ [0, 1]).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeStop {
    pub offset: f32,
    pub opacity: Option<f32>,
    pub rotate_deg: Option<f32>,
    pub scale: Option<[f32; 2]>,
    pub translate: Option<[f32; 2]>,
}

impl KeyframeStop {
    pub fn at(offset: f32) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            opacity: None,
            rotate_deg: None,
            scale: None,
            translate: None,
        }
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
}

/// Interpolated sample at a local progress `t` ∈ [0, 1].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyframeSample {
    pub opacity: Option<f32>,
    pub rotate_deg: Option<f32>,
    pub scale: Option<[f32; 2]>,
    pub translate: Option<[f32; 2]>,
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
    });
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

/// Sample keyframes at local progress `t` ∈ [0, 1] (after direction / timing).
pub fn sample_keyframes(stops: &[KeyframeStop], t: f32) -> KeyframeSample {
    if stops.is_empty() {
        return KeyframeSample::default();
    }
    let t = t.clamp(0.0, 1.0);
    if t <= stops[0].offset {
        return KeyframeSample {
            opacity: stops[0].opacity,
            rotate_deg: stops[0].rotate_deg,
            scale: stops[0].scale,
            translate: stops[0].translate,
        };
    }
    for i in 1..stops.len() {
        if t <= stops[i].offset {
            let span = (stops[i].offset - stops[i - 1].offset).max(1e-6);
            let u = (t - stops[i - 1].offset) / span;
            let a = &stops[i - 1];
            let b = &stops[i];
            return KeyframeSample {
                opacity: lerp_opt_f(a.opacity, b.opacity, u),
                rotate_deg: lerp_opt_f(a.rotate_deg, b.rotate_deg, u),
                scale: lerp_opt_xy(a.scale, b.scale, u),
                translate: lerp_opt_xy(a.translate, b.translate, u),
            };
        }
    }
    let last = stops.last().unwrap();
    KeyframeSample {
        opacity: last.opacity,
        rotate_deg: last.rotate_deg,
        scale: last.scale,
        translate: last.translate,
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
}
