use super::*;

#[test]
fn workspace_sources_are_not_limited_to_crates_and_examples() {
    let root = Path::new("workspace");
    for name in ["src/main.rs", "ui/widgets.rs", "shaders/app.wgsl"] {
        assert!(is_tracked_project_file(&root.join(name), root), "{name}");
    }
    assert!(!is_tracked_project_file(
        &root.join("target/generated.rs"),
        root
    ));
}
use std::path::Path;

#[test]
fn gallery_button_template_extracts_text_replacement() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery/src/pages/basics.rs");
    let key = normalize_path_for_id(&file);
    let previous = collect_templates_for_file(&file, &key);
    let source = fs::read_to_string(&file).unwrap().replace("\r\n", "\n");
    let changed = source.replacen(
        "\"Secondary\"\n                    </Button>",
        "\"HMR VERIFIED\"\n                    </Button>",
        1,
    );
    assert_ne!(
        changed, source,
        "Gallery Button fixture must contain the target label"
    );
    let templates = extract_ui_templates(&changed);
    let block = templates
        .iter()
        .enumerate()
        .find(|(_, block)| block.body.contains("HMR VERIFIED"))
        .unwrap();
    let mut current = TemplateFragment {
        id: format!("{}:{}:{}", key, block.1.line, block.1.column),
        hash: stable_hash(&block.1.body),
        len: block.1.body.len(),
        signature: jadix_hot_reload::syntax::template_signature(&block.1.body).unwrap_or_default(),
        declarations_signature: jadix_hot_reload::syntax::declarations_signature(&block.1.body)
            .unwrap_or_default(),
        tree: jadix_hot_reload::syntax::template_tree(&block.1.body),
        index: block.0,
        text_literals: rsx_text_order::extract_text_literals_tree_order(&block.1.body),
        text_replacements: vec![],
        style_attrs: vec![],
    };
    apply_text_replacements_single(&previous, &mut current);
    assert!(
        current
            .text_replacements
            .iter()
            .any(|change| change.from == "Secondary" && change.to == "HMR VERIFIED"),
        "{:?}",
        current.text_literals
    );
}

#[test]
fn strip_prefix_ci_ignores_drive_letter_case() {
    assert_eq!(
        strip_prefix_ci(
            "d:/projects/jadix/examples/gallery/src/main.rs",
            "D:/projects/jadix"
        ),
        Some("/examples/gallery/src/main.rs")
    );
    assert_eq!(
        strip_prefix_ci("d:/projects/jadix-extra/src/main.rs", "d:/projects/jadix"),
        None
    );
}

#[test]
fn extract_ui_templates_records_char_column() {
    let src = "    ui! { \"hi\" }\n";
    let blocks = extract_ui_templates(src);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].line, 1);
    assert_eq!(blocks[0].column, 5);
    assert!(blocks[0].body.contains("hi"));
}

#[test]
fn extraction_ignores_comments_strings_and_literal_braces() {
    let source = r##"// ui! { fake }
fn demo() {
    let ignored = "ui! { fake }";
    let ignored_raw = r#"ui! { fake }"#;
    ui! { <Box><Label> "}" </Label><Label> "ui! { literal }" </Label></Box> }
    ui! { <Box class="p-2" /> }
}"##;
    let blocks = extract_ui_templates(source);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].line, 5);
    assert_eq!(
        blocks[0].body,
        r#"<Box><Label> "}" </Label><Label> "ui! { literal }" </Label></Box>"#
    );
    assert_eq!(blocks[1].line, 6);
    assert_eq!(
        rsx_text_order::extract_text_literals_tree_order(&blocks[0].body),
        vec!["}", "ui! { literal }"]
    );
}

#[test]
fn extraction_preserves_unicode_columns_and_unspaced_tags() {
    let blocks =
        extract_ui_templates("fn demo() { let 中文 = 1; ui! { <Label> \"hi\" </Label> } }");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].column, 25);
    assert!(blocks[0].body.starts_with("<Label>"));
}

