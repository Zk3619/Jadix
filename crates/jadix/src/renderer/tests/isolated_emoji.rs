use super::*;

#[test]
fn isolated_emoji_engine_does_not_load_ui_fonts() {
    let engines = emoji_fallback_engines();
    let families: Vec<_> = engines
        .0
        .db()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.as_str()))
        .collect();
    assert!(
        !families.iter().any(|family| matches!(
            *family,
            "lucide" | "Inter" | "Inter Variable" | "Segoe UI" | "Times New Roman"
        )),
        "isolated emoji loaded unrelated UI faces: {families:?}"
    );
}

#[test]
#[ignore = "cold emoji raster timing and optional pixel capture"]
fn isolated_emoji_cold_raster_cost() {
    for (name, sequence) in [
        ("thumb", "👍"),
        ("wow", "😮"),
        ("party", "🎉"),
        ("family", "👨‍👩‍👧‍👦"),
        ("face", "😂"),
    ] {
        let now = std::time::Instant::now();
        let raster = load_font_fallback_raster(sequence, 36, 400)
            .unwrap_or_else(|| panic!("missing emoji raster for {sequence}"));
        eprintln!(
            "emoji-cold/{name}: ms={:.3} size={}x{}",
            now.elapsed().as_secs_f64() * 1000.0,
            raster.width,
            raster.height
        );
        assert!(raster.rgba.chunks_exact(4).any(|p| p[3] > 0));
        if let Some(dir) = std::env::var_os("JADIX_EMOJI_CAPTURE_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image::save_buffer(
                dir.join(format!("{name}.png")),
                &raster.rgba,
                raster.width.into(),
                raster.height.into(),
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
