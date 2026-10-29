#[test]
fn unix_local_hms_in_range() {
    let got = super::local_hms();
    if cfg!(unix) {
        let (h, m, s) = got.expect("localtime_r");
        assert!(h < 24 && m < 60 && s < 60, "hms=({h},{m},{s})");
    } else {
        assert!(got.is_none());
    }
}

#[test]
fn enable_ansi_is_noop_outside_windows_override() {
    super::enable_ansi_stderr();
}
