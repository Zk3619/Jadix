use super::*;

#[test]
fn detects_http_and_https() {
    assert!(is_remote_src("https://example.com/a.png"));
    assert!(is_remote_src(" HTTP://Example.COM/a.png "));
    assert!(!is_remote_src("data:image/png;base64,xx"));
    assert!(!is_remote_src("assets/a.png"));
    assert!(!is_remote_src("file:///tmp/a.png"));
    assert!(!is_remote_src(""));
}

#[test]
fn feature_flag_matches_cfg() {
    assert_eq!(remote_asset_enabled(), cfg!(feature = "remote-asset"));
}

#[test]
fn fetch_without_fetcher_errors() {
    let _guard = FETCHER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_remote_asset_fetcher();
    let err = fetch_remote_bytes("https://example.com/a.png").unwrap_err();
    assert!(err.contains("RemoteAssetFetcher"));
}

#[test]
fn installed_closure_is_the_client() {
    with_remote_fetcher(
        |url: &str| {
            assert_eq!(url, "https://cdn.example/a.png");
            Ok(b"PNG".to_vec())
        },
        || {
            assert!(remote_asset_fetcher_installed());
            assert_eq!(
                fetch_remote_bytes("https://cdn.example/a.png").unwrap(),
                b"PNG"
            );
        },
    );
    assert!(!remote_asset_fetcher_installed());
}
