use super::*;

#[test]
fn spike_api_returns_empty_until_wired() {
    assert!(glyph_outline_to_path_commands(&[], 0, 16.0).is_empty());
    assert!(!path_commands_ready_for_gpu2d(&[]));
}
