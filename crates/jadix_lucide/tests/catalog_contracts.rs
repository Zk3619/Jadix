#[test]
fn catalog_features_are_observable_to_consumers() {
    assert!(jadix_lucide::font_path().is_file());
    assert!(jadix_lucide::glyph("check").is_some());
    assert_eq!(
        jadix_lucide::svg_inner("check").is_some(),
        cfg!(feature = "paths")
    );
    #[cfg(feature = "font")]
    assert!(jadix_lucide::FONT_DATA.len() > 100_000);
}

#[cfg(feature = "ui")]
#[test]
fn default_product_installs_without_a_separate_data_dependency() {
    jadix_lucide::install().unwrap();
    let icon = jadix::utils::icon_named("check");
    assert_eq!(
        icon.text.as_deref(),
        jadix_lucide::glyph("check").as_deref()
    );
    let fonts = jadix::renderer::font_loader::create_font_system();
    assert!(fonts
        .db()
        .faces()
        .any(|face| face.families.iter().any(|(family, _)| family == "lucide")));
    jadix::utils::icon_provider::clear_icon_provider();
}
