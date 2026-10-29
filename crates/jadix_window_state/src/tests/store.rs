use super::*;
use std::path::PathBuf;

fn unique_file() -> PathBuf {
    std::env::temp_dir().join(format!(
        "jadix-ws-store-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn roundtrip_toml() {
    let path = unique_file();
    let mut doc = WindowStateFile::default();
    doc.windows.insert(
        "main".into(),
        WindowState {
            width: 1280,
            height: 800,
            x: 12,
            y: 34,
            prev_x: 12,
            prev_y: 34,
            maximized: false,
            visible: true,
            decorated: true,
            fullscreen: false,
        },
    );
    doc.save(&path).unwrap();
    let loaded = WindowStateFile::load(&path).unwrap();
    assert_eq!(loaded, doc);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("[windows.main]"));
    assert!(text.contains("width = 1280"));
    let _ = fs::remove_file(&path);
}

#[test]
fn missing_file_is_empty() {
    let path = unique_file();
    let doc = WindowStateFile::load(&path).unwrap();
    assert!(doc.windows.is_empty());
}

#[test]
fn corrupt_file_is_backed_up() {
    let path = unique_file();
    fs::write(&path, "not = [ toml").unwrap();
    let doc = WindowStateFile::load(&path).unwrap();
    assert!(doc.windows.is_empty());
    let bak = backup_path(&path);
    assert!(bak.exists());
    let _ = fs::remove_file(&bak);
    let _ = fs::remove_file(&path);
}
