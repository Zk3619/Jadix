use super::*;
use crate::media::encode_png;

#[test]
fn read_src_bytes_still_rejects_http() {
    assert!(read_src_bytes("https://example.com/a.png").is_none());
    assert!(read_src_bytes("http://127.0.0.1/a.png").is_none());
    assert!(load_src("https://example.com/a.png", DecodeOptions::image()).is_none());
}

#[test]
fn large_png_decodes_without_cap() {
    let big = vec![0u8; 129 * 1 * 4];
    let png = encode_png(&DecodedImage {
        width: 129,
        height: 1,
        rgba: big,
        intrinsic_hotspot: None,
        animation: None,
    })
    .unwrap();
    let d = decode_bytes(&png, DecodeOptions::image()).unwrap();
    assert_eq!((d.width, d.height), (129, 1));
}

#[cfg(feature = "image-bmp")]
#[test]
fn bmp24_roundtrip() {
    let w = 2u32;
    let h = 1u32;
    let stride = (w as usize * 3).div_ceil(4) * 4;
    let off = 54u32;
    let mut bytes = vec![0u8; off as usize + stride];
    bytes[0..2].copy_from_slice(b"BM");
    let file_size = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&file_size.to_le_bytes());
    bytes[10..14].copy_from_slice(&off.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&(w as i32).to_le_bytes());
    bytes[22..26].copy_from_slice(&(h as i32).to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[off as usize] = 0;
    bytes[off as usize + 1] = 0;
    bytes[off as usize + 2] = 255;
    let d = decode_bytes(&bytes, DecodeOptions::image()).unwrap();
    assert_eq!(&d.rgba[0..4], &[255, 0, 0, 255]);
}

#[cfg(feature = "svg")]
#[test]
fn svg_rasterizes_at_intrinsic_size() {
    let big = br#"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="128"><rect width="256" height="128" fill="lime"/></svg>"#;
    let d = decode_bytes(big, DecodeOptions::image()).unwrap();
    assert_eq!((d.width, d.height), (256, 128));
}
