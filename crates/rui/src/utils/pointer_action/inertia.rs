//! Pointer fling + smooth rest (interact.js `inertia` plugin).
//!
//! Exponential decay uses the same `e^(-λΔt)` shape as scroll fling
//! ([`crate::app::scroll_support`] `decay_scroll_velocity`), so a flung tile
//! and a flung viewport feel like one product. interact defaults:
//! `resistance=10`, `minSpeed=100`, `endSpeed=10`, `smoothEndDuration=300ms`.

use std::time::Instant;

/// interact `defaults.perAction.inertia`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InertiaOpts {
    pub enabled: bool,
    /// λ in `v' = v * e^(-λ Δt)` (interact `resistance`).
    pub resistance: f32,
    /// px/s; below this, inertia does not start (interact `minSpeed`).
    pub min_speed: f32,
    /// px/s; coast stops here (interact `endSpeed`). RUI also stops components
    /// independently at 12 px/s like scroll, whichever is higher.
    pub end_speed: f32,
    /// Seconds to lerp onto an `endOnly` snap/restrict rest when there is no fling.
    pub smooth_end_duration: f32,
    /// Grabbing the same target during coast resumes the action (interact `allowResume`).
    pub allow_resume: bool,
}

impl Default for InertiaOpts {
    fn default() -> Self {
        Self {
            enabled: false,
            resistance: 10.0,
            min_speed: 100.0,
            end_speed: 10.0,
            smooth_end_duration: 0.3,
            allow_resume: true,
        }
    }
}

impl InertiaOpts {
    pub fn enabled(mut self, yes: bool) -> Self {
        self.enabled = yes;
        self
    }
}

/// Sliding-window velocity (RUI beyond interact's last-sample `coords.velocity`).
#[derive(Clone, Debug)]
pub struct VelocityTracker {
    samples: Vec<(Instant, [f32; 2])>,
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self {
            samples: Vec::with_capacity(8),
        }
    }
}

impl VelocityTracker {
    pub fn reset(&mut self) {
        self.samples.clear();
    }

    pub fn sample(&mut self, pos: [f32; 2], at: Instant) {
        self.samples.push((at, pos));
        const KEEP: usize = 8;
        if self.samples.len() > KEEP {
            self.samples.remove(0);
        }
        // Drop samples older than 80ms so a pause does not fling.
        let cutoff = at.checked_sub(std::time::Duration::from_millis(80));
        if let Some(cut) = cutoff {
            self.samples.retain(|(t, _)| *t >= cut);
        }
    }

    /// px/s in pointer space.
    pub fn velocity(&self) -> [f32; 2] {
        let n = self.samples.len();
        if n < 2 {
            return [0.0, 0.0];
        }
        let (t0, p0) = self.samples[0];
        let (t1, p1) = self.samples[n - 1];
        let dt = t1.saturating_duration_since(t0).as_secs_f32();
        if dt < 1e-4 {
            return [0.0, 0.0];
        }
        [(p1[0] - p0[0]) / dt, (p1[1] - p0[1]) / dt]
    }

    pub fn speed(&self) -> f32 {
        let [vx, vy] = self.velocity();
        (vx * vx + vy * vy).sqrt()
    }
}

enum CoastKind {
    Fling {
        vx: f32,
        vy: f32,
    },
    Smooth {
        from: [f32; 2],
        to: [f32; 2],
        t: f32,
        duration: f32,
    },
}

/// Running coast. `step` returns the displacement this frame, or `None` when done.
pub struct Inertia {
    kind: CoastKind,
    opts: InertiaOpts,
}

impl Inertia {
    /// Start a fling when speed ≥ `min_speed`.
    pub fn try_fling(velocity: [f32; 2], opts: InertiaOpts) -> Option<Self> {
        if !opts.enabled {
            return None;
        }
        let speed = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
        if speed < opts.min_speed {
            return None;
        }
        Some(Self {
            kind: CoastKind::Fling {
                vx: velocity[0],
                vy: velocity[1],
            },
            opts,
        })
    }

    /// Animate from `from` to `to` over `opts.smooth_end_duration` (interact `smoothEnd`).
    pub fn try_smooth_end(from: [f32; 2], to: [f32; 2], opts: InertiaOpts) -> Option<Self> {
        if !opts.enabled || opts.smooth_end_duration <= 0.0 {
            return None;
        }
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        if (dx * dx + dy * dy).sqrt() < 0.5 {
            return None;
        }
        Some(Self {
            kind: CoastKind::Smooth {
                from,
                to,
                t: 0.0,
                duration: opts.smooth_end_duration.max(0.016),
            },
            opts,
        })
    }

