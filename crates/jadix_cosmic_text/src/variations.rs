//! Canonical user-space axis overrides, shared by shaping and glyph caches.

/// At most 16 explicit overrides; other axes retain font defaults. Unknown tags
/// are ignored by each fallback font, and known values clamp to its axis range.
/// A fixed representation keeps physical glyph/cache keys Copy and comparable.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontVariations {
    settings: [(u32, u32); 16],
    len: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VariationError {
    TooManyAxes,
    InvalidValue,
    InvalidTag,
    DuplicateTag,
}
impl core::fmt::Display for VariationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid font variation: {self:?}")
    }
}
#[cfg(feature = "std")]
impl std::error::Error for VariationError {}

impl Default for FontVariations {
    fn default() -> Self {
        Self::empty()
    }
}
impl FontVariations {
    pub const fn empty() -> Self {
        Self {
            settings: [(0, 0); 16],
            len: 0,
        }
    }
    /// Canonicalize a bounded set of explicit axis overrides.
    ///
    /// # Errors
    /// Rejects more than 16 settings, duplicate tags, non-ASCII tags and
    /// nonfinite values. Font-specific range clamping happens after matching.
    pub fn new(values: &[([u8; 4], f32)]) -> Result<Self, VariationError> {
        if values.len() > 16 {
            return Err(VariationError::TooManyAxes);
        }
        let mut result = Self::empty();
        for (i, (tag, value)) in values.iter().enumerate() {
            if !value.is_finite() {
                return Err(VariationError::InvalidValue);
            }
            if !tag.iter().all(|v| (0x20..=0x7e).contains(v)) {
                return Err(VariationError::InvalidTag);
            }
            result.settings[i] = (
                u32::from_be_bytes(*tag),
                if *value == 0. { 0 } else { value.to_bits() },
            );
        }
        result.len = values.len() as u8;
        result.settings[..values.len()].sort_unstable();
        if result.settings[..values.len()]
            .windows(2)
            .any(|w| w[0].0 == w[1].0)
        {
            return Err(VariationError::DuplicateTag);
        }
        Ok(result)
    }
    pub fn iter(&self) -> impl Iterator<Item = ([u8; 4], f32)> + '_ {
        self.settings[..self.len as usize]
            .iter()
            .map(|(tag, bits)| (tag.to_be_bytes(), f32::from_bits(*bits)))
    }
    /// Weight is the default wght setting; an explicit override takes precedence.
    pub fn settings(&self, weight: fontdb::Weight) -> impl Iterator<Item = ([u8; 4], f32)> + '_ {
        core::iter::once((*b"wght", f32::from(weight.0)))
            .filter(|_| !self.iter().any(|(t, _)| t == *b"wght"))
            .chain(self.iter())
    }
}
