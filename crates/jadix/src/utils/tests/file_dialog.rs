use super::*;

#[test]
fn file_dialog_signal_key() {
    assert_eq!(file_dialog_paths_signal_key("avatar"), "avatar__file_paths");
}
