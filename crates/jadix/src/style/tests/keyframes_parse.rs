use super::*;
use crate::style::keyframes::{lookup_keyframes, sample_keyframes};

#[test]
fn parses_from_to_opacity() {
    parse_and_register_keyframes("@keyframes fade-x { from { opacity: 0; } to { opacity: 1; } }")
        .unwrap();
    let stops = lookup_keyframes("fade-x").unwrap();
    let mid = sample_keyframes(&stops, 0.5);
    assert!((mid.opacity.unwrap() - 0.5).abs() < 1e-3);
}

#[test]
fn hot_reload_data_files_are_strict_without_changing_stylesheet_parsing() {
    let stylesheet = ".demo { opacity: 1; } @keyframes stylesheet-compat { from { opacity: 0; } to { opacity: 1; } } .after { opacity: 0; }";
    assert_eq!(
        parse_and_register_keyframes(stylesheet).unwrap(),
        vec!["stylesheet-compat"]
    );
    assert!(lookup_keyframes("stylesheet-compat").is_some());
    assert!(parse_keyframe_definitions(stylesheet).is_err());
    assert!(parse_keyframe_definitions(
        "@keyframes data-only { from { opacity: 0; } to { opacity: 1; } }"
    )
    .is_ok());
}

#[test]
fn hot_reload_invalid_keyframes_do_not_partially_register() {
    let stylesheet = "@keyframes atomic-first { from { opacity: 0; } to { opacity: 1; } } @keyframes atomic-second { invalid% { opacity: 0; } }";
    assert!(parse_and_register_keyframes(stylesheet).is_err());
    assert!(lookup_keyframes("atomic-first").is_none());
    assert!(lookup_keyframes("atomic-second").is_none());
}

#[test]
fn parses_percent_and_color() {
    parse_and_register_keyframes(
            "@keyframes flash-bg { 0% { background-color: #000000; } 100% { background-color: #ffffff; } }",
        )
        .unwrap();
    let stops = lookup_keyframes("flash-bg").unwrap();
    assert!(stops
        .iter()
        .any(|s| s.get(PropertyId::BackgroundColor).is_some()));
}
