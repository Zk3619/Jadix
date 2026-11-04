#![cfg(all(feature = "native-store", not(target_arch = "wasm32")))]

use ed25519_dalek::{Signer, SigningKey};
use jadix_shell::{package::*, store::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

fn fixture(
    release: u64,
    source: &[u8],
) -> (SignedManifest, PackagePolicy, BTreeMap<String, Vec<u8>>) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let manifest = Manifest {
        format: PACKAGE_FORMAT,
        package: "native.store.test".into(),
        version: "1.0.0".into(),
        publisher: "jadix-development".into(),
        key_id: "test-key".into(),
        target: Target::Native,
        release: release.to_string(),
        protocol: jadix_shell::presentation::PROTOCOL_VERSION,
        sdk: SDK_VERSION.into(),
        minimum_host: "0.1.0".into(),
        component_schema: sha256(b"store-test-schema"),
        capabilities: BTreeSet::new(),
        entry: "panel.js".into(),
        resources: BTreeMap::from([(
            "panel.js".into(),
            Resource {
                sha256: sha256(source),
                bytes: source.len() as u64,
            },
        )]),
        permissions: BTreeMap::new(),
        storage_namespace: "native.store.test".into(),
        storage_version: 1,
        migration_from: BTreeSet::new(),
    };
    let signed = SignedManifest {
        signature: key
            .sign(&manifest.signature_input().unwrap())
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        manifest,
    };
    let policy = PackagePolicy {
        package: "native.store.test".into(),
        publisher: "jadix-development".into(),
        target: Target::Native,
        host_version: "0.1.0".into(),
        minimum_release: 1,
        component_schema: sha256(b"store-test-schema"),
        capabilities: BTreeSet::new(),
        permissions: BTreeSet::new(),
        keys: BTreeMap::from([("test-key".into(), key.verifying_key().to_bytes())]),
        revoked_keys: BTreeSet::new(),
    };
    (
        signed,
        policy,
        BTreeMap::from([("panel.js".into(), source.into())]),
    )
}