    pub fn is_smooth(&self) -> bool {
        matches!(self.kind, CoastKind::Smooth { .. })
    }

    /// Displacement to add to the current origin this frame.
    pub fn step(&mut self, dt: f32) -> Option<[f32; 2]> {
        let dt = dt.max(0.0).min(0.05);
        match &mut self.kind {
            CoastKind::Fling { vx, vy } => {
                let damp = (-self.opts.resistance * dt).exp();
                let ox = *vx * dt;
                let oy = *vy * dt;
                *vx *= damp;
                *vy *= damp;
                let stop = self.opts.end_speed.max(12.0);
                if vx.abs() < stop {
                    *vx = 0.0;
                }
                if vy.abs() < stop {
                    *vy = 0.0;
                }
                if *vx == 0.0 && *vy == 0.0 {
                    None
                } else {
                    Some([ox, oy])
                }
            }
            CoastKind::Smooth {
                from,
                to,
                t,
                duration,
            } => {
                let prev = *t;
                *t = (*t + dt).min(*duration);
                if *t >= *duration && prev >= *duration {
                    return None;
                }
                let a = ease_out_cubic((prev / *duration).clamp(0.0, 1.0));
                let b = ease_out_cubic((*t / *duration).clamp(0.0, 1.0));
                let dx = (to[0] - from[0]) * (b - a);
                let dy = (to[1] - from[1]) * (b - a);
                if *t >= *duration {
                    Some([dx, dy])
                } else {
                    Some([dx, dy])
                }
            }
        }
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fling_requires_min_speed() {
        let opts = InertiaOpts {
            enabled: true,
            ..Default::default()
        };
        assert!(Inertia::try_fling([10.0, 0.0], opts).is_none());
        assert!(Inertia::try_fling([120.0, 0.0], opts).is_some());
        assert!(Inertia::try_fling([120.0, 0.0], InertiaOpts::default()).is_none());
    }

    #[test]
    fn fling_decays_and_stops() {
        let opts = InertiaOpts {
            enabled: true,
            resistance: 10.0,
            min_speed: 50.0,
            end_speed: 10.0,
            ..Default::default()
        };
        let mut i = Inertia::try_fling([400.0, 0.0], opts).unwrap();
        let mut travelled = 0.0;
        let mut frames = 0;
        while let Some(d) = i.step(1.0 / 60.0) {
            travelled += d[0];
            frames += 1;
            if frames > 600 {
                panic!("did not stop");
            }
        }
        assert!(travelled > 10.0, "{travelled}");
        assert!(frames > 5);
    }

    #[test]
    fn smooth_end_reaches_target() {
        let opts = InertiaOpts {
            enabled: true,
            smooth_end_duration: 0.3,
            ..Default::default()
        };
        let mut i = Inertia::try_smooth_end([0.0, 0.0], [30.0, 0.0], opts).unwrap();
        let mut x = 0.0;
        let mut frames = 0;
        while let Some(d) = i.step(1.0 / 60.0) {
            x += d[0];
            frames += 1;
            if frames > 120 {
                break;
            }
        }
        assert!((x - 30.0).abs() < 0.6, "{x}");
    }

    #[test]
    fn velocity_window_ignores_old_samples() {
        let mut t = VelocityTracker::default();
        let t0 = Instant::now();
        t.sample([0.0, 0.0], t0);
        t.sample([100.0, 0.0], t0 + Duration::from_millis(20));
        let v = t.velocity();
        assert!(v[0] > 1000.0, "{v:?}");
        t.sample([100.0, 0.0], t0 + Duration::from_millis(200));
        t.sample([100.0, 0.0], t0 + Duration::from_millis(210));
        assert!(t.speed() < 50.0, "{}", t.speed());
    }

    #[test]
    fn tiny_dt_does_not_explode() {
        let mut t = VelocityTracker::default();
        let t0 = Instant::now();
        t.sample([0.0, 0.0], t0);
        t.sample([8.0, 0.0], t0);
        assert_eq!(t.velocity(), [0.0, 0.0]);
    }
}
