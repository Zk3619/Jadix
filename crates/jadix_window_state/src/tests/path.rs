use super::*;
use std::fs;

fn unique_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jadix-ws-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn explicit_file_ignores_env() {
    let path = PathBuf::from("/tmp/explicit.toml");
    let resolved = resolve_storage_path(
        &StorageLocation::File(path.clone()),
        "ignored.toml",
        Some("/tmp/from-env.toml"),
    )
    .unwrap();
    assert_eq!(resolved, path);
}

#[test]
fn directory_joins_filename() {
    let dir = PathBuf::from("/var/data");
    let resolved = resolve_storage_path(&StorageLocation::Directory(dir), "w.toml", None).unwrap();
    assert_eq!(resolved, PathBuf::from("/var/data/w.toml"));
}

#[test]
fn env_dir_override_joins_filename() {
    let dir = unique_dir();
    let resolved = resolve_storage_path(
        &StorageLocation::CurrentDir,
        "w.toml",
        Some(dir.to_str().unwrap()),
    )
    .unwrap();
    assert_eq!(resolved, dir.join("w.toml"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn env_file_override_used_as_file() {
    let dir = unique_dir();
    let file = dir.join("custom.toml");
    fs::write(&file, "").unwrap();
    let resolved = resolve_storage_path(
        &StorageLocation::CurrentDir,
        "w.toml",
        Some(file.to_str().unwrap()),
    )
    .unwrap();
    assert_eq!(resolved, file);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn empty_filename_errors() {
    let err = resolve_storage_path(&StorageLocation::CurrentDir, "  ", None).unwrap_err();
    assert!(matches!(err, Error::Path(_)));
}
