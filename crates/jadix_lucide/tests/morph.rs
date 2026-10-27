use jadix::utils::icon_morph::{canonical_d, icon_input_from_src, morph_src_at, IconInput};

#[test]
fn menu_name_is_not_path_d() {
    jadix_lucide::install().unwrap();
    for source in ["M4 6h16", "m14 12 4 4"] {
        assert!(matches!(icon_input_from_src(source), Some(IconInput::D(_))));
    }
    for name in ["menu", "moon", "x"] {
        let input = icon_input_from_src(name).expect("catalog icon");
        assert!(!matches!(input, IconInput::D(_)));
        assert!(!canonical_d(&input).unwrap().is_empty());
    }
}

#[test]
fn unlock_aliases_lock_open_paths() {
    jadix_lucide::install().unwrap();
    let alias = icon_input_from_src("unlock").expect("unlock alias");
    let canonical = icon_input_from_src("lock-open").expect("lock-open");
    assert_eq!(
        canonical_d(&alias).unwrap(),
        canonical_d(&canonical).unwrap()
    );
}

#[test]
fn lucide_menu_to_x_mid() {
    jadix_lucide::install().unwrap();
    let middle = morph_src_at("menu", "x", 0.5).expect("lucide paths");
    assert!(middle.contains('M') && middle.contains('L'), "{middle}");
    let rest = morph_src_at("menu", "x", 1.0).unwrap();
    let destination = canonical_d(&icon_input_from_src("x").unwrap()).unwrap();
    assert_eq!(rest, destination);
}