fn directory(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.theme-evidence/native/n17-20261104/store-tests");
    let directory = root.join(format!(
        "{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn install(store: &mut PackageStore, release: u64, source: &[u8]) {
    let (signed, policy, resources) = fixture(release, source);
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    assert_eq!(
        store.candidate(&activation, &policy).unwrap().resources["panel.js"],
        source
    );
    store.accept(&activation, &policy).unwrap();
    assert!(matches!(
        store.accept(&activation, &policy),
        Err(StoreError::StaleActivation)
    ));
}

fn business_fixture(
    release: u64,
    version: u32,
    from: &[u32],
    namespace: &str,
) -> (SignedManifest, PackagePolicy, BTreeMap<String, Vec<u8>>) {
    let (mut signed, policy, resources) = fixture(release, b"business");
    signed.manifest.storage_version = version;
    signed.manifest.migration_from = from.iter().copied().collect();
    signed.manifest.storage_namespace = namespace.into();
    signed.signature = SigningKey::from_bytes(&[7; 32])
        .sign(&signed.manifest.signature_input().unwrap())
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (signed, policy, resources)
}

#[test]
fn business_journal_replays_after_failed_write_and_process_exit_and_rejects_corruption() {
    const CHILD: &str = "JADIX_BUSINESS_JOURNAL_CHILD";
    let latest = serde_json::json!({"draft":"日志恢复中文😀", "count":23});
    let (_, policy, _) = fixture(1, b"journal");
    if let Some(path) = std::env::var_os(CHILD) {
        let path = PathBuf::from(path);
        let mut store = PackageStore::open(&path, &policy).unwrap();
        install(&mut store, 1, b"journal");
        store
            .save_business(&policy, &serde_json::json!({"draft":"old"}))
            .unwrap();
        let package = store.current(&policy).unwrap().unwrap();
        let checkpoint = path
            .join("checkpoints")
            .join(format!("{}.json", package.identity));
        std::fs::remove_file(&checkpoint).unwrap();
        std::fs::create_dir(&checkpoint).unwrap();
        assert!(matches!(
            store.save_business(&policy, &latest),
            Err(StoreError::Format)
        ));
        assert!(path.join("business-journal.json").is_file());
        assert!(matches!(store.collect_unused(), Err(StoreError::Pending)));
        std::process::exit(75);
    }
    let path = directory("business-journal-process");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "business_journal_replays_after_failed_write_and_process_exit_and_rejects_corruption",
            "--nocapture",
        ])
        .env(CHILD, &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(75));
    let journal_path = path.join("business-journal.json");
    let bytes = std::fs::read(&journal_path).unwrap();
    assert!(bytes.len() < 256 * 1024 + 1024);
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let identity = original["checkpoint"]["identity"].as_str().unwrap();
    let checkpoint = path.join("checkpoints").join(format!("{identity}.json"));
    std::fs::remove_dir(&checkpoint).unwrap();
    std::fs::write(&checkpoint, b"interrupted checkpoint{").unwrap();
    for variant in 0..5 {
        let mut invalid = original.clone();
        match variant {
            0 => invalid["checksum"] = serde_json::json!("0".repeat(64)),
            1 => invalid["revision"] = serde_json::json!(999),
            2 => invalid["checkpoint"]["namespace"] = serde_json::json!("foreign.namespace"),
            3 => invalid["checkpoint"]["version"] = serde_json::json!(99),
            _ => invalid["checkpoint"]["identity"] = serde_json::json!("0".repeat(64)),
        }
        if variant != 0 {
            #[derive(serde::Serialize)]
            struct Checkpoint<'a> {
                schema: &'a serde_json::Value,
                identity: &'a serde_json::Value,
                namespace: &'a serde_json::Value,
                version: &'a serde_json::Value,
                value: &'a serde_json::Value,
            }
            let state = &invalid["checkpoint"];
            let checkpoint = Checkpoint {
                schema: &state["schema"],
                identity: &state["identity"],
                namespace: &state["namespace"],
                version: &state["version"],
                value: &state["value"],
            };
            let encoded =
                serde_json::to_vec(&(&invalid["schema"], &invalid["revision"], checkpoint))
                    .unwrap();
            invalid["checksum"] = serde_json::json!(sha256(&encoded));
        }
        std::fs::write(&journal_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(matches!(
            PackageStore::open(&path, &policy),
            Err(StoreError::Format)
        ));
        assert!(journal_path.exists());
        assert_eq!(
            std::fs::read(&checkpoint).unwrap(),
            b"interrupted checkpoint{"
        );
    }
    std::fs::write(&journal_path, vec![b' '; 256 * 1024 + 1025]).unwrap();
    assert!(matches!(
        PackageStore::open(&path, &policy),
        Err(StoreError::Format)
    ));
    std::fs::write(&journal_path, bytes).unwrap();
    let store = PackageStore::open(&path, &policy).unwrap();
    assert!(store.recovered_business());
    assert_eq!(store.business(&policy).unwrap(), Some(latest.clone()));
    assert!(!journal_path.exists());
    assert_eq!(store.highest_release(), 1);
    drop(store);
    let store = PackageStore::open(&path, &policy).unwrap();
    assert!(!store.recovered_business());
    assert_eq!(store.business(&policy).unwrap(), Some(latest));
}

