use super::*;

#[test]
fn rejected_manual_draft_does_not_replace_the_compiled_source_baseline() {
    let root = std::env::temp_dir().join(format!(
        "jadix-rejected-draft-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let file = root.join("source.rs");
    let original = r#"fn view(){ui!{<Box w=96.0>"old"</Box>}}"#;
    std::fs::write(&file, original).unwrap();
    let mut supervisor = Supervisor {
        command: Vec::new(),
        root: root.clone(),
        child: None,
        sources: HashMap::from([(file.clone(), file_signature(&file))]),
    };
    std::fs::write(&file, original.replace("w=96.0", "w={captured_width()}")).unwrap();
    assert!(supervisor.needs_restart(&[file.clone()]));
    assert!(!supervisor.restart_if_managed().unwrap());
    assert!(
        supervisor.needs_restart(&[file.clone()]),
        "an unchanged rejected draft still differs from compiled Rust"
    );
    std::fs::write(
        &file,
        original.replace("old", "repaired").replace("96.0", "128.0"),
    )
    .unwrap();
    assert!(
        !supervisor.needs_restart(&[file.clone()]),
        "restoring compiled Rust slots must allow a live patch"
    );
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn static_text_style_and_animation_use_live_patch_but_logic_rebuilds() {
    let old =
        r#"fn view() { let count = 1; ui! { <Box p="4" animation="spin 1s linear">"old"</Box> } }"#;
    let live = old
        .replace("old", "new")
        .replace(r#"p="4""#, r#"p="8""#)
        .replace("1s linear", "2s linear");
    assert_eq!(rust_signature(old), rust_signature(&live));
    assert_ne!(
        rust_signature(old),
        rust_signature(&old.replace("count = 1", "count = 2"))
    );
    assert_ne!(
        rust_signature(old),
        rust_signature(&old.replace("Box", "Button"))
    );
}

#[test]
fn callback_dynamic_props_and_non_style_literals_require_rebuild() {
    for old in [
        r#"fn view() { ui! { <Button variant="ghost">"hello"</Button> } }"#,
        r#"fn view() { ui! { <Box w={format!("100px")}>"hello"</Box> } }"#,
    ] {
        let next = old
            .replace("bg-red-500 ", "")
            .replace("ghost", "outline")
            .replace("100px", "200px");
        assert_ne!(rust_signature(old), rust_signature(&next));
    }
}
