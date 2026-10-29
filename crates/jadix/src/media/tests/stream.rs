use super::*;
use crate::app::pending_images::take_pending_key;

#[test]
fn rgba8_helper_enqueues() {
    let px = [10u8, 20, 30, 255];
    queue_media_frame("jadix-test-media-rgba", MediaFrame::rgba8(1, 1, &px));
    let got = take_pending_key("jadix-test-media-rgba");
    assert_eq!(
        got.as_ref().map(|(b, w, h)| (b.as_slice(), *w, *h)),
        Some((px.as_slice(), 1, 1))
    );
}

#[test]
fn short_buffer_is_dropped() {
    queue_media_frame(
        "jadix-test-media-short",
        MediaFrame::rgba8(2, 2, &[0, 0, 0, 255]),
    );
    assert!(
        take_pending_key("jadix-test-media-short").is_none(),
        "short buffer must not enqueue"
    );
}

#[test]
fn zero_size_is_dropped() {
    queue_media_frame("jadix-test-media-zero", MediaFrame::rgba8(0, 1, &[]));
    assert!(take_pending_key("jadix-test-media-zero").is_none());
}

#[test]
fn yuv_i420_feature_gate() {
    let y = [126u8; 4];
    let u = [128u8; 1];
    let v = [128u8; 1];
    queue_media_frame("jadix-test-media-i420", MediaFrame::i420(2, 2, &y, &u, &v));
    let got = take_pending_key("jadix-test-media-i420");
    #[cfg(feature = "image-stream")]
    {
        let (bytes, w, h) = got.expect("i420 uploads when image-stream is on");
        assert_eq!((w, h), (2, 2));
        assert_eq!(bytes.len(), 16);
        assert_eq!(bytes[3], 255);
    }
    #[cfg(not(feature = "image-stream"))]
    {
        assert!(got.is_none(), "YUV must drop without image-stream");
    }
}
