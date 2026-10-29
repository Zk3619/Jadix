#[test]
fn windows_local_hms_in_range() {
    let (h, m, s) = super::local_hms().expect("GetLocalTime");
    assert!(h < 24 && m < 60 && s < 60, "hms=({h},{m},{s})");
}

#[test]
fn enable_ansi_stderr_does_not_panic() {
    super::enable_ansi_stderr();
    super::enable_ansi_stderr();
}