#[test]
fn business_journal_recovery_keeps_failed_upgrade_inactive_and_replays_before_pending_cleanup() {
    let path = directory("business-journal-upgrade");
    let (_, policy, _) = fixture(1, b"stable");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    install(&mut store, 1, b"stable");
    let original = serde_json::json!({"draft":"stable 中文😀"});
    store.save_business(&policy, &original).unwrap();
    let (signed, _, resources) = business_fixture(2, 2, &[1], "native.store.test");
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    let candidate = store.candidate(&activation, &policy).unwrap();
    let checkpoint = path
        .join("checkpoints")
        .join(format!("{}.json", candidate.identity));
    std::fs::create_dir(&checkpoint).unwrap();
    assert!(matches!(
        store.accept_with_business(
            &activation,
            &policy,
            &serde_json::json!({"draft":"migrated", "schema":2})
        ),
        Err(StoreError::Format)
    ));
    assert_eq!(store.business(&policy).unwrap(), Some(original.clone()));
    assert!(matches!(
        store.reject(&activation),
        Err(StoreError::Pending)
    ));
    assert!(matches!(store.collect_unused(), Err(StoreError::Pending)));
    assert_eq!(store.highest_release(), 1);
    drop(store);
    std::fs::remove_dir(&checkpoint).unwrap();
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(store.recovered_business());
    assert!(store.recovered_pending());
    assert_eq!(store.highest_release(), 1);
    assert_eq!(store.business(&policy).unwrap(), Some(original));
    assert!(store.previous(&policy).unwrap().is_none());
    assert_eq!(store.collect_unused().unwrap(), 1);
    assert!(!checkpoint.exists());
}

#[test]
fn business_migration_failed_activation_restart_and_rollback_keep_versioned_checkpoints() {
    let path = directory("business");
    let (_, policy, _) = fixture(1, b"first");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(matches!(
        store.save_business(&policy, &serde_json::Value::Null),
        Err(StoreError::NoCurrent)
    ));
    install(&mut store, 1, b"first");
    let original = serde_json::json!({"draft":"中文😀", "count":7});
    store.save_business(&policy, &original).unwrap();
    let (signed, _, resources) = business_fixture(2, 2, &[1], "native.store.test");
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    assert!(matches!(
        store.candidate_business(&activation, &policy, |_, _, _| Err(StoreError::Migration)),
        Err(StoreError::Migration)
    ));
    let migrated = store
        .candidate_business(&activation, &policy, |from, to, value| {
            assert_eq!((from, to), (1, 2));
            Ok(serde_json::json!({"profile":{"name":value["draft"]}, "count":value["count"]}))
        })
        .unwrap()
        .unwrap();
    assert_eq!(store.business(&policy).unwrap(), Some(original.clone()));
    assert!(matches!(
        store.accept_with_business(
            &activation,
            &policy,
            &serde_json::json!({"oversize":"x".repeat(256*1024)})
        ),
        Err(StoreError::Limit)
    ));
    std::fs::rename(
        path.join("activation.json"),
        path.join("activation-backup.json"),
    )
    .unwrap();
    std::fs::create_dir(path.join("activation.json")).unwrap();
    assert!(matches!(
        store.accept_with_business(&activation, &policy, &migrated),
        Err(StoreError::Io(_))
    ));
    assert_eq!(store.business(&policy).unwrap(), Some(original.clone()));
    std::fs::remove_dir(path.join("activation.json")).unwrap();
    std::fs::rename(
        path.join("activation-backup.json"),
        path.join("activation.json"),
    )
    .unwrap();
    drop(store);
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(store.recovered_pending());
    assert_eq!(store.business(&policy).unwrap(), Some(original.clone()));
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    store
        .accept_with_business(&activation, &policy, &migrated)
        .unwrap();
    assert_eq!(store.business(&policy).unwrap(), Some(migrated.clone()));
    assert_eq!(
        store.previous_business(&policy).unwrap(),
        Some(original.clone())
    );
    let updated = serde_json::json!({"profile":{"name":"new 中文"}, "count":8});
    store.save_business(&policy, &updated).unwrap();
    store.rollback(&policy).unwrap();
    assert_eq!(store.business(&policy).unwrap(), Some(original.clone()));
    assert_eq!(store.previous_business(&policy).unwrap(), Some(updated));
    assert_eq!(store.highest_release(), 2);
    drop(store);
    let store = PackageStore::open(&path, &policy).unwrap();
    assert_eq!(store.business(&policy).unwrap(), Some(original));
}

