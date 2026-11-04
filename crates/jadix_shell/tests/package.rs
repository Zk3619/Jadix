use ed25519_dalek::{Signer, SigningKey};
use jadix_shell::package::*;
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> (SignedManifest, PackagePolicy, BTreeMap<String, Vec<u8>>) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let bytes = b"export default { component: 'Text', text: 'hello' };".to_vec();
    let schema = sha256(include_bytes!(
        "../../jadix_components/schema/portable-v1.json"
    ));
    let manifest = Manifest {
        format: 1,
        package: "example.counter".into(),
        version: "1.0.0".into(),
        publisher: "first-party".into(),
        key_id: "test-key".into(),
        target: Target::Native,
        release: "7".into(),
        protocol: 1,
        sdk: SDK_VERSION.into(),
        minimum_host: "0.1.0".into(),
        component_schema: schema.clone(),
        capabilities: BTreeSet::new(),
        entry: "main.mjs".into(),
        resources: BTreeMap::from([(
            "main.mjs".into(),
            Resource {
                sha256: sha256(&bytes),
                bytes: bytes.len() as u64,
            },
        )]),
        permissions: BTreeMap::from([("counter.save".into(), "Save counter".into())]),
        storage_namespace: "example.counter".into(),
        storage_version: 1,
        migration_from: BTreeSet::new(),
    };
    let signature = key
        .sign(&manifest.signature_input().unwrap())
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let policy = PackagePolicy {
        package: manifest.package.clone(),
        publisher: manifest.publisher.clone(),
        target: Target::Native,
        host_version: "0.1.0".into(),
        minimum_release: 7,
        component_schema: schema,
        capabilities: BTreeSet::new(),
        permissions: BTreeSet::from(["counter.save".into()]),
        keys: BTreeMap::from([("test-key".into(), key.verifying_key().to_bytes())]),
        revoked_keys: BTreeSet::new(),
    };
    (
        SignedManifest {
            manifest,
            signature,
        },
        policy,
        BTreeMap::from([("main.mjs".into(), bytes)]),
    )
}

#[test]
fn package_v1_signature_bytes_match_independently_generated_fixed_vector() {
    let vector: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/package-signature-v1.json")).unwrap();
    let (signed, policy, resources) = fixture();
    let input = signed.manifest.signature_input().unwrap();
    assert_eq!(&input[..SIGNATURE_DOMAIN.len()], SIGNATURE_DOMAIN);
    assert_eq!(
        std::str::from_utf8(&input[SIGNATURE_DOMAIN.len()..]).unwrap(),
        vector["canonical_manifest"].as_str().unwrap()
    );
    assert_eq!(sha256(&input), vector["input_sha256"].as_str().unwrap());
    assert_eq!(signed.signature, vector["signature"].as_str().unwrap());
    assert_eq!(
        policy.keys["test-key"],
        decode_hex::<32>(vector["public_key"].as_str().unwrap()).unwrap()
    );
    signed.verify(&policy, &resources).unwrap();
}

#[test]
fn signed_manifest_and_resources_roundtrip_with_deterministic_signature_input() {
    let (signed, policy, resources) = fixture();
    signed.verify(&policy, &resources).unwrap();
    let decoded = SignedManifest::decode(&serde_json::to_vec(&signed).unwrap()).unwrap();
    assert_eq!(decoded, signed);
    let reordered = serde_json::to_value(&signed).unwrap();
    let reordered = SignedManifest::decode(&serde_json::to_vec(&reordered).unwrap()).unwrap();
    assert_eq!(
        signed.manifest.signature_input().unwrap(),
        reordered.manifest.signature_input().unwrap()
    );
    reordered.verify(&policy, &resources).unwrap();
}

#[test]
fn rfc8032_empty_message_vector_uses_strict_ed25519_verification() {
    let key = ed25519_dalek::VerifyingKey::from_bytes(
        &decode_hex::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
            .unwrap(),
    )
    .unwrap();
    let signature = ed25519_dalek::Signature::from_bytes(
        &decode_hex::<64>(concat!(
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555f",
            "b8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
        ))
        .unwrap(),
    );
    key.verify_strict(b"", &signature).unwrap();
    assert!(key.verify_strict(b"tampered", &signature).is_err());
}

#[test]
fn tampering_revocation_rollback_and_permission_gates_fail_closed() {
    let (signed, mut policy, mut resources) = fixture();
    resources.get_mut("main.mjs").unwrap().push(b'!');
    assert_eq!(
        signed.verify(&policy, &resources),
        Err(PackageError::Integrity)
    );
    resources.get_mut("main.mjs").unwrap().pop();
    let mut tampered = signed.clone();
    tampered.manifest.version = "1.0.1".into();
    assert_eq!(
        tampered.verify(&policy, &resources),
        Err(PackageError::Signature)
    );
    policy.revoked_keys.insert("test-key".into());
    assert_eq!(
        signed.verify(&policy, &resources),
        Err(PackageError::RevokedKey)
    );
    policy.revoked_keys.clear();
    policy.minimum_release = 8;
    assert_eq!(
        signed.verify(&policy, &resources),
        Err(PackageError::Rollback)
    );
    policy.minimum_release = 7;
    policy.permissions.clear();
    assert_eq!(
        signed.verify(&policy, &resources),
        Err(PackageError::Permission)
    );
    policy.permissions.insert("counter.save".into());
    policy.package = "different.package".into();
    assert_eq!(
        signed.verify(&policy, &resources),
        Err(PackageError::Compatibility)
    );
}

#[test]
fn path_portability_size_limits_and_duplicate_manifest_fields_are_checked() {
    for invalid in [
        "../escape.js",
        "/root.js",
        "C:/x.js",
        "a\\x.js",
        "a//x.js",
        "a/%2e%2e/x.js",
        "CON.js",
        "a./x.js",
        "a/../x.js",
    ] {
        assert!(!safe_path(invalid), "{invalid}");
    }
    assert!(safe_path("assets/main-v1.mjs"));
    let (mut signed, _, _) = fixture();
    signed.manifest.resources.insert(
        "MAIN.mjs".into(),
        signed.manifest.resources["main.mjs"].clone(),
    );
    assert_eq!(signed.manifest.signature_input(), Err(PackageError::Path));
    signed.manifest.resources.remove("MAIN.mjs");
    signed.manifest.resources.get_mut("main.mjs").unwrap().bytes = 4 * 1024 * 1024 + 1;
    assert_eq!(signed.manifest.signature_input(), Err(PackageError::Limit));
    let (signed, _, _) = fixture();
    let json = serde_json::to_string(&signed)
        .unwrap()
        .replace("\"format\":1", "\"format\":1,\"format\":1");
    assert_eq!(
        SignedManifest::decode(json.as_bytes()),
        Err(PackageError::Format)
    );
}
