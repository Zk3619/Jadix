//! Validated, bounded font assets shared by platform font registries.
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FONTS: usize = 32;
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct FontData {
    digest: [u8; 32],
    families: Vec<String>,
    bytes: Arc<[u8]>,
}
impl FontData {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
            return Err("font size exceeds limit".into());
        }
        let count = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
        if count == 0 || count > 64 {
            return Err("font collection size exceeds limit".into());
        }
        let mut families = Vec::new();
        for index in 0..count {
            let face = ttf_parser::Face::parse(bytes, index)
                .map_err(|error| format!("invalid font face: {error}"))?;
            let family = face
                .names()
                .into_iter()
                .filter(|name| {
                    matches!(
                        name.name_id,
                        ttf_parser::name_id::TYPOGRAPHIC_FAMILY | ttf_parser::name_id::FAMILY
                    )
                })
                .filter_map(|name| name.to_string())
                .find(|name| !name.is_empty() && name.len() <= 1024 && !name.contains('\0'))
                .ok_or("font has no usable family name")?;
            if !families.contains(&family) {
                families.push(family);
            }
        }
        Ok(Self {
            digest: Sha256::digest(bytes).into(),
            families,
            bytes: bytes.into(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn families(&self) -> &[String] {
        &self.families
    }
}

#[derive(Clone, Default)]
pub struct FontCollection {
    fonts: BTreeMap<[u8; 32], FontData>,
    bytes: usize,
    revision: u64,
}
impl FontCollection {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn fonts(&self) -> impl Iterator<Item = &FontData> {
        self.fonts.values()
    }
    /// Stage on a clone, install in the platform, then commit the staged value.
    pub fn insert(&mut self, font: FontData) -> Result<bool, String> {
        if self.fonts.contains_key(&font.digest) {
            return Ok(false);
        }
        let bytes = self
            .bytes
            .checked_add(font.bytes.len())
            .ok_or("font byte count overflow")?;
        if self.fonts.len() >= MAX_FONTS || bytes > MAX_TOTAL_BYTES {
            return Err("font registry capacity exceeded".into());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("font revision exhausted")?;
        self.fonts.insert(font.digest, font);
        self.bytes = bytes;
        self.revision = revision;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_fonts_cannot_mutate_a_registry() {
        for bytes in [
            &[][..],
            &b"not a font"[..],
            &b"ttcf\0\x01\0\0\xff\xff\xff\xff"[..],
        ] {
            assert!(FontData::parse(bytes).is_err());
        }
        assert_eq!(FontCollection::default().revision(), 0);
    }
}
