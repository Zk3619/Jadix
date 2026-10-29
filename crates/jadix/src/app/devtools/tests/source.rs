use super::*;

#[test]
fn source_save_preserves_disk_edits_and_replaces_complete_file() {
    let root = std::env::temp_dir().join(format!("jadix-source-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.rs");
    std::fs::write(&path, "fn main() {}\r\n").unwrap();
    let document = SourceDocument {
        path: path.clone(),
        text: "fn main() {}\r\n".into(),
    };
    let edited = "fn main() { println!(\"saved\"); }\r\n";
    write_document(&document, edited).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    assert!(write_document(&document, "stale draft")
        .unwrap_err()
        .contains("changed on disk"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn source_resolver_rejects_outside_workspace_and_non_rust_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    assert!(resolve_source(&root, "Cargo.toml").is_err());
    assert!(resolve_source(&root, "../jadix_macros/src/lib.rs").is_err());
    assert!(resolve_source(&root, "src/lib.rs").is_ok());
}
