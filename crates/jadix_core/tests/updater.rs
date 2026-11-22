#![cfg(feature = "updater")]
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use jadix_core::updater::*;
use std::collections::BTreeMap;

fn config() -> (Config, SigningKey) {
    let key = SigningKey::from_bytes(&[7; 32]);
    (Config { id: "com.example.app".into(), version: "1.2.0".into(), github: None,
        url: Some("https://example.test/releases/latest.json?channel=stable".into()),
        channels: BTreeMap::from([("preview".into(), "https://example.test/preview/latest.json".into())]),
        public_key: Some(B64.encode(key.verifying_key().to_bytes())) }, key)
}
fn manifest(version: &str, archive: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"id":"com.example.newapp", "version":version, "notes":"Default notes",
        "localizedNotes":{"zh":{"url":"../notes/zh.md", "sha256":sha256("修复".as_bytes())}},
        "platforms":{"macos-aarch64":{"url":"./app.zip", "sha256":sha256(archive), "size":archive.len()}}})).unwrap()
}
fn signature(key: &SigningKey, bytes: &[u8]) -> String {
    let mut signed = SIGNATURE_DOMAIN.to_vec(); signed.extend_from_slice(bytes); B64.encode(key.sign(&signed).to_bytes())
}
fn checked(service: &Updater, key: &SigningKey, archive: &[u8]) -> u32 {
    service.check(None, Some("zh-CN")).unwrap(); let id = service.snapshot().generation;
    let bytes = manifest("2.0.0", archive); assert!(service.event(id, Event::Manifest { signature: Some(signature(key, &bytes)), bytes })); id
}
#[test]
fn signed_manifest_channels_versions_and_relative_urls() {
    let (config, key) = config(); let url = config.manifest_url(None).unwrap();
    let bytes = manifest("2.0.0", b"zip"); let sig = signature(&key, &bytes);
    let available = config.check(&url, &bytes, Some(&sig), "macos-aarch64", true).unwrap().unwrap();
    assert_eq!(available.url, "https://example.test/releases/app.zip");
    assert_eq!(notes_for(&available.localized_notes, "ZH_cn").unwrap().url, "https://example.test/notes/zh.md");
    assert_eq!(available.id, "com.example.newapp");
    assert!(config.manifest_url(Some("missing")).is_err());
    assert!(config.check(&url, &bytes, None, "macos-aarch64", true).is_err());
    assert!(config.check(&url, &bytes, Some(&B64.encode([0;64])), "macos-aarch64", true).is_err());
    let bytes = manifest("1.2.0+new-build", b"zip");
    // Build metadata has no precedence under SemVer.
    assert!(config.check(&url, &bytes, Some(&signature(&key, &bytes)), "macos-aarch64", true).unwrap().is_none());
    let bytes = manifest("1.2.0-beta.1", b"zip");
    assert!(config.check(&url, &bytes, Some(&signature(&key, &bytes)), "macos-aarch64", true).unwrap().is_none());
}
#[test]
fn trust_limits_and_signed_bad_assets_fail_closed() {
    let (mut config, key) = config(); config.public_key = None; assert!(config.validate().is_err());
    let (mut config, _) = self::config(); config.url = Some("https://user:secret@example.test/latest.json".into()); assert!(config.validate().is_err());
    let (config, _) = self::config(); let url = config.manifest_url(None).unwrap();
    for asset in [serde_json::json!({"url":"http://example.test/app.zip","sha256":sha256(b"a")}),
        serde_json::json!({"url":"app.zip","sha256":"ab"}),
        serde_json::json!({"url":"app.zip","sha256":sha256(b"a"),"size":DOWNLOAD_LIMIT+1})] {
        let bytes = serde_json::to_vec(&serde_json::json!({"id":"example", "version":"2.0.0", "platforms":{"macos-aarch64":asset}})).unwrap();
        assert!(config.check(&url, &bytes, Some(&signature(&key,&bytes)), "macos-aarch64", true).is_err());
    }
}
#[test]
fn streamed_verification_poisoning_and_short_download() {
    let mut download = Download::new(&sha256(b"ab"), Some(2)).unwrap();
    assert_eq!(download.update(b"a").unwrap(), 1); assert_eq!(download.update(b"b").unwrap(), 2); download.finish().unwrap();
    let mut download = Download::new(&sha256(b"a"), Some(1)).unwrap(); assert!(download.update(b"ab").is_err()); assert!(download.update(b"a").is_err()); assert!(download.finish().is_err());
    let mut download = Download::new(&sha256(b"ab"), Some(2)).unwrap(); download.update(b"a").unwrap(); assert!(download.finish().is_err());
    let mut download = Download::new(&sha256(b"ab"), Some(2)).unwrap(); download.update(b"aa").unwrap(); assert!(download.finish().is_err());
}
#[test]
fn shared_lifecycle_localized_fallback_stage_and_restart_override() {
    let (config, key) = config(); let service = Updater::new(Some(config), "macos-aarch64", true).unwrap();
    let id = checked(&service, &key, b"archive"); assert_eq!(service.snapshot().phase, Phase::Notes);
    service.event(id, Event::Notes { bytes: b"tampered".to_vec() }); assert_eq!(service.snapshot().available.unwrap().notes.as_deref(), Some("Default notes"));
    service.take_commands(); service.download().unwrap(); let id = service.snapshot().generation;
    assert!(service.chunk(id,b"arc")); assert!(service.chunk(id,b"hive")); service.event(id, Event::Downloaded);
    assert_eq!(service.snapshot().phase, Phase::Staging); assert!(service.install(false).is_err());
    assert!(service.take_commands().iter().any(|c| matches!(c, Command::Stage { .. })));
    service.event(id, Event::Staged); service.install(false).unwrap(); service.event(id, Event::Scheduled);
    assert_eq!(service.snapshot().phase, Phase::Scheduled); service.install(true).unwrap();
    assert!(matches!(service.take_commands().last(), Some(Command::Install {restart:true,..})));
    service.cancel().unwrap(); assert!(!service.chunk(id,b"late")); assert!(!service.event(id, Event::Staged));
}
#[test]
fn retries_ignore_previous_download_and_failed_notes_keep_candidate() {
    let (config,key)=config(); let service=Updater::new(Some(config),"macos-aarch64",true).unwrap();
    let id=checked(&service,&key,b"archive"); service.event(id,Event::Failed{message:"offline notes".into()});
    assert_eq!(service.snapshot().phase,Phase::Available); service.download().unwrap(); let old=service.snapshot().generation;
    service.chunk(old,b"bad"); service.event(old,Event::Downloaded); assert_eq!(service.snapshot().phase,Phase::Failed);
    service.download().unwrap(); let current=service.snapshot().generation; assert_ne!(old,current);
    assert!(!service.chunk(old,b"old")); assert!(!service.event(old,Event::Failed{message:"late".into()}));
    assert!(service.chunk(current,b"archive")); service.event(current,Event::Downloaded); assert_eq!(service.snapshot().phase,Phase::Staging);
}

