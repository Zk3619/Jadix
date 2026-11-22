use resvg::{tiny_skia, usvg};
use std::{ffi::c_void, io::Read, slice, str};

const MAX_SVG_BYTES: usize = 1024 * 1024;
const MAX_EDGE: f32 = 4096.0;
const MAX_PIXELS: f32 = 4.0 * 1024.0 * 1024.0;

thread_local! {
    static OPTIONS: usvg::Options<'static> = {
        let mut options = usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        let resolve = usvg::ImageHrefResolver::default_data_resolver();
        options.image_href_resolver.resolve_data = Box::new(move |mime, data, options| {
            if !matches!(mime, "image/jpg" | "image/jpeg" | "image/png" | "image/gif" | "image/webp")
                || data.len() > MAX_SVG_BYTES { return None; }
            let size = imagesize::blob_size(&data).ok()?;
            if size.width > 16_384 || size.height > 16_384
                || size.width.checked_mul(size.height)? > 64 * 1024 * 1024 {
                return None;
            }
            // Only declared raster MIME types may embed pixels. External and
            // nested SVG references never bypass the host's resource policy.
            resolve(mime, data, options)
        });
        options
    };
}

pub(crate) fn source_bytes(bytes: &[u8]) -> Option<std::borrow::Cow<'_, [u8]>> {
    if bytes.is_empty() || bytes.len() > MAX_SVG_BYTES { return None; }
    let mut expanded = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_SVG_BYTES as u64 + 1)
            .read_to_end(&mut expanded)
            .ok()?;
        if expanded.len() > MAX_SVG_BYTES {
            return None;
        }
        Some(std::borrow::Cow::Owned(expanded))
    } else {
        Some(std::borrow::Cow::Borrowed(bytes))
    }
}

pub(crate) fn rasterize(
    bytes: &[u8],
    target_width: u32,
    target_height: u32,
    fit: u8,
) -> Option<(tiny_skia::Pixmap, usvg::Size)> {
    if target_width == 0 || target_height == 0 { return None; }
    let expanded = source_bytes(bytes)?;
    let bytes = expanded.as_ref();
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let source = str::from_utf8(bytes).ok()?;
    if !source.trim_start().starts_with('<') {
        return None;
    }
    let tree = OPTIONS.with(|options| usvg::Tree::from_str(source, options).ok())?;
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if width > 16_384.0 || height > 16_384.0 || width * height > 64.0 * 1024.0 * 1024.0 {
        return None;
    }
    let scale_x = target_width as f32 / width;
    let scale_y = target_height as f32 / height;
    let fit_scale = match fit {
        0 => scale_x.min(scale_y),
        1 => scale_x.max(scale_y),
        2 => scale_x.min(scale_y).min(1.0),
        3 => 1.0,
        _ => return None,
    };
    let scale = fit_scale
        .min(MAX_EDGE / width.max(height))
        .min((MAX_PIXELS / (width * height)).sqrt());
    let out_width = (width * scale).floor().clamp(1.0, MAX_EDGE) as u32;
    let out_height = (height * scale).floor().clamp(1.0, MAX_EDGE) as u32;
    let scale_x = out_width as f32 / width;
    let scale_y = out_height as f32 / height;
    if !scale_x.is_finite() || !scale_y.is_finite() {
        return None;
    }
    let mut pixmap = tiny_skia::Pixmap::new(out_width, out_height)?;
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some((pixmap, size))
}

#[cfg(test)]
mod tests {
    use super::rasterize;
    use std::io::Write;

    #[test]
    fn renders_svg_to_premultiplied_pixels_at_the_requested_fit() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4"><rect width="8" height="4" fill="red" opacity="0.5"/></svg>"#;
        let (contained, size) = rasterize(svg, 24, 24, 0).unwrap();
        let (covered, _) = rasterize(svg, 24, 24, 1).unwrap();
        assert_eq!((size.width(), size.height()), (8.0, 4.0));
        assert_eq!((contained.width(), contained.height()), (24, 12));
        assert_eq!((covered.width(), covered.height()), (48, 24));
        assert_eq!(&contained.data()[..4], &[128, 0, 0, 128]);
        assert!(rasterize(b"<not-svg/>", 24, 24, 0).is_none());
    }

    #[test]
    fn svgz_expansion_is_bounded_and_raster_references_require_an_allowed_mime() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="red"/></svg>"#;
        let compress = |bytes: &[u8]| {
            let mut gzip =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(bytes).unwrap();
            gzip.finish().unwrap()
        };
        assert_eq!(
            rasterize(&compress(svg), 1, 1, 0).unwrap().0.data(),
            &[255, 0, 0, 255]
        );
        assert!(rasterize(&compress(&vec![b' '; super::MAX_SVG_BYTES + 1]), 1, 1, 0).is_none());
        assert!(rasterize(&[0x1f, 0x8b, 0], 1, 1, 0).is_none());

        use base64::Engine;
        let mut pixels = resvg::tiny_skia::Pixmap::new(1, 1).unwrap();
        pixels.fill(resvg::tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let png = base64::engine::general_purpose::STANDARD.encode(pixels.encode_png().unwrap());
        for (mime, expected) in [("image/png", 255), ("text/plain", 0), ("image/svg+xml", 0)] {
            let source = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><image width="1" height="1" href="data:{mime};base64,{png}"/></svg>"#
            );
            let (image, _) = rasterize(source.as_bytes(), 1, 1, 0).unwrap();
            assert_eq!(image.data()[3], expected, "MIME {mime}");
        }
        for href in [
            "https://example.com/image.png",
            "file:///image.png",
            "relative.png",
        ] {
            let source = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><image width="1" height="1" href="{href}"/></svg>"#
            );
            assert_eq!(
                rasterize(source.as_bytes(), 1, 1, 0).unwrap().0.data()[3],
                0
            );
        }
    }
}

type RasterCallback = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, f32, f32);

/// Rasterizes one UTF-8 SVG and borrows the resulting premultiplied RGBA8
/// pixels for a synchronous callback.
///
/// # Safety
/// `bytes` must point to `len` readable bytes. The callback and its context
/// must be valid for the duration of the call. The callback must copy pixels
/// it needs after returning.
#[no_mangle]
pub unsafe extern "C" fn jadix_native_rasterize_svg(
    bytes: *const u8,
    len: usize,
    target_width: u32,
    target_height: u32,
    fit: u8,
    context: *mut c_void,
    callback: RasterCallback,
) {
    if bytes.is_null() || len == 0 || len > MAX_SVG_BYTES || target_width == 0 || target_height == 0
    {
        return;
    }
    let bytes = unsafe { slice::from_raw_parts(bytes, len) };
    let Some((pixels, size)) = rasterize(bytes, target_width, target_height, fit) else {
        return;
    };
    unsafe {
        callback(
            context,
            pixels.data().as_ptr(),
            pixels.width(),
            pixels.height(),
            size.width(),
            size.height(),
        );
    }
}
