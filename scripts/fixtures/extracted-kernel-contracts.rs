#[test]
fn copied_text_snapshot_and_lsp_coordinates_share_source_bytes() {
    let original = jadix_document::TextDocument::new("a😀中\ntext");
    let position = jadix_lsp::position(original.text(), "a😀".len());
    assert_eq!(
        jadix_lsp::byte_at(original.text(), &position),
        Some("a😀".len())
    );
    let edited = original.replace(1.."a😀".len(), "ok").unwrap();
    assert_eq!(edited.text(), "aok中\ntext");
    assert_eq!(original.text(), "a😀中\ntext");
}

#[cfg(feature = "expanded")]
#[test]
fn copied_optional_diff_and_highlighting_preserve_source() {
    let diff = jadix_document::DiffDocument::new("old", "new");
    assert!(!diff.is_identical());
    let source = "fn main() { let value = 42; }\n";
    let highlighted = jadix_document::syntax::highlight_code(source, "rs", false);
    assert_eq!(
        highlighted
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        source
    );
}

#[test]
fn copied_core_layout_theme_and_receiver_do_not_need_gui() {
    let commands = jadix_core::path::decode_path_d("M0 0 L10 0");
    assert_eq!(commands.len(), 2);
    assert!(jadix_layout::css_root_font_size_px() > 0.0);
    assert_eq!(
        jadix_window_materials::WindowBackdrop::from("mica"),
        jadix_window_materials::WindowBackdrop::Mica
    );
    let theme = jadix_theme::get_theme();
    assert!(theme
        .colors
        .background
        .iter()
        .all(|value| value.is_finite()));
    assert!(matches!(
        jadix_hot_reload::parse_command("reload"),
        Some(jadix_hot_reload::HotReloadCommand::Reload)
    ));
    jadix_animation::clock::set_css_transition_synthetic_clock(true);
    jadix_animation::clock::advance_css_transition_timeline_ms(12.0);
    assert_eq!(jadix_animation::clock::css_timeline_now_ms(), 12.0);
    jadix_animation::clock::set_css_transition_synthetic_clock(false);
}

#[cfg(feature = "expanded")]
#[test]
fn copied_animation_path_and_document_semantics_share_public_types() {
    use jadix_core::path::PathCommand;
    let morph = jadix_animation::path::PathMorph::new(
        &[PathCommand::MoveTo((0.0, 0.0))],
        &[PathCommand::MoveTo((10.0, 20.0))],
    )
    .unwrap();
    assert_eq!(morph.sample(0.5), vec![PathCommand::MoveTo((5.0, 10.0))]);
    let document = jadix_document::Document::markdown("| a |\n| :-: |\n| b |\n");
    let row = document
        .blocks()
        .iter()
        .find(|block| {
            matches!(
                block.kind,
                jadix_document::DocumentBlockKind::TableRow { .. }
            )
        })
        .unwrap();
    assert_eq!(row.alignments[0], jadix_document::DocumentAlignment::Center);
}
