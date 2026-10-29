use super::*;

#[test]
fn chemical_counts_arrows_and_charges_are_explicit() {
    let latex = chemical_equation_to_latex("2 H2 + O2 -> 2 H2O").unwrap();
    assert!(latex.contains("\\mathrm{H}_{2}"));
    assert!(latex.contains("\\rightarrow"));
    assert!(chemical_equation_to_latex("SO4^2-")
        .unwrap()
        .ends_with("^{2-}"));
    assert!(chemical_equation_to_latex("\\unknown{x}").is_err());
}

#[cfg(feature = "document-math")]
#[test]
fn fractions_and_invalid_input_use_real_renderer() {
    let fraction: Element = math_view("\\frac{1}{2}").into();
    assert!(fraction.image_texture.is_some());
    assert!(!fraction.attrs.contains_key("data-render-error"));
    let key = fraction.image_texture.as_ref().unwrap();
    crate::media::svg_raster::flush_svg_textures(|_| false);
    let (pixels, width, height) =
        crate::app::pending_images::take_pending_key(key).expect("decoded formula pixels");
    assert!(width > 0 && height > 0);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 100));
    crate::style::set_gradient_raster_dpi(2.0);
    let sharper: Element = math_view("\\frac{1}{2}").into();
    crate::media::svg_raster::flush_svg_textures(|_| false);
    let (_, sharper_width, sharper_height) =
        crate::app::pending_images::take_pending_key(sharper.image_texture.as_ref().unwrap())
            .unwrap();
    assert!(sharper_width >= width * 2 - 1 && sharper_height >= height * 2 - 1);
    crate::style::set_gradient_raster_dpi(1.0);
    let bad: Element = math_view("\\definitelyunsupported{a}").into();
    assert!(bad.attrs.contains_key("data-render-error"));
    let chemistry: Element = chemistry_view("2 H2 + O2 -> 2 H2O").into();
    assert!(!chemistry.attrs.contains_key("data-render-error"));
}
