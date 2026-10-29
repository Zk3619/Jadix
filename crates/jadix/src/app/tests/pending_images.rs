use super::*;

#[test]
#[cfg(feature = "images")]
fn hot_reload_image_replacement_keeps_cached_pixels_on_decode_failure() {
    let path = std::env::temp_dir().join(format!("jadix-hr-image-{}.png", std::process::id()));
    let source = path.to_string_lossy().into_owned();
    let key = texture_key_for_src(&source);
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255u8, 0, 0, 255]))
        .save(&path)
        .unwrap();
    assert!(queue_image_from_src(&key, &source));
    let _ = take_pending_key(&key);
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0u8, 255, 0, 255]))
        .save(&path)
        .unwrap();
    commit_local_reload(prepare_local_reload(&path).unwrap());
    let (rgba, width, height) = take_pending_key(&key).unwrap();
    assert_eq!((width, height), (2, 2));
    assert_eq!(&rgba[..4], &[0, 255, 0, 255]);
    std::fs::write(&path, b"invalid image").unwrap();
    assert!(prepare_local_reload(&path).is_err());
    let cached = DECODED_SRC.lock().unwrap();
    assert_eq!(
        &cached.as_ref().unwrap().get(&key).unwrap().rgba[..4],
        &[0, 255, 0, 255]
    );
    drop(cached);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn circular_alpha_mask_zeros_outside_radius() {
    let mut rgba = vec![0u8; 4 * 8 * 8];
    for px in rgba.chunks_exact_mut(4) {
        px[0..3].fill(255);
        px[3] = 255;
    }
    apply_circular_alpha_mask(&mut rgba, 8, 8);
    assert_eq!(rgba[(4 * 4 + 4) * 4 + 3], 255, "center pixel stays opaque");
    assert_eq!(rgba[3], 0, "corner pixel is transparent");
}

#[test]
fn ensure_avatar_src_loaded_marks_failed_for_missing_file() {
    let key = "jadix-test-avatar-src-fail";
    let state = ensure_avatar_src_loaded(
        key,
        "/nonexistent/jadix-avatar-fail-test.png",
        AvatarImageMask::Circle,
    );
    assert_eq!(state, SrcLoadState::Failed);
}

#[test]
fn ensure_src_loaded_marks_failed_for_missing_file() {
    let key = "jadix-test-src-load-fail";
    let state = ensure_src_loaded(key, "/nonexistent/jadix-avatar-fail-test.png");
    assert_eq!(state, SrcLoadState::Failed);
    assert_eq!(src_load_state_for_key(key), SrcLoadState::Failed);
    assert!(!queue_image_from_src(
        key,
        "/nonexistent/jadix-avatar-fail-test.png"
    ));
}

#[test]
fn ensure_src_loaded_empty_src_is_failed() {
    let state = ensure_src_loaded("jadix-test-src-empty", "");
    assert_eq!(state, SrcLoadState::Failed);
}

#[cfg(not(feature = "remote-asset"))]
#[test]
fn ensure_src_loaded_http_fails_without_feature() {
    let key = "jadix-test-src-http-disabled";
    let state = ensure_src_loaded(key, "https://example.invalid/jadix-test.png");
    assert_eq!(state, SrcLoadState::Failed);
    assert_eq!(src_load_state_for_key(key), SrcLoadState::Failed);
    assert!(!queue_image_from_src(
        key,
        "https://example.invalid/jadix-test.png"
    ));
}

#[cfg(feature = "remote-asset")]
#[test]
fn ensure_src_loaded_remote_uses_installed_fetcher() {
    use crate::app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK};
    use crate::media::encode_png;
    use crate::media::remote::with_remote_fetcher;

    let png = encode_png(&DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![0, 255, 0, 255],
        intrinsic_hotspot: None,
        animation: None,
    })
    .expect("png");
    let url = "https://jadix.test/remote-src.png";
    let key = format!("jadix-test-remote-src:{url}");
    with_remote_fetcher(
        {
            let png = png.clone();
            move |u: &str| {
                assert_eq!(u, url);
                Ok(png.clone())
            }
        },
        || {
            let _ui = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(ensure_src_loaded(&key, url), SrcLoadState::Pending);
            for _ in 0..200 {
                drain_ui_tasks();
                if src_load_state_for_key(&key).is_ready() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            drain_ui_tasks();
            assert_eq!(
                src_load_state_for_key(&key),
                SrcLoadState::Ready,
                "installed fetcher should supply PNG bytes"
            );
        },
    );
}

