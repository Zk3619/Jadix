use jadix_native::checkpoint::CheckpointStore;
use std::{fs, path::PathBuf};

fn directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "jadix-checkpoint-{}-{}",
        std::process::id(),
        jadix_native::presentation::RootId::new().token()
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn checkpoint_replacement_conflict_and_corruption_preserve_saved_data() {
    let directory = directory();
    let path = directory.join("request.json");
    let store = CheckpointStore::new(&path, 1024).unwrap();
    assert!(store.read().unwrap().is_none());
    assert_eq!(store.save(None, "first"), Ok(1));
    assert!(store.save(None, "lost update").is_err());
    assert_eq!(store.save(Some(1), "second\n\0😀"), Ok(2));
    assert_eq!(store.read().unwrap().unwrap().payload, "second\n\0😀");
    let saved = fs::read(&path).unwrap();
    assert!(store.save(Some(2), &"x".repeat(1025)).is_err());
    assert_eq!(fs::read(&path).unwrap(), saved);
    let mut altered: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    altered["payload"] = serde_json::json!("tampered payload");
    let altered = serde_json::to_vec(&altered).unwrap();
    fs::write(&path, &altered).unwrap();
    assert!(store.read().is_err());
    assert!(store.save(Some(2), "replacement").is_err());
    assert_eq!(fs::read(&path).unwrap(), altered);
    fs::write(&path, b"invalid original").unwrap();
    assert!(store.save(Some(2), "replacement").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"invalid original");
    assert!(!directory.join("request.json.lock").exists());
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn checkpoint_lock_and_directory_are_rejected_without_overwrite() {
    let directory = directory();
    let path = directory.join("request.json");
    let lock = directory.join("request.json.lock");
    fs::write(&lock, "external owner").unwrap();
    let store = CheckpointStore::new(&path, 128).unwrap();
    assert!(store.save(None, "draft").is_err());
    assert_eq!(fs::read_to_string(&lock).unwrap(), "external owner");
    fs::remove_file(lock).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(store.save(None, "draft").is_err());
    assert!(path.is_dir());
    fs::remove_dir(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
