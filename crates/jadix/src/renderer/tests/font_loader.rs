use super::*;

#[test]
fn registered_only_database_has_all_generic_defaults() {
    let mut db = Database::new();
    db.load_font_data(
        include_bytes!("../../../../jadix_cosmic_text/fonts/InterVariable.ttf").to_vec(),
    );
    apply_default_families(&mut db);
    let mut fs = FontSystem::new_with_locale_and_db("en-US".into(), db);
    for family in [
        glyphon::Family::SansSerif,
        glyphon::Family::Serif,
        glyphon::Family::Monospace,
    ] {
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(16.0, 20.0));
        buffer.set_text(
            "Registered font",
            &glyphon::Attrs::new().family(family),
            glyphon::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fs, false);
        assert!(buffer.layout_runs().next().is_some());
    }
}

#[test]
fn warm_coverage_does_not_wait_for_another_threads_font_load() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let mut fs = create_font_system();
    ensure_coverage_for_text(&mut fs, "中文");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let loader = std::thread::spawn(move || {
        with_lazy_state(|_state| {
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
        });
    });
    ready_rx.recv().unwrap();
    let start = Instant::now();
    ensure_coverage_for_text(&mut fs, "中文");
    let elapsed = start.elapsed();
    let _ = release_tx.send(());
    loader.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(400),
        "warm coverage blocked for {elapsed:?}"
    );
}

#[test]
fn unix_symbol_fallback_preserves_warm_font_caches() {
    let needs = ScriptNeeds {
        symbols: true,
        ..Default::default()
    };
    let mut source = create_font_system();
    ensure_scripts(&mut source, needs);
    // Reuse a real face but project Unix's fallback family into its fontdb
    // metadata, so this regression also runs on Windows build hosts.
    let mut face = source.db().faces().next().expect("UI font").clone();
    let language = face.families[0].1;
    face.families = vec![("DejaVu Sans".into(), language)];
    let mut db = Database::new();
    db.push_face_info(face);
    let mut fs = FontSystem::new_with_locale_and_db("en-US".into(), db);
    let attrs = glyphon::Attrs::new().family(glyphon::Family::Name("DejaVu Sans"));
    let matches = fs.get_font_matches(&attrs);
    let faces = fs.db().len();
    for _ in 0..100 {
        ensure_coverage_for_text(&mut fs, "∑ → ★");
        assert!(
            std::sync::Arc::ptr_eq(&matches, &fs.get_font_matches(&attrs)),
            "an already loaded Unix symbol fallback must not clear font matching every frame"
        );
        assert_eq!(
            fs.db().len(),
            faces,
            "must not inject duplicate fallback faces"
        );
    }
}

#[test]
fn shared_font_file_is_not_loaded_again() {
    let path = crate::platform::eager_ui_font_files()
        .iter()
        .find_map(|name| crate::platform::resolve_system_font(name))
        .expect("system UI font");
    let bytes = std::sync::Arc::new(std::fs::read(&path).unwrap());
    let mut db = Database::new();
    db.load_font_source(glyphon::fontdb::Source::SharedFile(path.clone(), bytes));
    let faces = db.len();
    assert!(faces > 0);
    assert!(try_load_font_path(
        &mut db,
        &path,
        &mut LazyFontState::default()
    ));
    assert_eq!(
        db.len(),
        faces,
        "cosmic-text's shared file source must remain deduplicated"
    );
}

#[test]
fn deferred_emoji_coverage_preserves_font_match_cache() {
    if env_flag_enabled("JADIX_LOAD_SYSTEM_EMOJI") {
        return;
    }
    let mut fs = create_font_system();
    let attrs = glyphon::Attrs::new().family(glyphon::Family::SansSerif);
    let matches = fs.get_font_matches(&attrs);
    for _ in 0..3 {
        ensure_scripts(
            &mut fs,
            ScriptNeeds {
                emoji: true,
                ..Default::default()
            },
        );
        assert!(
            std::sync::Arc::ptr_eq(&matches, &fs.get_font_matches(&attrs)),
            "deferred emoji coverage must not invalidate font matching"
        );
    }
}

#[test]
fn ensure_coverage_injects_cjk_into_each_font_system() {
    let mut layout_fs = create_font_system();
    let mut render_fs = create_font_system();
    ensure_coverage_for_text(&mut layout_fs, "中文");
    ensure_coverage_for_text(&mut render_fs, "日本語");
    assert!(
        cjk_coverage_present(layout_fs.db()),
        "layout FontSystem missing CJK after ensure_coverage"
    );
    assert!(
            cjk_coverage_present(render_fs.db()),
            "render FontSystem missing CJK after ensure_coverage (global loaded_paths short-circuit regression)"
        );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_eager_opens_times_new_roman_when_present() {
    let path = std::path::PathBuf::from(r"C:\Windows\Fonts\times.ttf");
    if !path.is_file() {
        return;
    }
    let fs = create_font_system();
    assert!(
        fs.db()
            .faces()
            .any(|face| face.families.iter().any(|(f, _)| f == "Times New Roman")),
        "times.ttf is on disk but Times New Roman is missing from fontdb"
    );
}
