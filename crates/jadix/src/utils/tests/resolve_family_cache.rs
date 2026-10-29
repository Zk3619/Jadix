use super::*;
use glyphon::fontdb::Database;

use crate::renderer::font_loader::create_font_system;

#[cfg(feature = "product-tests")]
#[test]
fn lucide_resolves_after_font_load_despite_early_miss_cache() {
    clear_resolve_family_cache();
    let empty = Database::new();
    assert!(matches!(
        resolve_family(&empty, "lucide"),
        Family::SansSerif
    ));

    register_font_data(crate::catalog::FONT_DATA.to_vec()).unwrap();
    let fs = create_font_system();
    assert!(matches!(
        resolve_family(fs.db(), "lucide"),
        Family::Name("lucide")
    ));
}

#[test]
fn resolved_default_sans_respects_user_config_when_present() {
    clear_resolve_family_cache();
    std::env::set_var("JADIX_FONT_FAMILY", "Segoe UI");
    let fs = create_font_system();
    let sans = resolved_default_sans_family(fs.db());
    assert_eq!(sans, "Segoe UI");
    std::env::remove_var("JADIX_FONT_FAMILY");
    clear_resolve_family_cache();
}

#[test]
fn serif_stack_does_not_collapse_to_sans_when_serif_face_loaded() {
    clear_resolve_family_cache();
    let fs = create_font_system();
    let db = fs.db();
    let sans = resolved_default_sans_family(db);
    let serif = resolved_default_serif_family(db);
    let serif_loaded = db
        .faces()
        .any(|face| face.families.iter().any(|(f, _)| f == serif));
    if !serif_loaded {
        return;
    }
    assert_ne!(
        sans, serif,
        "generic serif must resolve to a different family than UI sans (got {sans})"
    );
    match resolve_family(db, "Times New Roman, Georgia, serif") {
        Family::Name(name) => {
            assert_ne!(name, sans, "Times/Georgia stack must not land on {sans}");
        }
        Family::Serif => {
            assert_ne!(serif, sans);
        }
        other => panic!("expected named serif or Family::Serif, got {other:?}"),
    }
}

#[test]
fn css_fonts4_weight_matching_examples() {
    let segoe = [400_u16, 600, 700];
    assert_eq!(css_fonts4_match_weight(400, &segoe), Some(400));
    assert_eq!(css_fonts4_match_weight(500, &segoe), Some(400));
    assert_eq!(css_fonts4_match_weight(600, &segoe), Some(600));
    assert_eq!(css_fonts4_match_weight(700, &segoe), Some(700));
    assert_eq!(css_fonts4_match_weight(800, &segoe), Some(700));
    assert_eq!(css_fonts4_match_weight(300, &segoe), Some(400));
    let only_reg = [400_u16];
    assert_eq!(css_fonts4_match_weight(500, &only_reg), Some(400));
    assert_eq!(css_fonts4_match_weight(700, &only_reg), Some(400));
    let gap = [400_u16, 700];
    assert_eq!(css_fonts4_match_weight(600, &gap), Some(700));
}

#[test]
fn segoe_medium_uses_regular_plus_synthetic_bold() {
    let fs = create_font_system();
    let family = crate::renderer::text_hit::best_font_family(&fs);
    let resolved = resolve_face_weight(fs.db(), family, 500);
    if resolved.variable {
        assert_eq!(resolved.attrs_weight, 500);
        assert!(!resolved.synthetic_bold);
    } else {
        assert!(
            resolved.attrs_weight <= 500,
            "CSS match for 500 should not jump past Medium (got {})",
            resolved.attrs_weight
        );
        if resolved.attrs_weight < 500 {
            assert!(
                resolved.synthetic_bold,
                "no Medium face → font-synthesis: weight"
            );
        }
    }
    let bold = resolve_face_weight(fs.db(), family, 700);
    assert!(
        bold.attrs_weight >= 600,
        "Bold should land on a real bolder face when loaded (got {})",
        bold.attrs_weight
    );
}

#[test]
fn text_defaults_size_matches_css_root_slot() {
    let d = text_defaults();
    assert!((d.size_px - root_font_size_px()).abs() < 1e-6);
    assert!((d.size_px - crate::layout::dimension::css_root_font_size_px()).abs() < 1e-6);
}
