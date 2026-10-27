//! Native TeX math and chemical-equation surfaces. Unsupported input stays visible.

use crate::components::base::{paint_slot, SlotChrome};
use crate::core::tree::Element;

pub fn math_view(source: impl Into<String>) -> MathViewBuilder {
    MathViewBuilder {
        source: source.into(),
        display: false,
        font_size: 18.0,
        chemistry: false,
        chrome: SlotChrome::default(),
    }
}

pub fn chemistry_view(source: impl Into<String>) -> MathViewBuilder {
    let mut builder = math_view(source);
    builder.chemistry = true;
    builder.display = true;
    builder
}

pub struct MathViewBuilder {
    source: String,
    display: bool,
    font_size: f32,
    chemistry: bool,
    chrome: SlotChrome,
}

impl MathViewBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn display(mut self, display: bool) -> Self {
        self.display = display;
        self
    }
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size.clamp(8.0, 96.0);
        self
    }
}

pub(crate) fn render_diagnostic(source: &str, message: &str) -> Element {
    let mut element = Element::new()
        .flex_col()
        .min_w(0.0)
        .py(4.0)
        .child(
            Element::new()
                .text(source)
                .font_family("monospace")
                .white_space("pre-wrap"),
        )
        .child(
            Element::new()
                .text(message)
                .text_size(12.0)
                .text_color("destructive"),
        );
    element.set_attr("data-render-error", message);
    element
}

impl From<MathViewBuilder> for Element {
    fn from(builder: MathViewBuilder) -> Self {
        let latex = if builder.chemistry {
            chemical_equation_to_latex(&builder.source)
        } else {
            Ok(builder.source.clone())
        };
        let mut element = match latex {
            Ok(latex) => render_math(&latex, &builder.source, builder.display, builder.font_size),
            Err(error) => render_diagnostic(&builder.source, error),
        };
        element.set_attr("aria-label", &builder.source);
        element.set_attr("accesskit_role", "image");
        element.set_attr("data-source", &builder.source);
        paint_slot(
            element,
            if builder.chemistry {
                "chemistry-view"
            } else {
                "math-view"
            },
            &builder.chrome,
        )
    }
}

#[cfg(not(feature = "document-math"))]
fn render_math(_latex: &str, source: &str, _display: bool, _size: f32) -> Element {
    render_diagnostic(source, "Math rendering requires the document-math feature")
}

#[cfg(feature = "document-math")]
fn render_math(latex: &str, source: &str, display: bool, size: f32) -> Element {
    let color = crate::style::parse_text_color("foreground");
    let rendered = crate::text::math::render_svg(latex, display, size, color);
    match rendered {
        Ok(rendered) => {
            let mut element = Element::new()
                .w(rendered.width)
                .h(rendered.height)
                .flex_shrink(0.0);
            if !update_formula_texture(&mut element, &rendered.svg, rendered.width, rendered.height)
            {
                return render_diagnostic(
                    source,
                    "Formula SVG could not be decoded within the raster budget",
                );
            }
            element.reactive_sync(move |element, _| {
                let previous = element.image_texture.clone();
                update_formula_texture(element, &rendered.svg, rendered.width, rendered.height);
                if previous != element.image_texture {
                    crate::core::tree::LayoutStatus::StyleChanged
                } else {
                    crate::core::tree::LayoutStatus::Clean
                }
            })
        }
        Err(error) => render_diagnostic(source, &error),
    }
}

#[cfg(feature = "document-math")]
fn update_formula_texture(element: &mut Element, svg: &str, width: f32, height: f32) -> bool {
    let scale = (crate::style::gradient_raster_dpi() * 2.0)
        .min((1_000_000.0 / (width.ceil() * height.ceil())).sqrt());
    let pixel_width = (width * scale).ceil().max(1.0) as u32;
    let pixel_height = (height * scale).ceil().max(1.0) as u32;
    let Some(key) =
        crate::media::svg_raster::ensure_svg_document_texture(svg, pixel_width, pixel_height)
    else {
        return false;
    };
    element.image_texture = Some(key);
    element.set_attr("data-raster-width", pixel_width.to_string());
    element.set_attr("data-raster-height", pixel_height.to_string());
    true
}

pub fn chemical_equation_to_latex(source: &str) -> Result<String, &'static str> {
    let source = source.trim();
    let source = if source.starts_with("\\ce{") && source.ends_with('}') {
        &source[4..source.len() - 1]
    } else {
        source
    };
    let mut output = String::new();
    let mut chars = source.char_indices().peekable();
    let mut following_atom = false;
    while let Some((offset, character)) = chars.next() {
        if source[offset..].starts_with("<=>") || source[offset..].starts_with("<->") {
            output.push_str("\\rightleftharpoons ");
            chars.next();
            chars.next();
            following_atom = false;
        } else if source[offset..].starts_with("->") {
            output.push_str("\\rightarrow ");
            chars.next();
            following_atom = false;
        } else if source[offset..].starts_with("<-") {
            output.push_str("\\leftarrow ");
            chars.next();
            following_atom = false;
        } else if character.is_ascii_alphabetic() {
            let mut token = character.to_string();
            while chars
                .peek()
                .is_some_and(|(_, character)| character.is_ascii_lowercase())
            {
                token.push(chars.next().unwrap().1);
            }
            output.push_str(&format!("\\mathrm{{{token}}}"));
            following_atom = true;
        } else if character.is_ascii_digit() {
            let mut digits = character.to_string();
            while chars
                .peek()
                .is_some_and(|(_, character)| character.is_ascii_digit())
            {
                digits.push(chars.next().unwrap().1);
            }
            if following_atom {
                output.push_str(&format!("_{{{digits}}}"));
            } else {
                output.push_str(&digits);
            }
        } else if character.is_whitespace() {
            output.push_str("\\,");
            following_atom = false;
        } else if matches!(character, '+' | '=' | '(' | ')' | '[' | ']' | '.' | ',') {
            output.push(character);
            following_atom = matches!(character, ')' | ']');
        } else if character == '^' {
            let mut charge = String::new();
            while chars.peek().is_some_and(|(_, character)| {
                character.is_ascii_digit() || matches!(character, '+' | '-')
            }) {
                charge.push(chars.next().unwrap().1);
            }
            if charge.is_empty() {
                return Err("Use ^2- or ^+ for ionic charge");
            }
            output.push_str(&format!("^{{{charge}}}"));
            following_atom = false;
        } else {
            return Err("Unsupported chemistry syntax; supported: formulas, counts, groups, ^charges and reaction arrows");
        }
    }
    Ok(output)
}

crate::__component_contract! {
MathViewProps [];
#[allow(non_snake_case)]
pub fn MathView(
default [] source: String,
plain [] display: Option<bool>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    math_view(source).display(display.unwrap_or(false)).into()
}
}

crate::__component_contract! {
ChemistryViewProps [];
#[allow(non_snake_case)]
pub fn ChemistryView(
default [] source: String,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let view = chemistry_view(source);
    view.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
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
}
