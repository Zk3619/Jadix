use crate::presentation::PROTOCOL_VERSION;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const PACKAGE_FORMAT: u32 = 1;
pub const SDK_VERSION: &str = "0.1.0";
pub const SIGNATURE_DOMAIN: &[u8] = b"JADIX-SHELL-PACKAGE\0\x01";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Native,
    Browser,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub package: String,
    pub version: String,
    pub publisher: String,
    pub key_id: String,
    pub target: Target,
    pub release: String,
    pub protocol: u32,
    pub sdk: String,
    pub minimum_host: String,
    pub component_schema: String,
    pub capabilities: BTreeSet<String>,
    pub entry: String,
    pub resources: BTreeMap<String, Resource>,
    pub permissions: BTreeMap<String, String>,
    pub storage_namespace: String,
    pub storage_version: u32,
    pub migration_from: BTreeSet<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    pub manifest: Manifest,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageError {
    Format,
    Path,
    Limit,
    Integrity,
    Signature,
    UntrustedKey,
    RevokedKey,
    Rollback,
    Compatibility,
    Permission,
}

pub struct PackagePolicy {
    pub package: String,
    pub publisher: String,
    pub target: Target,
    pub host_version: String,
    pub minimum_release: u64,
    pub component_schema: String,
    pub capabilities: BTreeSet<String>,
    pub permissions: BTreeSet<String>,
    pub keys: BTreeMap<String, [u8; 32]>,
    pub revoked_keys: BTreeSet<String>,
}

impl Manifest {
    pub fn signature_input(&self) -> Result<Vec<u8>, PackageError> {
        self.validate()?;
        let mut input = SIGNATURE_DOMAIN.to_vec();
        input.extend(serde_json::to_vec(self).map_err(|_| PackageError::Format)?);
        Ok(input)
    }

    fn validate(&self) -> Result<(), PackageError> {
        if self.format != PACKAGE_FORMAT
            || self.protocol != PROTOCOL_VERSION
            || self.sdk != SDK_VERSION
            || !identifier(&self.package)
            || !identifier(&self.publisher)
            || !identifier(&self.key_id)
            || !identifier(&self.storage_namespace)
            || version(&self.version).is_none()
            || version(&self.minimum_host).is_none()
            || !digest_valid(&self.component_schema)
            || self.release.parse::<u64>().is_err()
            || self.release.parse::<u64>().unwrap().to_string() != self.release
            || self.release == "0"
            || self.resources.is_empty()
            || !self.resources.contains_key(&self.entry)
            || !(self.entry.ends_with(".js") || self.entry.ends_with(".mjs"))
        {
            return Err(PackageError::Format);
        }
        if self.resources.len() > 256
            || self.permissions.len() > 128
            || self.capabilities.len() > 128
            || self.migration_from.len() > 32
        {
            return Err(PackageError::Limit);
        }
        let mut total = 0u64;
        let mut paths = BTreeSet::new();
        for (path, resource) in &self.resources {
            if !safe_path(path) || !paths.insert(path.to_ascii_lowercase()) {
                return Err(PackageError::Path);
            }
            if !digest_valid(&resource.sha256) {
                return Err(PackageError::Format);
            }
            total = total
                .checked_add(resource.bytes)
                .ok_or(PackageError::Limit)?;
            if resource.bytes > 4 * 1024 * 1024 || total > 16 * 1024 * 1024 {
                return Err(PackageError::Limit);
            }
        }
        if self.permissions.iter().any(|(name, reason)| {
            !identifier(name) || reason.trim().is_empty() || reason.len() > 1024
        }) || self.capabilities.iter().any(|name| !identifier(name))
        {
            return Err(PackageError::Format);
        }
        Ok(())
    }
}

impl SignedManifest {
    pub fn decode(bytes: &[u8]) -> Result<Self, PackageError> {
        if bytes.len() > 256 * 1024 {
            return Err(PackageError::Limit);
        }
        let value: crate::schema::Value =
            serde_json::from_slice(bytes).map_err(|_| PackageError::Format)?;
        if !value.is_valid() {
            return Err(PackageError::Limit);
        }
        let signed: Self = serde_json::from_slice(bytes).map_err(|_| PackageError::Format)?;
        signed.manifest.validate()?;
        if decode_hex::<64>(&signed.signature).is_none() {
            return Err(PackageError::Signature);
        }
        Ok(signed)
    }

    pub fn verify(
        &self,
        policy: &PackagePolicy,
        resources: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), PackageError> {
        self.verify_manifest(policy)?;
        if resources.len() != self.manifest.resources.len() {
            return Err(PackageError::Integrity);
        }
        for (path, expected) in &self.manifest.resources {
            let bytes = resources.get(path).ok_or(PackageError::Integrity)?;
            if bytes.len() as u64 != expected.bytes || sha256(bytes) != expected.sha256 {
                return Err(PackageError::Integrity);
            }
        }
        Ok(())
    }

    /// Authenticate metadata and declared budgets before starting resource IO.
    pub fn verify_manifest(&self, policy: &PackagePolicy) -> Result<(), PackageError> {
        let input = self.manifest.signature_input()?;
        if policy.revoked_keys.contains(&self.manifest.key_id) {
            return Err(PackageError::RevokedKey);
        }
        let key = policy
            .keys
            .get(&self.manifest.key_id)
            .ok_or(PackageError::UntrustedKey)?;
        let key = VerifyingKey::from_bytes(key).map_err(|_| PackageError::UntrustedKey)?;
        let signature = Signature::from_bytes(
            &decode_hex::<64>(&self.signature).ok_or(PackageError::Signature)?,
        );
        key.verify_strict(&input, &signature)
            .map_err(|_| PackageError::Signature)?;
        if self.manifest.release.parse::<u64>().unwrap() < policy.minimum_release {
            return Err(PackageError::Rollback);
        }
        if self.manifest.package != policy.package
            || self.manifest.publisher != policy.publisher
            || self.manifest.target != policy.target
            || self.manifest.component_schema != policy.component_schema
            || version(&policy.host_version)
                .is_none_or(|host| host < version(&self.manifest.minimum_host).unwrap())
            || !self.manifest.capabilities.is_subset(&policy.capabilities)
        {
            return Err(PackageError::Compatibility);
        }
        if self
            .manifest
            .permissions
            .keys()
            .any(|permission| !policy.permissions.contains(permission))
        {
            return Err(PackageError::Permission);
        }
        Ok(())
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.ends_with('.')
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
                && !matches!(
                    segment
                        .split('.')
                        .next()
                        .unwrap()
                        .to_ascii_uppercase()
                        .as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn version(value: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut result = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        result[index] = part.parse().ok()?;
        if result[index].to_string() != *part {
            return None;
        }
    }
    Some(result)
}

fn digest_valid(value: &str) -> bool {
    decode_hex::<32>(value).is_some()
}

pub fn decode_hex<const LENGTH: usize>(value: &str) -> Option<[u8; LENGTH]> {
    if value.len() != LENGTH * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut output = [0; LENGTH];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}