#[test]
fn business_requires_signed_migration_namespace_and_valid_bounded_checkpoint() {
    let path = directory("business-policy");
    let (_, policy, _) = fixture(1, b"first");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    install(&mut store, 1, b"first");
    let original = serde_json::json!({"draft":"safe"});
    store.save_business(&policy, &original).unwrap();
    for (version, from, namespace) in [
        (2, vec![], "native.store.test"),
        (2, vec![1], "other.namespace"),
    ] {
        let (signed, _, resources) = business_fixture(2, version, &from, namespace);
        let activation = store.stage(&signed, &resources, &policy).unwrap();
        assert!(matches!(
            store.candidate_business(&activation, &policy, |_, _, _| panic!(
                "unauthorized migration invoked"
            )),
            Err(StoreError::Migration)
        ));
        store.reject(&activation).unwrap();
    }
    let (signed, _, resources) = business_fixture(2, 1, &[], "native.store.test");
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    assert_eq!(
        store
            .candidate_business(&activation, &policy, |_, _, _| panic!(
                "same schema invoked migration"
            ))
            .unwrap(),
        Some(original)
    );
    store.reject(&activation).unwrap();
    let current = store.current(&policy).unwrap().unwrap();
    let checkpoint = path
        .join("checkpoints")
        .join(format!("{}.json", current.identity));
    let bytes = std::fs::read(&checkpoint).unwrap();
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    invalid["version"] = serde_json::json!(99);
    std::fs::write(&checkpoint, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(matches!(store.business(&policy), Err(StoreError::Format)));
    std::fs::write(&checkpoint, vec![b' '; 256 * 1024 + 1]).unwrap();
    assert!(matches!(store.business(&policy), Err(StoreError::Format)));
    std::fs::write(&checkpoint, bytes).unwrap();
    assert!(store.business(&policy).unwrap().is_some());
    assert_eq!(store.collect_unused().unwrap(), 3);
}

#[test]
fn healthy_activation_restart_reverification_rollback_and_monotonic_release_floor() {
    let path = directory("restart");
    let (_, policy, _) = fixture(1, b"first");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(store.current(&policy).unwrap().is_none());
    install(&mut store, 1, b"first");
    install(&mut store, 2, b"second");
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"second"
    );
    assert_eq!(
        store.rollback(&policy).unwrap().resources["panel.js"],
        b"first"
    );
    assert_eq!(store.highest_release(), 2);
    let (signed, _, resources) = fixture(1, b"first");
    assert!(matches!(
        store.stage(&signed, &resources, &policy),
        Err(StoreError::Package(PackageError::Rollback))
    ));
    let (signed, _, resources) = fixture(2, b"conflicting-release");
    assert!(matches!(
        store.stage(&signed, &resources, &policy),
        Err(StoreError::ReleaseConflict)
    ));
    drop(store);
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(!store.recovered_pending());
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"first"
    );
    assert_eq!(store.highest_release(), 2);
    assert_eq!(
        store.rollback(&policy).unwrap().resources["panel.js"],
        b"second"
    );
}

#[test]
fn lock_pending_rejection_foreign_tokens_revocation_and_corruption_fail_closed() {
    let path = directory("failure");
    let (signed, mut policy, resources) = fixture(1, b"stable");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(matches!(
        PackageStore::open(&path, &policy),
        Err(StoreError::Locked)
    ));
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    assert!(matches!(
        store.stage(&signed, &resources, &policy),
        Err(StoreError::Pending)
    ));
    let mut other = PackageStore::open(directory("other"), &policy).unwrap();
    assert!(matches!(
        other.reject(&activation),
        Err(StoreError::StaleActivation)
    ));
    policy.revoked_keys.insert("test-key".into());
    assert!(matches!(
        store.accept(&activation, &policy),
        Err(StoreError::Package(PackageError::RevokedKey))
    ));
    store.reject(&activation).unwrap();
    assert!(store.current(&policy).unwrap().is_none());
    policy.revoked_keys.clear();
    install(&mut store, 1, b"stable");
    let current = store.current(&policy).unwrap().unwrap();
    let resource = path
        .join("objects")
        .join(current.identity)
        .join(format!("{}.bin", sha256(b"stable")));
    std::fs::write(resource, b"forged").unwrap();
    assert!(matches!(
        store.current(&policy),
        Err(StoreError::Package(PackageError::Integrity))
    ));
    drop(store);
    assert!(matches!(
        PackageStore::open(&path, &policy),
        Err(StoreError::Package(PackageError::Integrity))
    ));
}

