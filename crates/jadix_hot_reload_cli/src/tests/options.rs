use super::*;

#[test]
fn toml_paths_are_relative_to_config_and_cli_overrides_values() {
    let dir = std::env::temp_dir().join(format!("jadix-watch-config-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src/generated")).unwrap();
    let config = dir.join("dev.toml");
    std::fs::write(&config, "[hot-reload]\nwatch = ['src']\nexclude = ['src/generated']\naddr = '127.0.0.1:9900'\nrun = ['cargo', 'run']\n").unwrap();
    let options = Options::parse(
        [
            "--config".into(),
            config.to_string_lossy().into_owned(),
            "--addr".into(),
            "127.0.0.1:9901".into(),
        ]
        .into_iter(),
    )
    .unwrap();
    assert_eq!(options.addr, "127.0.0.1:9901");
    assert_eq!(options.watch, [options.root.join("src")]);
    assert!(options.tracks(&options.root.join("src/lib.rs")));
    assert!(!options.tracks(&options.root.join("src/generated/lib.rs")));
    assert_eq!(options.run, ["cargo", "run"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_toml_is_reported_with_path() {
    let path =
        std::env::temp_dir().join(format!("jadix-invalid-config-{}.toml", std::process::id()));
    std::fs::write(&path, "[hot-reload]\nwatc = ['src']").unwrap();
    let error =
        Options::parse(["--config".into(), path.to_string_lossy().into_owned()].into_iter())
            .err()
            .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("watc"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn explicit_watch_and_exclusion_restrict_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let options = Options::parse(
        [
            "--root",
            root.to_str().unwrap(),
            "--watch",
            "crates",
            "--exclude",
            "crates/jadix/src/app",
            "--run",
            "--",
            "cargo",
            "run",
            "-p",
            "gallery",
        ]
        .into_iter()
        .map(str::to_string),
    )
    .unwrap();
    assert!(options.tracks(&options.root.join("crates/jadix/src/lib.rs")));
    assert!(!options.tracks(&options.root.join("crates/jadix/src/app/state.rs")));
    assert!(!options.tracks(&options.root.join("examples/gallery/src/main.rs")));
    assert_eq!(options.run, ["cargo", "run", "-p", "gallery"]);
}
