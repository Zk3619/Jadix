use super::*;

#[test]
fn native_icon_rejects_invalid_dimensions_and_bytes() {
    for (rgba, w, h) in [
        (vec![], 0, 0),
        (vec![0; 3], 1, 1),
        (vec![0; 8], 1, 1),
        (vec![], u32::MAX, u32::MAX),
    ] {
        assert!(AppIcon::from_rgba(rgba, w, h).is_err());
    }
    assert!(AppIcon::from_bytes(b"invalid PNG").is_err());
}

#[test]
#[cfg(feature = "product-tests")]
fn native_icon_decodes_and_reaches_both_entry_surfaces() {
    let rgba = image::RgbaImage::from_pixel(2, 2, image::Rgba([17, 33, 55, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let icon = AppIcon::from_bytes(bytes.get_ref()).unwrap();
    assert_eq!((icon.width(), icon.height()), (2, 2));
    assert_eq!(&icon.rgba()[..4], &[17, 33, 55, 255]);
    let opts = crate::app::app().icon(icon.clone()).into_window_options();
    assert_eq!(opts.icon.as_ref(), Some(&icon));
    let attrs = opts.to_window_attributes();
    let native = attrs.window_icon.unwrap();
    assert_eq!(
        native.cast_ref::<winit::icon::RgbaIcon>().unwrap().buffer(),
        icon.rgba()
    );
    let shell = crate::components::App(crate::components::AppProps {
        icon: Some(icon.clone()),
        ..Default::default()
    });
    assert_eq!(shell.get_attr("data-slot").map(String::as_str), Some("app"));
}
