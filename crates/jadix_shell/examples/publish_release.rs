//! Offline signer/static-origin publisher. Never generates a production key.
use ed25519_dalek::{Signer, SigningKey};
use jadix_shell::{package::*, publication::*};
use std::{collections::BTreeMap, io::Read, path::Path};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: publish_release MANIFEST_TEMPLATE RESOURCE_ROOT STATIC_ORIGIN_DIRECTORY; key: JADIX_RELEASE_SIGNING_KEY (32-byte lowercase hex)".into());
    }
    let secret = std::env::var("JADIX_RELEASE_SIGNING_KEY")
        .map_err(|_| "signing key environment variable is required")?;
    let mut seed = decode_hex::<32>(&secret).ok_or("invalid signing key encoding")?;
    let key = SigningKey::from_bytes(&seed);
    seed.fill(0);
    let mut input = Vec::new();
    std::fs::File::open(&args[0])?
        .take(256 * 1024 + 1)
        .read_to_end(&mut input)?;
    if input.len() > 256 * 1024 {
        return Err("manifest template too large".into());
    }
    let mut manifest: Manifest = serde_json::from_slice(&input)?;
    let resources = read_resources(Path::new(&args[1]), &manifest)?;
    for (name, bytes) in &resources {
        manifest.resources.insert(
            name.clone(),
            Resource {
                sha256: sha256(bytes),
                bytes: bytes.len() as u64,
            },
        );
    }
    let signature = key.sign(
        &manifest
            .signature_input()
            .map_err(|e| format!("manifest: {e:?}"))?,
    );
    let signed = SignedManifest {
        manifest,
        signature: signature
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    };
    let m = &signed.manifest;
    let policy = PackagePolicy {
        package: m.package.clone(),
        publisher: m.publisher.clone(),
        target: m.target,
        host_version: SDK_VERSION.into(),
        minimum_release: 1,
        component_schema: m.component_schema.clone(),
        capabilities: m.capabilities.clone(),
        permissions: m.permissions.keys().cloned().collect(),
        keys: BTreeMap::from([(m.key_id.clone(), key.verifying_key().to_bytes())]),
        revoked_keys: Default::default(),
    };
    let path = publish_release(Path::new(&args[2]), &signed, &resources, &policy)?;
    println!(
        "{}",
        serde_json::json!({"release":m.release,"directory":path,"public_key":key.verifying_key().to_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>()})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("publication failed: {error}");
        std::process::exit(1);
    }
}