#[test]
fn lost_ack_retries_the_same_transaction_without_a_second_commit() {
    use std::net::TcpListener;
    let root = std::env::temp_dir().join(format!("jadix-hmr-ack-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("source.rs");
    fs::write(&file, r#"fn demo() { ui! { <Text>"new"</Text> } }"#).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let mut gate = jadix_hot_reload::SessionGate::new(jadix_hot_reload::SessionInfo::new(
            "lost-ack".into(),
        ));
        let mut commits = 0;
        let mut first = None;
        for attempt in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                reader.get_mut(),
                "{}",
                serde_json::to_string(gate.info()).unwrap()
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let jadix_hot_reload::IncomingMessage::Transaction { transaction } =
                serde_json::from_str(&line).unwrap()
            else {
                panic!("transaction")
            };
            if let Some(first) = &first {
                assert_eq!(&transaction, first);
            } else {
                first = Some(transaction.clone());
            }
            let acknowledgement = gate.apply(&transaction, |_| {
                commits += 1;
                Ok(jadix_hot_reload::CommitStatus::Applied)
            });
            if attempt == 1 {
                writeln!(
                    reader.get_mut(),
                    "{}",
                    serde_json::to_string(&acknowledgement).unwrap()
                )
                .unwrap();
            }
        }
        assert_eq!(commits, 1);
    });
    let mut cache = HashMap::new();
    let mut client = DevClient::default();
    assert!(send_dev_batch(&address, &[file.clone()], &[], &[], &mut cache, &mut client).is_err());
    assert!(cache.is_empty());
    assert!(client.pending.is_some());
    assert!(matches!(
        send_dev_batch(&address, &[file.clone()], &[], &[], &mut cache, &mut client),
        Ok(SendOutcome::Sent { .. })
    ));
    assert!(!cache.is_empty());
    assert!(client.pending.is_none());
    server.join().unwrap();
    fs::remove_file(file).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn stylesheet_content_changes_at_the_same_path_are_not_duplicates() {
    use std::net::TcpListener;
    let root = std::env::temp_dir().join(format!("jadix-hmr-css-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("theme.css");
    fs::write(&file, ".card { padding: 4px; }").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let mut gate =
            jadix_hot_reload::SessionGate::new(jadix_hot_reload::SessionInfo::new("css".into()));
        for attempt in 0..3 {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                reader.get_mut(),
                "{}",
                serde_json::to_string(gate.info()).unwrap()
            )
            .unwrap();
            line.clear();
            let length = reader.read_line(&mut line).unwrap();
            if attempt == 2 {
                assert_eq!(length, 0);
                break;
            }
            let jadix_hot_reload::IncomingMessage::Transaction { transaction } =
                serde_json::from_str(&line).unwrap()
            else {
                panic!("transaction")
            };
            let acknowledgement = gate.apply(&transaction, |_| {
                Ok(jadix_hot_reload::CommitStatus::Applied)
            });
            assert_eq!(acknowledgement.revision, attempt + 1);
            writeln!(
                reader.get_mut(),
                "{}",
                serde_json::to_string(&acknowledgement).unwrap()
            )
            .unwrap();
        }
    });
    let mut cache = HashMap::new();
    let mut client = DevClient::default();
    assert!(matches!(
        send_dev_batch(&address, &[], &[], &[file.clone()], &mut cache, &mut client),
        Ok(SendOutcome::Sent { .. })
    ));
    fs::write(&file, ".card { padding: 8px; }").unwrap();
    assert!(matches!(
        send_dev_batch(&address, &[], &[], &[file.clone()], &mut cache, &mut client),
        Ok(SendOutcome::Sent { .. })
    ));
    assert!(matches!(
        send_dev_batch(&address, &[], &[], &[file.clone()], &mut cache, &mut client),
        Ok(SendOutcome::Duplicate)
    ));
    server.join().unwrap();
    fs::remove_file(file).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn binary_image_changes_at_the_same_path_are_sent_and_deduplicated() {
    use std::net::TcpListener;
    let root = std::env::temp_dir().join(format!("jadix-hmr-image-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("image.png");
    fs::write(&file, [0xff, 0x00, 0x80, 0x10]).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let mut gate =
            jadix_hot_reload::SessionGate::new(jadix_hot_reload::SessionInfo::new("css".into()));
        for attempt in 0..3 {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                reader.get_mut(),
                "{}",
                serde_json::to_string(gate.info()).unwrap()
            )
            .unwrap();
            line.clear();
            let length = reader.read_line(&mut line).unwrap();
            if attempt == 2 {
                assert_eq!(length, 0);
                break;
            }
            let jadix_hot_reload::IncomingMessage::Transaction { transaction } =
                serde_json::from_str(&line).unwrap()
            else {
                panic!("transaction")
            };
            let acknowledgement = gate.apply(&transaction, |_| {
                Ok(jadix_hot_reload::CommitStatus::Applied)
            });
            assert_eq!(acknowledgement.revision, attempt + 1);
            writeln!(
                reader.get_mut(),
                "{}",
                serde_json::to_string(&acknowledgement).unwrap()
            )
            .unwrap();
        }
    });
    let mut cache = HashMap::new();
    let mut client = DevClient::default();
    assert!(matches!(
        send_dev_batch(&address, &[], &[file.clone()], &[], &mut cache, &mut client),
        Ok(SendOutcome::Sent { .. })
    ));
    fs::write(&file, [0xff, 0x00, 0x80, 0x20]).unwrap();
    assert!(matches!(
        send_dev_batch(&address, &[], &[file.clone()], &[], &mut cache, &mut client),
        Ok(SendOutcome::Sent { .. })
    ));
    assert!(matches!(
        send_dev_batch(&address, &[], &[file.clone()], &[], &mut cache, &mut client),
        Ok(SendOutcome::Duplicate)
    ));
    server.join().unwrap();
    fs::remove_file(file).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn failed_send_keeps_cache_and_fingerprint_for_retry() {
    use std::net::TcpListener;
    let root = std::env::temp_dir().join(format!("jadix-hmr-retry-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("source.rs");
    fs::write(&file, r#"fn demo() { ui! { <Label> "old" </Label> } }"#).unwrap();
    let files = vec![file.clone()];
    let mut cache = HashMap::new();
    build_template_patch_msg(&files, &mut cache);
    let key = normalize_path_for_id(&file);
    let old_hash = cache[&key][0].hash.clone();
    fs::write(&file, r#"fn demo() { ui! { <Label> "new" </Label> } }"#).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut fingerprint = DevClient::default();
    assert!(send_dev_batch(
        &address.to_string(),
        &files,
        &[],
        &[],
        &mut cache,
        &mut fingerprint
    )
    .is_err());
    assert_eq!(cache[&key][0].hash, old_hash);
    assert!(fingerprint.last_payload.is_none());
    let listener = TcpListener::bind(address).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        assert!(request.contains("hello"));
        let session = jadix_hot_reload::SessionInfo::new("retry-build".into());
        let mut response = serde_json::to_vec(&session).unwrap();
        response.push(b'\n');
        reader.get_mut().write_all(&response).unwrap();
        request.clear();
        reader.read_line(&mut request).unwrap();
        assert!(request.contains("new"));
        let jadix_hot_reload::IncomingMessage::Transaction { transaction } =
            serde_json::from_str(&request).unwrap()
        else {
            panic!("transaction")
        };
        let mut gate = jadix_hot_reload::SessionGate::new(session);
        let acknowledgement = gate.apply(&transaction, |_| {
            Ok(jadix_hot_reload::CommitStatus::Applied)
        });
        let mut response = serde_json::to_vec(&acknowledgement).unwrap();
        response.push(b'\n');
        reader.get_mut().write_all(&response).unwrap();
    });
    assert!(matches!(
        send_dev_batch(
            &address.to_string(),
            &files,
            &[],
            &[],
            &mut cache,
            &mut fingerprint
        ),
        Ok(SendOutcome::Sent { templates: 1, .. })
    ));
    server.join().unwrap();
    assert_ne!(cache[&key][0].hash, old_hash);
    assert!(fingerprint.last_payload.is_some());
    assert!(matches!(
        send_dev_batch(
            &address.to_string(),
            &files,
            &[],
            &[],
            &mut cache,
            &mut fingerprint
        ),
        Ok(SendOutcome::Empty)
    ));
    fs::remove_file(file).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn tracks_gallery_theme_and_styles_not_cargo_toml() {
    let root = Path::new(".");
    assert!(is_stylesheet_name(Path::new("examples/gallery/theme.toml")));
    assert!(is_stylesheet_name(Path::new("styles.css")));
    assert!(!is_stylesheet_name(Path::new("Cargo.toml")));
    assert!(is_tracked_stylesheet(
        Path::new("examples/gallery/styles.toml"),
        root
    ));
    assert!(!is_tracked_stylesheet(Path::new("Cargo.toml"), root));
}

#[test]
fn normalize_path_for_id_uses_forward_slashes() {
    let p = Path::new("examples")
        .join("gallery")
        .join("src")
        .join("main.rs");
    let key = normalize_path_for_id(&p);
    assert!(!key.contains('\\'));
}

#[test]
fn normalize_path_text_strips_windows_verbatim_prefixes() {
    assert_eq!(
        normalize_path_text(Path::new(r"\\?\C:\workspace\src\main.rs")),
        "C:/workspace/src/main.rs"
    );
    assert_eq!(
        normalize_path_text(Path::new(r"\\?\UNC\server\share\src\main.rs")),
        "//server/share/src/main.rs"
    );
    assert_eq!(
        normalize_path_text(Path::new(r"\\server\share\src\main.rs")),
        "//server/share/src/main.rs"
    );
}