#[cfg(feature = "remote-asset")]
#[test]
fn ensure_src_loaded_remote_fails_without_fetcher() {
    use crate::app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK};
    use crate::media::remote::{clear_remote_asset_fetcher, FETCHER_TEST_LOCK};

    let _fetch = FETCHER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_remote_asset_fetcher();
    let _ui = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let url = "https://jadix.test/missing-fetcher.png";
    let key = format!("jadix-test-remote-nofetch:{url}");
    assert_eq!(ensure_src_loaded(&key, url), SrcLoadState::Pending);
    for _ in 0..200 {
        drain_ui_tasks();
        if src_load_state_for_key(&key) == SrcLoadState::Failed {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drain_ui_tasks();
    assert_eq!(src_load_state_for_key(&key), SrcLoadState::Failed);
}

#[cfg(feature = "remote-asset")]
#[test]
fn ensure_avatar_src_loaded_remote_uses_installed_fetcher() {
    use crate::app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK};
    use crate::media::encode_png;
    use crate::media::remote::with_remote_fetcher;

    let png = encode_png(&DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![0, 0, 255, 255],
        intrinsic_hotspot: None,
        animation: None,
    })
    .expect("png");
    let url = "https://jadix.test/remote-avatar.png";
    let key = format!("jadix-test-remote-avatar:{url}");
    with_remote_fetcher(
        {
            let png = png.clone();
            move |_: &str| Ok(png.clone())
        },
        || {
            let _ui = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                ensure_avatar_src_loaded(&key, url, AvatarImageMask::Circle),
                SrcLoadState::Pending
            );
            for _ in 0..200 {
                drain_ui_tasks();
                if src_load_state_for_key(&key).is_ready() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            drain_ui_tasks();
            assert_eq!(src_load_state_for_key(&key), SrcLoadState::Ready);
        },
    );
}

#[cfg(feature = "remote-asset-cache")]
#[test]
fn image_and_avatar_share_the_cached_remote_download() {
    use crate::app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK};
    use crate::media::remote::with_remote_fetcher;
    use crate::media::{encode_png, CachedRemoteAssetFetcher};
    use std::{
        num::NonZeroU32,
        sync::{atomic::AtomicUsize, Arc},
        time::Duration,
    };

    let png = encode_png(&DecodedImage {
        width: 1,
        height: 1,
        rgba: vec![0, 255, 0, 255],
        intrinsic_hotspot: None,
        animation: None,
    })
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let cache = CachedRemoteAssetFetcher::new(
        move |_url: &str| {
            observed.fetch_add(1, Ordering::AcqRel);
            Ok(png.clone())
        },
        NonZeroU32::new(4096).unwrap(),
        Duration::from_secs(60),
    );
    with_remote_fetcher(cache, || {
        let _ui = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let url = "https://jadix.test/cached-image-avatar.png";
        let image_key = texture_key_for_src(url);
        let avatar_key = texture_key_for_avatar_src(url, AvatarImageMask::Circle);
        assert_eq!(ensure_src_loaded(&image_key, url), SrcLoadState::Pending);
        assert_eq!(
            ensure_avatar_src_loaded(&avatar_key, url, AvatarImageMask::Circle),
            SrcLoadState::Pending
        );
        for _ in 0..200 {
            drain_ui_tasks();
            if src_load_state_for_key(&image_key).is_ready()
                && src_load_state_for_key(&avatar_key).is_ready()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(src_load_state_for_key(&image_key).is_ready());
        assert!(src_load_state_for_key(&avatar_key).is_ready());
        assert_eq!(calls.load(Ordering::Acquire), 1);
    });
}

#[test]
fn queue_rgba_bumps_dynamic_texture_epoch() {
    let before = dynamic_texture_epoch();
    queue_image_rgba8("jadix-test-tex-epoch", 1, 1, vec![0, 0, 0, 255]);
    assert!(
        dynamic_texture_epoch() > before,
        "same texture key with new pixels must change present fingerprint"
    );
}
