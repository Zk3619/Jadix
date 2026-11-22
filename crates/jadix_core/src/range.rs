//! Shared finite range arithmetic for pointer, keyboard and accessibility input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range { min: f64, max: f64, step: f64 }
impl Range {
    pub fn new(min: f64, max: f64, step: f64) -> Result<Self, &'static str> {
        let span = max - min;
        if !min.is_finite() || !max.is_finite() || !span.is_finite() || span <= 0.
            || !step.is_finite() || step < 0. || (step > 0. && !(span / step).is_finite()) {
            return Err("invalid finite range");
        }
        Ok(Self { min, max, step })
    }
    pub fn min(self) -> f64 { self.min }
    pub fn max(self) -> f64 { self.max }
    pub fn clamp(self, value: f64) -> Option<f64> { value.is_finite().then(|| value.clamp(self.min, self.max)) }
    pub fn snap(self, value: f64) -> Option<f64> {
        let value = self.clamp(value)?;
        if value == self.min || value == self.max { return Some(value); }
        if self.step == 0. { return Some(value); }
        Some((self.min + ((value - self.min) / self.step).round() * self.step).clamp(self.min, self.max))
    }
    pub fn fraction(self, value: f64) -> Option<f64> { self.clamp(value).map(|value| (value - self.min) / (self.max - self.min)) }
    pub fn at_fraction(self, fraction: f64) -> Option<f64> {
        fraction.is_finite().then(|| self.min + fraction.clamp(0., 1.) * (self.max - self.min))
    }
    pub fn advance(self, value: f64, steps: f64) -> Option<f64> {
        let value = self.clamp(value)?;
        if !steps.is_finite() { return None; }
        let step = if self.step > 0. { self.step } else { (self.max - self.min) / 100. };
        let next = value + step * steps;
        // Overflow from a finite movement reaches its corresponding end.
        self.snap(if next.is_infinite() { if next.is_sign_positive() { self.max } else { self.min } } else { next })
    }
}
