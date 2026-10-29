use super::*;
#[test]
fn save_replaces_complete_file_and_rejects_stale_draft() {
    let directory = std::env::temp_dir().join(format!(
        "jadix-file-{}-{}",
        std::process::id(),
        NEXT_SAVE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("document.md");
    save_text_document(&path, "original", None).unwrap();
    save_text_document(&path, "updated\r\n中文", Some("original")).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "updated\r\n中文");
    assert!(save_text_document(&path, "stale", Some("original")).is_err());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}
