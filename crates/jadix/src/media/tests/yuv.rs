use super::*;
use crate::media::stream::{MediaFrame, TextureColorSpace};

fn luma_i420(width: u32, height: u32, y: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (cw, ch) = chroma_wh(width, height);
    (
        vec![y; (width * height) as usize],
        vec![128; (cw * ch) as usize],
        vec![128; (cw * ch) as usize],
    )
}

#[test]
fn limited_black_white_gray() {
    assert_eq!(bt709_yuv8_to_rgb(16, 128, 128), [0, 0, 0]);
    let w = bt709_yuv8_to_rgb(235, 128, 128);
    assert!(w.iter().all(|&c| c >= 250), "white {w:?}");
    let g = bt709_yuv8_to_rgb(126, 128, 128);
    assert!(
        (120..140).contains(&g[0]) && (g[0].abs_diff(g[1]) < 3) && (g[1].abs_diff(g[2]) < 3),
        "mid gray {g:?}"
    );
}

#[test]
fn i420_gray_bar_to_rgba() {
    let (y, u, v) = luma_i420(8, 4, 126);
    let frame = i420_frame(8, 4, &y, &u, &v);
    let rgba = media_frame_to_rgba8(&frame);
    #[cfg(feature = "image-stream")]
    {
        let rgba = rgba.expect("i420");
        assert_eq!(rgba.len(), 8 * 4 * 4);
        let px = &rgba[0..4];
        assert_eq!(px[3], 255);
        assert!(
            px[0].abs_diff(px[1]) < 4 && px[1].abs_diff(px[2]) < 4,
            "gray {px:?}"
        );
    }
    #[cfg(not(feature = "image-stream"))]
    {
        assert!(rgba.is_none(), "YUV dropped without image-stream");
    }
}

#[test]
fn nv12_matches_i420_gray() {
    let (y, u_p, v_p) = luma_i420(4, 4, 81);
    let mut uv = vec![0u8; 2 * 2 * 2];
    for i in 0..4 {
        uv[i * 2] = u_p[i];
        uv[i * 2 + 1] = v_p[i];
    }
    let i420 = i420_frame(4, 4, &y, &u_p, &v_p);
    let nv12 = MediaFrame {
        width: 4,
        height: 4,
        layout: PixelLayout::Nv12,
        color: TextureColorSpace::DisplaySrgb,
        pts: None,
        planes: MediaPlanes::Yuv {
            y: &y,
            u: &uv,
            v: &[],
            y_stride: 4,
            uv_stride: 4,
        },
    };
    let a = media_frame_to_rgba8(&i420);
    let b = media_frame_to_rgba8(&nv12);
    #[cfg(feature = "image-stream")]
    {
        assert_eq!(a, b);
    }
    #[cfg(not(feature = "image-stream"))]
    {
        assert!(a.is_none() && b.is_none());
    }
}

#[test]
fn p010_white_is_near_255() {
    let w = 2u32;
    let h = 2u32;
    let y10: u16 = 940u16 << 6;
    let c10: u16 = 512u16 << 6;
    let mut y = vec![0u8; 8];
    for i in 0..4 {
        y[i * 2..i * 2 + 2].copy_from_slice(&y10.to_le_bytes());
    }
    let mut uv = vec![0u8; 8];
    uv[0..2].copy_from_slice(&c10.to_le_bytes());
    uv[2..4].copy_from_slice(&c10.to_le_bytes());
    let frame = MediaFrame {
        width: w,
        height: h,
        layout: PixelLayout::P010,
        color: TextureColorSpace::DisplaySrgb,
        pts: None,
        planes: MediaPlanes::Yuv {
            y: &y,
            u: &uv,
            v: &[],
            y_stride: 4,
            uv_stride: 8,
        },
    };
    let rgba = media_frame_to_rgba8(&frame);
    #[cfg(feature = "image-stream")]
    {
        let rgba = rgba.expect("p010");
        assert!(
            rgba[0] >= 240 && rgba[1] >= 240 && rgba[2] >= 240,
            "{rgba:?}"
        );
    }
    #[cfg(not(feature = "image-stream"))]
    {
        let _ = (w, h);
        assert!(rgba.is_none());
    }
}

#[test]
fn short_i420_is_dropped() {
    let frame = i420_frame(4, 4, &[16u8; 4], &[128], &[128]);
    assert!(media_frame_to_rgba8(&frame).is_none());
}

#[test]
fn rgba_i420_roundtrip_gray() {
    let rgba = vec![
        80u8, 80, 80, 255, 80, 80, 80, 255, 80, 80, 80, 255, 80, 80, 80, 255,
    ];
    let Some((y, u, v)) = rgba8_to_i420(2, 2, &rgba) else {
        return;
    };
    let back = media_frame_to_rgba8(&i420_frame(2, 2, &y, &u, &v));
    #[cfg(feature = "image-stream")]
    {
        let back = back.expect("roundtrip");
        for i in 0..4 {
            assert!(
                back[i * 4].abs_diff(80) < 18,
                "channel drift {:?}",
                &back[i * 4..i * 4 + 3]
            );
        }
    }
    #[cfg(not(feature = "image-stream"))]
    {
        assert!(back.is_none());
    }
}