#[test]
fn release_signatures_are_consumable_and_all_targets_are_validated() {
    let (config, _) = config(); let url = config.manifest_url(None).unwrap();
    let bytes = manifest("2.0.0", b"archive");
    let (public, signature) = sign_release(&[7; 32], url.as_str(), &bytes).unwrap();
    assert_eq!(Some(public), config.public_key);
    assert!(config.check(&url, &bytes, Some(&signature), "macos-aarch64", true).unwrap().is_some());
    let mut modified = bytes.clone(); modified.push(b' ');
    assert!(config.check(&url, &modified, Some(&signature), "macos-aarch64", true).is_err());
    let mut release: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    release["platforms"]["macos-x86_64"] = serde_json::json!({"url":"https://example.test/bad.zip"});
    assert!(validate_release(url.as_str(), &serde_json::to_vec(&release).unwrap()).is_err());
    release["platforms"] = serde_json::json!({"ios":{"url":"https://apps.apple.com/app/example"}});
    let bytes = serde_json::to_vec(&release).unwrap();
    validate_release(url.as_str(), &bytes).unwrap();
    let (_, signature) = sign_release(&[7; 32], url.as_str(), &bytes).unwrap();
    assert!(!config.check(&url, &bytes, Some(&signature), "ios", true).unwrap().unwrap().can_install);
    release["platforms"] = serde_json::json!({});
    assert!(validate_release(url.as_str(), &serde_json::to_vec(&release).unwrap()).is_err());
}

#[test]
fn command_waker_can_reenter_the_service_without_a_borrow_panic() {
    use std::{cell::Cell, rc::Rc};
    let (config, _) = config();
    let service = Updater::new(Some(config), "linux-x86_64", true).unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let reentrant = service.clone();
    service.set_command_waker(Some(Rc::new(move || {
        let _ = reentrant.snapshot();
        assert!(!reentrant.take_commands().is_empty());
        observed.set(observed.get() + 1);
    })));
    service.check(None, None).unwrap();
    service.cancel().unwrap();
    assert_eq!(calls.get(), 2);
    service.set_command_waker(None);
}
