use super::*;
use crate::style::{
    parse_color, semantic_chrome, SemanticChromeDecor, SemanticChromeOpts, SemanticSurface,
    SemanticTone,
};

#[test]
fn chrome_disabled_skips_container_paint() {
    let mut el = Element::new();
    set_variant_chrome_enabled(&mut el, false);
    let chrome = semantic_chrome(SemanticChromeOpts {
        tone: SemanticTone::Info,
        filled: false,
        surface: SemanticSurface::Card,
        decor: SemanticChromeDecor::BORDERED,
    });
    el.style.bg_color = parse_color("background");
    apply_semantic_chrome_respecting_overrides(&mut el, &chrome);
    assert_eq!(el.style.bg_color, parse_color("background"));
}

#[test]
fn bg_override_preserved_on_reapply() {
    let mut el = Element::new();
    mark_variant_style_override(&mut el, VariantStyleField::Bg);
    let custom = parse_color("red-500");
    el.style.bg_color = custom;
    let chrome = semantic_chrome(SemanticChromeOpts {
        tone: SemanticTone::Success,
        filled: false,
        surface: SemanticSurface::Card,
        decor: SemanticChromeDecor::BORDERED,
    });
    apply_semantic_chrome_respecting_overrides(&mut el, &chrome);
    assert_eq!(el.style.bg_color, custom);
}