#[test]
fn abandoned_candidate_recovers_stable_version_and_gc_preserves_active_objects() {
    let path = directory("abandoned");
    let (_, policy, _) = fixture(1, b"stable");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    install(&mut store, 1, b"stable");
    let (signed, _, resources) = fixture(2, b"candidate");
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    drop(store);
    let mut store = PackageStore::open(&path, &policy).unwrap();
    assert!(store.recovered_pending());
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"stable"
    );
    assert!(matches!(
        store.accept(&activation, &policy),
        Err(StoreError::StaleActivation)
    ));
    assert_eq!(store.collect_unused().unwrap(), 1);
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"stable"
    );
}

#[test]
fn process_exit_with_pending_activation_releases_os_lock_and_recovers_previous_version() {
    let path = directory("process-death");
    let (_, policy, _) = fixture(1, b"stable");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    install(&mut store, 1, b"stable");
    drop(store);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_fixture_child", "--nocapture"])
        .env("JADIX_STORE_CRASH_DIRECTORY", &path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("store crash fixture timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(73));
    let store = PackageStore::open(&path, &policy).unwrap();
    assert!(store.recovered_pending());
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"stable"
    );
}

#[test]
fn crash_fixture_child() {
    let Some(path) = std::env::var_os("JADIX_STORE_CRASH_DIRECTORY") else {
        return;
    };
    let (signed, policy, resources) = fixture(2, b"unconfirmed candidate");
    let mut store = PackageStore::open(path, &policy).unwrap();
    store.stage(&signed, &resources, &policy).unwrap();
    std::process::exit(73);
}

#[test]
fn eight_object_budget_requires_collection_and_releases_unconfirmed_packages() {
    let path = directory("budget");
    let (_, policy, _) = fixture(1, b"stable");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    install(&mut store, 1, b"stable");
    for release in 2..=8 {
        let (signed, _, resources) = fixture(release, format!("candidate-{release}").as_bytes());
        let activation = store.stage(&signed, &resources, &policy).unwrap();
        store.reject(&activation).unwrap();
    }
    let (signed, _, resources) = fixture(9, b"candidate-9");
    assert!(matches!(
        store.stage(&signed, &resources, &policy),
        Err(StoreError::Limit)
    ));
    assert_eq!(store.collect_unused().unwrap(), 7);
    let activation = store.stage(&signed, &resources, &policy).unwrap();
    assert_eq!(store.collect_unused().unwrap(), 0);
    store.accept(&activation, &policy).unwrap();
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"candidate-9"
    );
}

#[test]
fn invalid_state_and_failed_pointer_replacement_leave_the_current_in_memory_version() {
    let path = directory("pointer");
    let (_, policy, _) = fixture(1, b"stable");
    let mut store = PackageStore::open(&path, &policy).unwrap();
    install(&mut store, 1, b"stable");
    std::fs::rename(
        path.join("activation.json"),
        path.join("activation-backup.json"),
    )
    .unwrap();
    std::fs::create_dir(path.join("activation.json")).unwrap();
    let (signed, _, resources) = fixture(2, b"candidate");
    assert!(matches!(
        store.stage(&signed, &resources, &policy),
        Err(StoreError::Io(_))
    ));
    assert_eq!(
        store.current(&policy).unwrap().unwrap().resources["panel.js"],
        b"stable"
    );
    assert!(!std::fs::read_dir(&path).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")
    }));
    drop(store);
    assert!(matches!(
        PackageStore::open(&path, &policy),
        Err(StoreError::Format)
    ));
}
